//! Couch mode: the full-screen, controller-first window that SteamDrop's own Steam shortcut opens.
//!
//! Steam starts `SteamDrop.exe --couch`. That process asks the SteamDrop app (starting it if needed)
//! to open Couch mode, then simply waits until Couch mode closes, so Steam shows SteamDrop as running
//! for exactly that long. The app holds `couch.lock` exclusively while the window is open.

use crate::state::App;
#[cfg(windows)]
use std::path::PathBuf;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "couch";

#[cfg(windows)]
fn lock_path() -> PathBuf {
    crate::state::data_dir().join("couch.lock")
}

#[cfg(windows)]
fn open_exclusive() -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let _ = std::fs::create_dir_all(crate::state::data_dir());
    std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(0).open(lock_path())
}

/// `SteamDrop.exe --couch`: start Couch mode in the app, then wait for it to close.
#[cfg(windows)]
pub fn run_from_steam() {
    use std::time::{Duration, Instant};
    let Ok(exe) = std::env::current_exe() else { return };
    let mut child = std::process::Command::new(exe);
    child.arg("--couch-open");
    if started_in_big_picture() {
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
        // Error 32 = sharing violation: the app holds the lock, so Couch mode is open.
        let open = matches!(open_exclusive(), Err(e) if e.raw_os_error() == Some(32));
        if open {
            seen_open = true;
        } else if seen_open || start.elapsed() > Duration::from_secs(60) {
            return;
        }
    }
}

/// Steam sets SteamTenfoot=1 for games started from Big Picture; a Steam Deck sets SteamDeck=1.
pub fn started_in_big_picture() -> bool {
    std::env::var("SteamTenfoot").is_ok_and(|v| v == "1") || std::env::var("SteamDeck").is_ok_and(|v| v == "1")
}

/// Opens (or focuses) the Couch mode window.
pub fn open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_focus();
        return;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let built = WebviewWindowBuilder::new(&app2, LABEL, WebviewUrl::App("couch.html".into()))
            .title("SteamDrop Couch mode")
            .fullscreen(true)
            .build();
        match built {
            Ok(w) => {
                let _ = w.set_focus();
                #[cfg(windows)]
                {
                    *app2.state::<App>().couch_lock.lock().unwrap() = open_exclusive().ok();
                }
                crate::log::info("Couch mode opened.");
            }
            Err(e) => crate::log::problem(format!("Couch mode could not open: {e}")),
        }
    });
}

/// The window is gone: release the lock so the process Steam started can exit.
pub fn closed(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    let state = app.state::<App>();
    state.couch_lock.lock().unwrap().take();
    crate::log::info("Couch mode closed.");
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
