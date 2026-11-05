//! Sending a game folder to the Recycle Bin. Never deletes permanently: SteamDrop only recycles on a
//! local fixed drive that Windows says has a Recycle Bin. Anywhere else (network shares, USB sticks
//! and cards) it refuses and leaves the folder alone.

use std::path::Path;

const NO_BIN: &str = "This drive has no Recycle Bin, so SteamDrop won't delete the game. You can delete the folder yourself in File Explorer.";

#[cfg(windows)]
fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain([0]).collect()
}

/// Number of items in the drive's Recycle Bin, or None if it has none.
#[cfg(windows)]
fn bin_items(root: &[u16]) -> Option<(i64, i64)> {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    // SAFETY: `root` is a NUL-terminated UTF-16 string and `info` is a valid, sized struct.
    let hr = unsafe { SHQueryRecycleBinW(root.as_ptr(), &mut info) };
    (hr == 0).then_some((info.i64NumItems, info.i64Size))
}

/// The root ("D:\") of a path on a local fixed drive with a Recycle Bin; None anywhere else.
#[cfg(windows)]
fn recyclable_root(path: &Path) -> Option<Vec<u16>> {
    use std::path::{Component, Prefix};
    let Some(Component::Prefix(prefix)) = path.components().next() else { return None };
    // Only plain drive letters: UNC paths (\\server\share) are network locations without a Recycle Bin.
    let letter = match prefix.kind() {
        Prefix::Disk(l) | Prefix::VerbatimDisk(l) => l as char,
        _ => return None,
    };
    let root = wide(Path::new(&format!("{letter}:\\")));
    const DRIVE_FIXED: u32 = 3;
    // SAFETY: `root` is a NUL-terminated UTF-16 string.
    let kind = unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr()) };
    (kind == DRIVE_FIXED && bin_items(&root).is_some()).then_some(root)
}

#[cfg(windows)]
pub fn recycle(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::*;
    // Location first, so a path on a network share is refused before anything touches it.
    if recyclable_root(path).is_none() {
        return Err(NO_BIN.into());
    }
    if !path.is_dir() {
        return Err("The game folder no longer exists.".into());
    }
    // pFrom is a list ending in two NULs.
    let mut from = wide(path);
    from.push(0);
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

    fn game_folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("steamdrop-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/save.dat"), vec![7u8; 64 * 1024]).unwrap();
        dir
    }

    /// Wine has a trash folder but doesn't report Recycle Bin counts, so the strict check can't run there.
    fn under_wine() -> bool {
        winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE).open_subkey(r"Software\Wine").is_ok()
    }

    /// On a normal fixed drive the folder must really end up in the Recycle Bin: gone from disk,
    /// and the Recycle Bin holds one more item with at least the folder's data. Strict on Windows
    /// and on CI; skipped only under Wine.
    #[test]
    fn recycles_on_a_fixed_drive() {
        if under_wine() && std::env::var_os("CI").is_none() {
            eprintln!("skipped under Wine: it doesn't report Recycle Bin contents");
            return;
        }
        let dir = game_folder("recycle");
        let root = recyclable_root(&dir).expect("the temp folder must be on a fixed drive with a Recycle Bin");
        let (items_before, size_before) = bin_items(&root).unwrap();
        recycle(&dir).expect("recycling on a fixed drive must work");
        assert!(!dir.exists(), "the folder must be gone from disk");
        let (items_after, size_after) = bin_items(&root).unwrap();
        assert!(items_after > items_before, "the Recycle Bin must hold one more item ({items_before} -> {items_after})");
        assert!(size_after >= size_before + 64 * 1024, "the Recycle Bin must hold the folder's data ({size_before} -> {size_after})");
        eprintln!("recycled for real: Recycle Bin items {items_before} -> {items_after}, bytes {size_before} -> {size_after}");
    }

    /// A network location (here the same folder through \\localhost\C$) is refused untouched.
    #[test]
    fn refuses_where_there_is_no_recycle_bin() {
        let dir = game_folder("refuse");
        let local = dir.to_string_lossy().into_owned();
        let (drive, rest) = local.split_once(":\\").unwrap();
        let unc = std::path::PathBuf::from(format!(r"\\localhost\{drive}$\{rest}"));
        assert_eq!(recycle(&unc), Err(NO_BIN.to_string()));
        assert_eq!(std::fs::read(dir.join("sub/save.dat")).unwrap().len(), 64 * 1024, "the folder must be untouched");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
