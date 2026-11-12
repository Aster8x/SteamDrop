//! Keeps Big Picture usable with only a controller, using Windows' own window events (no polling):
//! - when Big Picture opens while SteamDrop is open from Steam, SteamDrop switches to Couch mode;
//! - when a game started from Steam exits, Big Picture gets the focus back;
//! - with "Keep Big Picture in front" on, Big Picture comes back when another app takes the focus,
//!   and other apps' windows that sit on top of it are sent behind it (never closed or minimized).

use crate::state::App;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static APP: OnceLock<AppHandle> = OnceLock::new();
/// The Big Picture window last seen, so its opening is noticed once (not every time it gets focus again).
static BIG_PICTURE: AtomicIsize = AtomicIsize::new(0);
const BIG_PICTURE_TITLE: &str = "Steam Big Picture Mode";
/// Counts foreground changes, so a delayed "bring Big Picture back" is dropped once the focus moved on.
static CHANGES: AtomicU64 = AtomicU64::new(0);
/// How long another app may keep the focus before Big Picture is brought back.
const KEEP_FRONT_DELAY: Duration = Duration::from_secs(2);
/// A check for windows on top of Big Picture is already waiting.
static SWEEP_WAITING: AtomicBool = AtomicBool::new(false);
/// The game whose exit is being waited for.
static GAME: AtomicU32 = AtomicU32::new(0);

pub fn start(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    std::thread::spawn(|| {
        // SAFETY: an out-of-context hook on this thread, kept for the life of the app; the message
        // loop below is what delivers its events.
        unsafe {
            BIG_PICTURE.store(find_big_picture() as isize, Ordering::SeqCst);
            SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, std::ptr::null_mut(), Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT);
            SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, std::ptr::null_mut(), Some(on_show), 0, 0, WINEVENT_OUTOFCONTEXT);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                DispatchMessageW(&msg);
            }
        }
    });
}

fn find_big_picture() -> HWND {
    let title: Vec<u16> = BIG_PICTURE_TITLE.encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated title and no class name.
    unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) }
}

pub fn title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: the buffer length is passed along.
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    // SAFETY: `pid` is a valid place for the process id.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    let Some(app) = APP.get() else { return };
    let change = CHANGES.fetch_add(1, Ordering::SeqCst) + 1;
    // No window: the secure desktop (a permission prompt) or the lock screen.
    if hwnd.is_null() {
        return;
    }
    if title(hwnd) != BIG_PICTURE_TITLE {
        if find_big_picture().is_null() {
            return;
        }
        watch_game(app, pid_of(hwnd));
        if app.state::<App>().settings().keep_big_picture {
            let (app, hwnd) = (app.clone(), hwnd as isize);
            std::thread::spawn(move || keep_big_picture_in_front(&app, hwnd, change));
        }
    } else {
        sweep_soon(app);
        let opened = BIG_PICTURE.swap(hwnd as isize, Ordering::SeqCst) != hwnd as isize;
        if opened && app.state::<App>().steam_session.load(Ordering::SeqCst) {
            let app = app.clone();
            std::thread::spawn(move || {
                // Let Big Picture finish opening, or it takes the focus straight back.
                std::thread::sleep(Duration::from_millis(1500));
                crate::log::info("Big Picture opened while SteamDrop was open from Steam.");
                crate::couch::resume(&app);
            });
        }
    }
}

/// A window was shown somewhere: if it's a top-level one, check soon whether it sits on top of Big Picture.
unsafe extern "system" fn on_show(_: HWINEVENTHOOK, _: u32, hwnd: HWND, object: i32, _: i32, _: u32, _: u32) {
    // This fires for every control of every app, so the cheap tests come first.
    if object != OBJID_WINDOW || hwnd.is_null() || GetAncestor(hwnd, GA_ROOT) != hwnd {
        return;
    }
    if let Some(app) = APP.get() {
        sweep_soon(app);
    }
}

/// If the window that just came to the front is a game started from Steam, wait (without polling)
/// for it to exit, then give Big Picture the focus back.
fn watch_game(app: &AppHandle, pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};
    if pid == 0 || GAME.load(Ordering::SeqCst) == pid || !crate::steam::from_steam(pid, &crate::steam::processes()) {
        return;
    }
    GAME.store(pid, Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        // SAFETY: the handle is checked before use and closed after.
        unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if h.is_null() {
                GAME.store(0, Ordering::SeqCst);
                return;
            }
            WaitForSingleObject(h, INFINITE);
            CloseHandle(h);
        }
        let _ = GAME.compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst);
        // Let Windows settle on the next window first (a launcher handing over to the game, for example).
        std::thread::sleep(Duration::from_millis(800));
        let big_picture = find_big_picture();
        // SAFETY: a plain window query.
        let front = unsafe { GetForegroundWindow() };
        if big_picture.is_null() || front == big_picture {
            return;
        }
        let procs = crate::steam::processes();
        let settings = app.state::<App>().settings();
        if !front.is_null() && crate::steam::may_keep_focus(pid_of(front), &procs, &settings.keep_big_picture_except, std::process::id()).is_some() {
            return;
        }
        if bring_to_front(big_picture) {
            crate::log::info("A game started from Steam closed, so SteamDrop gave Big Picture the focus back.");
        } else {
            crate::log::problem("A game started from Steam closed, but Windows didn't let SteamDrop give Big Picture the focus back.");
        }
    });
}

