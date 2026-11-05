//! Sending a game folder to the Recycle Bin. Never deletes permanently: if the drive has no
//! Recycle Bin (network or some USB drives), SteamDrop refuses instead.

use std::path::Path;

const NO_BIN: &str = "This drive has no Recycle Bin, so SteamDrop won't delete the game. You can delete the folder yourself in File Explorer.";

#[cfg(windows)]
fn bin_items(root: &[u16]) -> Option<i64> {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    // SAFETY: `root` is a NUL-terminated UTF-16 string and `info` is a valid, sized struct.
    let hr = unsafe { SHQueryRecycleBinW(root.as_ptr(), &mut info) };
    (hr == 0).then_some(info.i64NumItems)
}

#[cfg(windows)]
pub fn recycle(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Component;
    use windows_sys::Win32::UI::Shell::*;
    if !path.is_dir() {
        return Err("The game folder no longer exists.".into());
    }
    let Some(Component::Prefix(prefix)) = path.components().next() else { return Err(NO_BIN.into()) };
    let root: Vec<u16> = Path::new(prefix.as_os_str()).join("\\").as_os_str().encode_wide().chain([0]).collect();
    if bin_items(&root).is_none() {
        return Err(NO_BIN.into());
    }
    // pFrom is a list ending in two NULs.
    let from: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut op = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: std::ptr::null(),
        // ALLOWUNDO = Recycle Bin. WANTNUKEWARNING makes Windows ask before any permanent delete.
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOF_WANTNUKEWARNING) as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    // SAFETY: every pointer in `op` is valid for the duration of the call.
    let result = unsafe { SHFileOperationW(&mut op) };
    if result != 0 || op.fAnyOperationsAborted != 0 {
        return Err(format!("Windows couldn't move the folder to the Recycle Bin (code {result}). Close the game and any open files in that folder, then try again."));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn recycle(_: &Path) -> Result<(), String> {
    Err(NO_BIN.into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;

    #[test]
    fn recycles_or_refuses_but_never_destroys() {
        let dir = std::env::temp_dir().join(format!("steamdrop-recycle-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/save.dat"), b"keep me").unwrap();
        let root: Vec<u16> = dir.ancestors().last().unwrap().as_os_str().encode_wide().chain([0]).collect();
        let before = bin_items(&root);
        match recycle(&dir) {
            Ok(()) => {
                assert!(!dir.exists());
                assert!(bin_items(&root).unwrap() > before.unwrap(), "the folder must be in the Recycle Bin");
            }
            // Refused: the folder must still be there, untouched.
            Err(_) => assert_eq!(std::fs::read(dir.join("sub/save.dat")).unwrap(), b"keep me"),
        }
    }
}
