//! How SteamDrop opens from Steam, and its two views: the desktop window ("main") and the
//! full-screen, controller-first Couch mode ("couch").
//!
//! Steam starts `SteamDrop.exe --from-steam`. That process picks a view (Big Picture means Couch mode,
//! desktop Steam means the window, unless the setting says otherwise), asks the SteamDrop app to open
//! it (starting the app if needed), then waits while any SteamDrop window is open, so Steam shows
//! SteamDrop as running for exactly that long. The app holds `steam-session.lock` exclusively while
//! a window exists; Windows releases it even if the app crashes.

use crate::state::App;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const MAIN: &str = "main";
pub const COUCH: &str = "couch";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    Window,
    Couch,
}

/// Steam sets SteamTenfoot=1 for games started from Big Picture; a Steam Deck sets SteamDeck=1.
pub fn started_in_big_picture() -> bool {
    std::env::var("SteamTenfoot").is_ok_and(|v| v == "1") || std::env::var("SteamDeck").is_ok_and(|v| v == "1")
}

/// The view to open from Steam, from the "When opened from Steam" setting.
pub fn view_for(setting: &str, big_picture: bool) -> View {
    match setting {
        "couch" => View::Couch,
        "window" => View::Window,
        _ if big_picture => View::Couch,
        _ => View::Window,
    }
}

/// `--open-view=couch|window` (and the older `--couch-open`) on SteamDrop's command line.
pub fn parse_view<S: AsRef<str>>(args: &[S]) -> Option<View> {
    args.iter().find_map(|a| match a.as_ref() {
        "--open-view=couch" | "--couch-open" => Some(View::Couch),
        "--open-view=window" => Some(View::Window),
        _ => None,
    })
}

#[cfg(windows)]
fn open_lock() -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = crate::state::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(0).open(dir.join("steam-session.lock"))
}

/// `SteamDrop.exe --from-steam`: open the right view in the app, then wait until every window is closed.
#[cfg(windows)]
pub fn run_from_steam() {
    use std::time::{Duration, Instant};
    let Ok(exe) = std::env::current_exe() else { return };
    let big_picture = started_in_big_picture();
    let setting = crate::state::load(&crate::state::data_dir()).settings.steam_view;
    let view = view_for(&setting, big_picture);
    let mut child = std::process::Command::new(exe);
    child.arg(if view == View::Couch { "--open-view=couch" } else { "--open-view=window" });
    if big_picture {
        child.arg("--big-picture");
    }
    if child.spawn().is_err() {
        return;
    }
    let start = Instant::now();
    let mut seen_open = false;
    loop {
        // shortcut: checks the lock twice a second; a named event would avoid that, if it ever matters.
        std::thread::sleep(Duration::from_millis(500));
        // Error 32 = sharing violation: the app holds the lock, so a SteamDrop window is open.
        let open = matches!(open_lock(), Err(e) if e.raw_os_error() == Some(32));
        if open {
            seen_open = true;
        } else if seen_open || start.elapsed() > Duration::from_secs(60) {
            return;
        }
    }
}

/// Holds the session lock while any SteamDrop window exists (`closing` is about to go away).
pub fn sync_lock(app: &AppHandle, closing: Option<&str>) {
    let any = [MAIN, COUCH].iter().any(|l| Some(*l) != closing && app.get_webview_window(l).is_some());
    let state = app.state::<App>();
    let mut lock = state.couch_lock.lock().unwrap();
    if !any {
        lock.take();
    } else if lock.is_none() {
        #[cfg(windows)]
        {
            *lock = open_lock().ok();
        }
    }
}

/// Opens (or brings back) a view.
pub fn open(app: &AppHandle, view: View) {
    if view == View::Window {
        crate::show_window(app);
        return;
    }
    if let Some(w) = app.get_webview_window(COUCH) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || build_couch(&app2));
}

fn build_couch(app: &AppHandle) {
    let built = WebviewWindowBuilder::new(app, COUCH, WebviewUrl::App("couch.html".into()))
        .title("SteamDrop Couch mode")
        .fullscreen(true)
        .build();
    match built {
        Ok(w) => {
            let _ = w.set_focus();
            sync_lock(app, None);
            crate::log::info("Couch mode opened.");
        }
        Err(e) => crate::log::problem(format!("Couch mode could not open: {e}")),
    }
}

/// The view switch button and F11: open the other view first, then close this one.
pub fn switch(app: &AppHandle, from: &str) {
    if from == COUCH {
        crate::show_window(app);
    } else {
        open(app, View::Couch);
    }
    let app2 = app.clone();
    let from = from.to_string();
    // Queued after the window above is built, so a window always exists and Steam keeps tracking SteamDrop.
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window(&from) {
            let _ = w.destroy();
        }
    });
}

/// A window is gone: release the lock if it was the last one, and restart Steam if asked when leaving Couch mode.
pub fn closed(app: &AppHandle, label: &str) {
    sync_lock(app, Some(label));
    if label != COUCH {
        return;
    }
    crate::log::info("Couch mode closed.");
    let state = app.state::<App>();
    if state.restart_after_couch.swap(false, Ordering::SeqCst) {
        let app = app.clone();
        std::thread::spawn(move || {
            // Let the process Steam started exit and Steam finish its own bookkeeping first.
            std::thread::sleep(std::time::Duration::from_secs(3));
            let big_picture = app.state::<App>().big_picture.load(Ordering::SeqCst);
            if let Err(e) = crate::library::restart_steam_applying(&app, big_picture) {
                crate::log::problem(format!("Could not restart Steam after Couch mode: {e}"));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_view() {
        assert_eq!(view_for("auto", true), View::Couch);
        assert_eq!(view_for("auto", false), View::Window);
        assert_eq!(view_for("couch", false), View::Couch);
        assert_eq!(view_for("window", true), View::Window);
        assert_eq!(parse_view(&["x", "--open-view=window"]), Some(View::Window));
        assert_eq!(parse_view(&["--couch-open"]), Some(View::Couch));
        assert_eq!(parse_view::<&str>(&[]), None);
    }
}