/// "Keep Big Picture in front": a moment after another app took the focus, bring Big Picture back,
/// unless the focus has moved on, a game is running, or that app may keep it (see steam::may_keep_focus).
fn keep_big_picture_in_front(app: &AppHandle, hwnd: isize, change: u64) {
    std::thread::sleep(KEEP_FRONT_DELAY);
    let state = app.state::<App>();
    let settings = state.settings();
    // SAFETY: plain window queries; a window that closed meanwhile just gives other answers.
    let foreground = unsafe { GetForegroundWindow() };
    let big_picture = find_big_picture();
    if CHANGES.load(Ordering::SeqCst) != change || foreground as isize != hwnd || big_picture.is_null() || !settings.keep_big_picture {
        return;
    }
    let pid = pid_of(foreground);
    let procs = crate::steam::processes();
    if crate::steam::may_keep_focus(pid, &procs, &settings.keep_big_picture_except, std::process::id()).is_some() {
        return;
    }
    let games = state.data.lock().unwrap().games.clone();
    if crate::steam::game_running(&games) {
        return;
    }
    let name = procs.iter().find(|p| p.pid == pid).map(|p| p.name.clone()).unwrap_or_default();
    if bring_to_front(big_picture) {
        crate::log::info(format!("Keep Big Picture in front: {name} took the focus, so SteamDrop brought Big Picture back."));
    } else {
        crate::log::problem(format!("Keep Big Picture in front: {name} took the focus, but Windows didn't let SteamDrop bring Big Picture back."));
    }
}

/// Schedules one check for windows on top of Big Picture, a moment from now.
fn sweep_soon(app: &AppHandle) {
    if !app.state::<App>().settings().keep_big_picture || find_big_picture().is_null() || SWEEP_WAITING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(KEEP_FRONT_DELAY);
        SWEEP_WAITING.store(false, Ordering::SeqCst);
        sweep(&app);
    });
}

/// Windows' own desktop, taskbar and overlay windows: never touched.
const SHELL_CLASSES: [&str; 6] = ["Shell_TrayWnd", "Shell_SecondaryTrayWnd", "Progman", "WorkerW", "Windows.UI.Core.CoreWindow", "XamlExplorerHostIslandWindow"];

/// "Keep Big Picture in front", for windows that cover Big Picture without taking the focus (a
/// progress popup, say): while Big Picture has the focus and no game runs, each such window of
/// another app is put right behind Big Picture. Only the stacking order changes.
fn sweep(app: &AppHandle) {
    unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> windows_sys::core::BOOL {
        let (stop, above) = &mut *(list as *mut (HWND, Vec<HWND>));
        if hwnd == *stop {
            return 0;
        }
        above.push(hwnd);
        1
    }
    let state = app.state::<App>();
    let settings = state.settings();
    let big_picture = find_big_picture();
    // SAFETY: plain window queries and one stacking-order change per window; `found` outlives EnumWindows.
    unsafe {
        if !settings.keep_big_picture || big_picture.is_null() || GetForegroundWindow() != big_picture || IsIconic(big_picture) != 0 {
            return;
        }
        // EnumWindows goes from the top window down, so everything before Big Picture is above it.
        let mut found: (HWND, Vec<HWND>) = (big_picture, vec![]);
        EnumWindows(Some(collect), &mut found as *mut _ as LPARAM);
        let mut area: RECT = std::mem::zeroed();
        GetWindowRect(big_picture, &mut area);
        let procs = crate::steam::processes();
        let mut game_checked = false;
        for hwnd in found.1 {
            let mut r: RECT = std::mem::zeroed();
            let mut cloaked = 0u32;
            windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(hwnd, windows_sys::Win32::Graphics::Dwm::DWMWA_CLOAKED as u32, &mut cloaked as *mut _ as *mut _, 4);
            let mut class = [0u16; 64];
            let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..n]);
            // Shown, not see-through to clicks (overlays), and really over Big Picture.
            let covers = IsWindowVisible(hwnd) != 0
                && IsIconic(hwnd) == 0
                && cloaked == 0
                && GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT == 0
                && !SHELL_CLASSES.contains(&class.as_str())
                && GetWindowRect(hwnd, &mut r) != 0
                && overlap(&r, &area);
            let pid = pid_of(hwnd);
            if !covers || crate::steam::may_keep_focus(pid, &procs, &settings.keep_big_picture_except, std::process::id()).is_some() {
                continue;
            }
            if !game_checked && crate::steam::game_running(&state.data.lock().unwrap().games.clone()) {
                return;
            }
            game_checked = true;
            let name = procs.iter().find(|p| p.pid == pid).map(|p| p.name.clone()).unwrap_or_default();
            if SetWindowPos(hwnd, big_picture, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) != 0 {
                crate::log::info(format!("Keep Big Picture in front: a window of {name} was on top of Big Picture, so SteamDrop sent it behind."));
            } else {
                crate::log::problem(format!("Keep Big Picture in front: a window of {name} is on top of Big Picture, but Windows didn't let SteamDrop send it behind."));
            }
        }
    }
}

/// True if two window rectangles share any area.
fn overlap(a: &RECT, b: &RECT) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

/// Puts a window in front. Windows only lets the app in front (or the one that got the last key
/// press) do that, so this joins the front app's input, and if that isn't enough sends a key press
/// first: a key that means nothing, then Alt, the usual way to be allowed. Windows may still refuse.
fn bring_to_front(hwnd: HWND) -> bool {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_MENU};
    // SAFETY: plain window calls; the input attachment is always undone and every key press released.
    unsafe {
        for key in [None, Some(0xE8u8), Some(VK_MENU as u8)] {
            let front = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
            let me = GetCurrentThreadId();
            let joined = front != 0 && front != me && AttachThreadInput(me, front, 1) != 0;
            if let Some(k) = key {
                keybd_event(k, 0, 0, 0);
            }
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            if let Some(k) = key {
                keybd_event(k, 0, KEYEVENTF_KEYUP, 0);
            }
            if joined {
                AttachThreadInput(me, front, 0);
            }
            if GetForegroundWindow() == hwnd {
                return true;
            }
        }
        false
    }
}
