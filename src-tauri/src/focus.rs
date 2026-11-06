//! Reacts to the foreground window changing, using Windows' own event (no polling):
//! when Big Picture opens while SteamDrop is open from Steam, SteamDrop switches to Couch mode, and
//! with "Keep Big Picture in front" on, Big Picture comes back when another app takes the focus.

use crate::state::App;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use windows_sys::Win32::Foundation::HWND;
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

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    let Some(app) = APP.get() else { return };
    let change = CHANGES.fetch_add(1, Ordering::SeqCst) + 1;
    // No window: the secure desktop (a permission prompt) or the lock screen.
    if hwnd.is_null() {
        return;
    }
    if title(hwnd) != BIG_PICTURE_TITLE {
        if app.state::<App>().settings().keep_big_picture && !find_big_picture().is_null() {
            let (app, hwnd) = (app.clone(), hwnd as isize);
            std::thread::spawn(move || keep_big_picture_in_front(&app, hwnd, change));
        }
    } else {
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
    let mut pid = 0;
    // SAFETY: `pid` is a valid place for the process id.
    unsafe { GetWindowThreadProcessId(foreground, &mut pid) };
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
        crate::log::info(format!("Keep Big Picture in front: {name} took the focus, but Windows didn't let SteamDrop bring Big Picture back."));
    }
}

/// Puts a window in front. Windows only lets the app in front do that, so this briefly joins
/// the front app's input; Windows may still refuse.
fn bring_to_front(hwnd: HWND) -> bool {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    // SAFETY: plain window calls; the input attachment is always undone.
    unsafe {
        let front = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        let me = GetCurrentThreadId();
        let joined = front != 0 && front != me && AttachThreadInput(me, front, 1) != 0;
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if joined {
            AttachThreadInput(me, front, 0);
        }
        GetForegroundWindow() == hwnd
    }
}
