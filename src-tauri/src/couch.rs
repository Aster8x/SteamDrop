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
///
/// This is the process Steam tracks, and it has no view of its own (the views live in the app), so
/// Steam's "Resume" had nothing to bring to the front. It now shows a tiny, see-through "SteamDrop"
/// window for Steam to focus; when Steam activates it, the app is asked to come to the front
/// (`--resume`: Couch mode while Big Picture is open). The window's message loop replaces the old sleep loop.
#[cfg(windows)]
pub fn run_from_steam() {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::time::{Duration, Instant};

    static EXE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    static SEEN_OPEN: AtomicBool = AtomicBool::new(false);
    static LAST_RESUME_MS: AtomicU64 = AtomicU64::new(0);
    let Ok(exe) = std::env::current_exe() else { return };
    let big_picture = started_in_big_picture();
    let setting = crate::state::load(&crate::state::data_dir()).settings.steam_view;
    let view = view_for(&setting, big_picture);
    let mut child = std::process::Command::new(&exe);
    child.arg(if view == View::Couch { "--open-view=couch" } else { "--open-view=window" });
    if child.spawn().is_err() {
        return;
    }
    let _ = EXE.set(exe);
    let _ = START.set(Instant::now());

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        let start = *START.get().unwrap();
        match msg {
            // Steam's "Resume" activates this window. Not in the first moments (that's the launch itself),
            // and at most once a second.
            WM_ACTIVATE if (wp & 0xFFFF) as u32 != WA_INACTIVE && start.elapsed() > Duration::from_secs(2) => {
                let now = start.elapsed().as_millis() as u64;
                if now.saturating_sub(LAST_RESUME_MS.swap(now, Ordering::SeqCst)) > 1000 {
                    // This process is in front right now, so it may let the app take the foreground.
                    AllowSetForegroundWindow(ASFW_ANY);
                    let _ = std::process::Command::new(EXE.get().unwrap()).arg("--resume").spawn();
                }
                0
            }
            WM_TIMER => {
                // Error 32 = sharing violation: the app holds the lock, so a SteamDrop window is open.
                let open = matches!(open_lock(), Err(e) if e.raw_os_error() == Some(32));
                if open {
                    SEEN_OPEN.store(true, Ordering::SeqCst);
                } else if SEEN_OPEN.load(Ordering::SeqCst) || start.elapsed() > Duration::from_secs(60) {
                    PostQuitMessage(0);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    let class: Vec<u16> = "SteamDropSession".encode_utf16().chain([0]).collect();
    let title: Vec<u16> = "SteamDrop".encode_utf16().chain([0]).collect();
    // SAFETY: plain Win32 window setup with NUL-terminated strings that outlive the window;
    // the window procedure only touches the statics above.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
        // 1x1 and almost fully transparent: nothing to see, but a real visible window Steam can focus.
        // A tool window, so it adds no second button to the taskbar.
        let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOOLWINDOW, class.as_ptr(), title.as_ptr(), WS_POPUP, 0, 0, 1, 1, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
        if hwnd.is_null() {
            return;
        }
        SetLayeredWindowAttributes(hwnd, 0, 1, LWA_ALPHA);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        // shortcut: checks the lock twice a second; a named event would avoid that, if it ever matters.
        SetTimer(hwnd, 1, 500, None);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Steam's "Resume" (see run_from_steam) and Big Picture opening during a Steam session: bring
/// SteamDrop to the front. While Big Picture is open that's Couch mode, switching the desktop window
/// over rather than opening a second view.
pub fn resume(app: &AppHandle) {
    if !crate::steam::big_picture_open() {
        bring_up(app);
    } else if app.get_webview_window(MAIN).is_some() && app.get_webview_window(COUCH).is_none() {
        crate::log::info("Big Picture is open, so SteamDrop switched to Couch mode.");
        switch(app, MAIN);
    } else {
        open(app, View::Couch);
    }
}

/// Holds the session lock while any SteamDrop window exists (`closing` is about to go away).
pub fn sync_lock(app: &AppHandle, closing: Option<&str>) {
    let any = [MAIN, COUCH].iter().any(|l| Some(*l) != closing && app.get_webview_window(l).is_some());
    let state = app.state::<App>();
    let mut lock = state.couch_lock.lock().unwrap();
    if !any {
        lock.take();
        state.steam_session.store(false, Ordering::SeqCst);
    } else if lock.is_none() {
        #[cfg(windows)]
        {
            *lock = open_lock().ok();
        }
    }
}

/// Brings SteamDrop to the front for a question (new games, deleted folders): Couch mode if it's
/// open, otherwise the desktop window.
pub fn bring_up(app: &AppHandle) {
    let view = if app.get_webview_window(COUCH).is_some() { View::Couch } else { View::Window };
    open(app, view);
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
        // The page's own background, so the window never flashes white while it loads.
        .background_color(tauri::window::Color(11, 15, 24, 255))
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
            // Let the process Steam started exit first, so Steam has stopped tracking SteamDrop.
            let me = std::process::id();
            let exe = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase())).unwrap_or_default();
            let t = std::time::Instant::now();
            while t.elapsed() < std::time::Duration::from_secs(5) && crate::steam::processes().iter().any(|p| p.pid != me && p.name == exe) {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            crate::library::request_restart(&app);
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
