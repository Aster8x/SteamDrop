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
    let settings = state.settings();
    let roots: Vec<PathBuf> = settings.watched.iter().map(PathBuf::from).filter(|p| p.is_dir()).collect();
    let excluded: Vec<PathBuf> = settings.excluded.iter().map(PathBuf::from).collect();
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
    std::thread::spawn(move || run(app, roots, excluded, rx));
}

/// The game folder `path` belongs to: a direct subfolder of the most specific watched folder
/// that contains it. None for a watched folder itself, a watched folder inside another one,
/// and anything in or under an excluded folder.
pub fn game_folder(roots: &[PathBuf], excluded: &[PathBuf], path: &Path) -> Option<PathBuf> {
    let root = roots.iter().filter(|r| path.starts_with(r) && path != r.as_path()).max_by_key(|r| r.components().count())?;
    let folder = root.join(path.strip_prefix(root).ok()?.components().next()?);
    let ignored = roots.contains(&folder) || excluded.iter().any(|x| path.starts_with(x) || folder.starts_with(x));
    (!ignored).then_some(folder)
}

/// The game folders directly inside one watched folder, with the same rules as `game_folder`.
pub fn subfolders(root: &Path, roots: &[PathBuf], excluded: &[PathBuf]) -> Vec<PathBuf> {
    let entries = std::fs::read_dir(root).into_iter().flatten().flatten().map(|e| e.path());
    entries.filter(|p| p.is_dir() && game_folder(roots, excluded, p).as_ref() == Some(p)).collect()
}

fn run(app: AppHandle, roots: Vec<PathBuf>, excluded: Vec<PathBuf>, rx: mpsc::Receiver<notify::Result<notify::Event>>) {
    let state = app.state::<App>();
    let unknown = |p: &Path| p.is_dir() && !state.data.lock().unwrap().is_known(&p.to_string_lossy());
    let mut due: HashMap<PathBuf, Instant> = HashMap::new();
    // Folders that arrived while SteamDrop wasn't running.
    for root in &roots {
        for folder in subfolders(root, &roots, &excluded) {
            if unknown(&folder) {
                due.insert(folder, Instant::now());
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
                // Any kind of event counts: a copy sends many, a move or rename into the folder just one.
                for p in event.paths {
                    if let Some(folder) = game_folder(&roots, &excluded, &p) {
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

fn rules(app: &AppHandle) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let s = app.state::<App>().settings();
    (s.watched.iter().map(PathBuf::from).collect(), s.excluded.iter().map(PathBuf::from).collect())
}

/// "Add existing games now" for one watched folder: its folders that aren't games yet, even ones
/// recorded as "already there" when watching began.
pub fn add_existing(app: &AppHandle, root: &Path) -> usize {
    let (roots, excluded) = rules(app);
    let state = app.state::<App>();
    let folders: Vec<PathBuf> = {
        let d = state.data.lock().unwrap();
        subfolders(root, &roots, &excluded).into_iter().filter(|f| !d.games.iter().any(|g| crate::state::same_path(&g.folder, &f.to_string_lossy()))).collect()
    };
    let _ = state.update(|d| d.known.retain(|k| !folders.iter().any(|f| crate::state::same_path(k, &f.to_string_lossy()))));
    let count = folders.len();
    if count > 0 {
        crate::library::found(app, folders);
    }
    count
}

/// "Scan now": hands over every new, fully copied folder in the watched folders. Returns how many.
pub fn scan_now(app: &AppHandle) -> usize {
    let state = app.state::<App>();
    let (roots, excluded) = rules(app);
    let ready: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| subfolders(root, &roots, &excluded))
        .filter(|p| !state.data.lock().unwrap().is_known(&p.to_string_lossy()))
        .filter(|p| std::fs::read_dir(p).is_ok_and(|mut d| d.next().is_some()))
        // Folders still being copied are left to the watcher, which adds them once copying ends.
        .filter(|p| !still_copying(p))
        .collect();
    let count = ready.len();
    if count > 0 {
        crate::library::found(app, ready);
    }
    count
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
        let p = PathBuf::from;
        // Games and Games/Torrent are watched; Games/Torrent/X is excluded.
        let roots = vec![p("/g"), p("/g/Torrent"), p("/more")];
        let excluded = vec![p("/g/Torrent/X")];
        let gf = |path: &str| game_folder(&roots, &excluded, Path::new(path));
        assert_eq!(gf("/g/Foo/bin/x.dll"), Some(p("/g/Foo")));
        assert_eq!(gf("/more/Bar"), Some(p("/more/Bar")));
        assert_eq!(gf("/g"), None, "a watched folder is never a game");
        assert_eq!(gf("/elsewhere/x"), None);
        assert_eq!(gf("/g/Torrent"), None, "the inner watched folder isn't a game of the outer one");
        assert_eq!(gf("/g/Torrent/Celeste/c.exe"), Some(p("/g/Torrent/Celeste")), "the most specific watched folder wins");
        assert_eq!(gf("/g/Torrent/X"), None);
        assert_eq!(gf("/g/Torrent/X/Downloads/Game/game.exe"), None, "everything inside an excluded folder is ignored");
    }

    #[test]
    fn subfolders_follow_the_rules() {
        let base = std::env::temp_dir().join(format!("steamdrop-sub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["Torrent/X/Downloads", "Torrent/Celeste", "Hades"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        std::fs::write(base.join("readme.txt"), b"not a folder").unwrap();
        let roots = vec![base.clone(), base.join("Torrent")];
        let excluded = vec![base.join("Torrent/X")];
        assert_eq!(subfolders(&base, &roots, &excluded), vec![base.join("Hades")]);
        assert_eq!(subfolders(&base.join("Torrent"), &roots, &excluded), vec![base.join("Torrent/Celeste")]);
    }

    /// A folder moved (renamed) into a watched folder sends an event for its new path, which maps to a game.
    #[test]
    fn a_move_into_a_watched_folder_is_seen() {
        let base = std::env::temp_dir().join(format!("steamdrop-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("Games");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(base.join("Downloads/Tunic")).unwrap();
        std::fs::write(base.join("Downloads/Tunic/tunic.exe"), b"x").unwrap();
        let (tx, rx) = mpsc::channel();
        let mut w = notify::recommended_watcher(tx).unwrap();
        w.watch(&root, RecursiveMode::Recursive).unwrap();
        std::fs::rename(base.join("Downloads/Tunic"), root.join("Tunic")).unwrap();
        let roots = vec![root.clone()];
        let deadline = Instant::now() + Duration::from_secs(10);
        let seen = loop {
            let Ok(Ok(ev)) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) else { break false };
            // Windows may report a short (8.3) or differently-cased path; compare canonical forms.
            let hit = ev.paths.iter().filter_map(|p| game_folder(&roots, &[], p)).any(|f| f.canonicalize().ok() == root.join("Tunic").canonicalize().ok());
            if hit {
                break true;
            }
        };
        assert!(seen, "moving a folder in must be noticed");
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
