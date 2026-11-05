//! Watches the chosen folders using the system's change notifications (no polling).
//! A new game folder is handed over once nothing in it has changed for a while
//! and no file in it is still open for writing.

use crate::state::App;
use notify::{RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Manager};

/// How long a folder must stay quiet before we look at it.
const QUIET: Duration = Duration::from_secs(12);

/// (Re)starts watching the folders in settings. Dropping the old watcher stops its thread.
pub fn restart(app: &AppHandle) {
    let state = app.state::<App>();
    let roots: Vec<PathBuf> = state.settings().watched.iter().map(PathBuf::from).filter(|p| p.is_dir()).collect();
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            crate::log::problem(format!("Folder watching could not start: {e}"));
            return;
        }
    };
    for root in &roots {
        if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
            crate::log::problem(format!("Can't watch {}: {e}", root.display()));
        }
    }
    let list: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
    crate::log::info(if list.is_empty() { "Not watching any folders yet.".into() } else { format!("Watching: {}", list.join(", ")) });
    *state.watcher.lock().unwrap() = Some(watcher);
    let app = app.clone();
    std::thread::spawn(move || run(app, roots, rx));
}

/// The top-level folder inside a watched folder that `path` belongs to.
fn game_folder(roots: &[PathBuf], path: &Path) -> Option<PathBuf> {
    roots.iter().find_map(|root| {
        let first = path.strip_prefix(root).ok()?.components().next()?;
        Some(root.join(first))
    })
}

fn run(app: AppHandle, roots: Vec<PathBuf>, rx: mpsc::Receiver<notify::Result<notify::Event>>) {
    let state = app.state::<App>();
    let unknown = |p: &Path| p.is_dir() && !state.data.lock().unwrap().is_known(&p.to_string_lossy());
    let mut due: HashMap<PathBuf, Instant> = HashMap::new();
    // Folders that arrived while SteamDrop wasn't running.
    for root in &roots {
        for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
            if unknown(&entry.path()) {
                due.insert(entry.path(), Instant::now());
            }
        }
    }
    loop {
        let next = due.values().min().copied();
        let msg = match next {
            Some(t) => rx.recv_timeout(t.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        match msg {
            Ok(Ok(event)) => {
                for p in event.paths {
                    if let Some(folder) = game_folder(&roots, &p) {
                        if unknown(&folder) {
                            due.insert(folder, Instant::now() + QUIET);
                        }
                    }
                }
            }
            Ok(Err(e)) => crate::log::problem(format!("Folder watching: {e}")),
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let now = Instant::now();
        let ready: Vec<PathBuf> = due.iter().filter(|(_, t)| **t <= now).map(|(p, _)| p.clone()).collect();
        let mut batch = vec![];
        for folder in ready {
            due.remove(&folder);
            if !unknown(&folder) {
                continue;
            }
            if still_copying(&folder) {
                due.insert(folder, now + QUIET);
            } else {
                batch.push(folder);
            }
        }
        if !batch.is_empty() {
            crate::library::found(&app, batch);
        }
    }
}

/// True while any recently changed file can't be opened exclusively (it's still being written).
fn still_copying(folder: &Path) -> bool {
    let recent = SystemTime::now() - Duration::from_secs(180);
    let mut stack = vec![folder.to_path_buf()];
    let mut budget = 20_000;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            budget -= 1;
            if budget == 0 {
                return false;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.modified().is_ok_and(|m| m > recent) && locked(&entry.path()) {
                return true;
            }
        }
    }
    false
}

#[cfg(windows)]
fn locked(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    // share_mode(0): fails while anyone else (e.g. Explorer copying) has the file open.
    std::fs::OpenOptions::new().read(true).share_mode(0).open(path).is_err()
}

#[cfg(not(windows))]
fn locked(_: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_paths_to_their_game_folder() {
        let roots = vec![PathBuf::from("/games"), PathBuf::from("/more")];
        assert_eq!(game_folder(&roots, Path::new("/games/Foo/bin/x.dll")), Some(PathBuf::from("/games/Foo")));
        assert_eq!(game_folder(&roots, Path::new("/more/Bar")), Some(PathBuf::from("/more/Bar")));
        assert_eq!(game_folder(&roots, Path::new("/games")), None);
        assert_eq!(game_folder(&roots, Path::new("/elsewhere/x")), None);
    }

    #[cfg(windows)]
    #[test]
    fn open_files_count_as_still_copying() {
        let dir = std::env::temp_dir().join(format!("steamdrop-copy-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let writer = std::fs::File::create(dir.join("sub/big.pak")).unwrap();
        assert!(still_copying(&dir));
        drop(writer);
        assert!(!still_copying(&dir));
    }
}
