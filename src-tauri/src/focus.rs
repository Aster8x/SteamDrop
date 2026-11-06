//! Reacts to the foreground window changing, using Windows' own event (no polling):
//! when Big Picture opens while SteamDrop is open from Steam, SteamDrop switches to Couch mode.

use crate::state::App;
use std::sync::atomic::{AtomicIsize, Ordering};
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
    if hwnd.is_null() {
        return;
    }
    if title(hwnd) == BIG_PICTURE_TITLE {
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
