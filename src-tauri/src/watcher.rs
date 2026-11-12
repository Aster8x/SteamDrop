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
/// Folders that are ready (or deleted) within this time of each other are handled together:
/// one prompt and at most one Steam restart.
const BATCH: Duration = Duration::from_secs(4);
/// A batch waits for folders that are still copying, but never longer than this.
const BATCH_MAX: Duration = Duration::from_secs(90);

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
    let missing: Vec<PathBuf> = settings.watched.iter().map(PathBuf::from).filter(|p| !p.is_dir()).collect();
    for m in &missing {
        crate::log::problem(format!("Watched folder {} isn't there right now (a drive that's off or unplugged?). SteamDrop starts watching it as soon as it appears.", m.display()));
    }
    let list: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
    crate::log::info(if list.is_empty() { "Not watching any folders yet.".into() } else { format!("Watching: {}", list.join(", ")) });
    *state.watcher.lock().unwrap() = Some(watcher);
    let app = app.clone();
    std::thread::spawn(move || run(app, roots, excluded, missing, rx));
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

fn run(app: AppHandle, roots: Vec<PathBuf>, excluded: Vec<PathBuf>, missing: Vec<PathBuf>, rx: mpsc::Receiver<notify::Result<notify::Event>>) {
    let state = app.state::<App>();
    let unknown = |p: &Path| p.is_dir() && !state.data.lock().unwrap().is_known(&p.to_string_lossy());
    let mut due: HashMap<PathBuf, Instant> = HashMap::new();
    let is_game = |f: &Path| state.data.lock().unwrap().games.iter().any(|g| crate::state::same_path(&g.folder, &f.to_string_lossy()));
    // The current batch: new folders that finished copying, and game folders that were deleted.
    let mut batch = Batch::default();
    // Folders that arrived while SteamDrop wasn't running.
    for root in &roots {
        for folder in subfolders(root, &roots, &excluded) {
            if unknown(&folder) {
                due.insert(folder, Instant::now());
            }
        }
    }
    loop {
        // Only while a watched folder is missing: look for it twice a minute.
        if missing.iter().any(|m| m.is_dir()) {
            return restart(&app);
        }
        let look_again = (!missing.is_empty()).then(|| Instant::now() + Duration::from_secs(30));
        let next = due.values().copied().chain(batch.flush_at(&due)).chain(look_again).min();
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
                        } else if !folder.exists() && !batch.gone.contains(&folder) && is_game(&folder) {
                            batch.gone.push(folder);
                            batch.touch();
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
        for folder in ready {
            due.remove(&folder);
            if !unknown(&folder) {
                continue;
            }
            if still_copying(&folder) {
                due.insert(folder, now + QUIET);
            } else if !batch.new.contains(&folder) {
                batch.new.push(folder);
                batch.touch();
            }
        }
        if batch.flush_at(&due).is_some_and(|t| t <= now) {
            let Batch { new, gone, .. } = std::mem::take(&mut batch);
            if !new.is_empty() {
                crate::library::found(&app, new.clone());
            }
            if !gone.is_empty() {
                crate::library::gone(&app, gone, &new);
            }
        }
    }
}

/// Folder changes waiting to be handled together.
#[derive(Default)]
struct Batch {
    new: Vec<PathBuf>,
    gone: Vec<PathBuf>,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Batch {
    fn touch(&mut self) {
        let now = Instant::now();
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// When to hand the batch over: a few seconds after its last folder, once no other folder is
    /// still on its way (a moved game's new folder may still be waiting), or after BATCH_MAX at most.
    fn flush_at(&self, due: &HashMap<PathBuf, Instant>) -> Option<Instant> {
        let (first, last) = (self.first?, self.last?);
        let cap = first + BATCH_MAX;
        let waiting = due.values().max().map_or(last, |t| (*t).max(last));
        Some((waiting + BATCH).min(cap))
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
    fn a_batch_waits_for_its_last_folder_and_for_folders_still_copying() {
        let mut b = Batch::default();
        assert_eq!(b.flush_at(&HashMap::new()), None, "nothing to hand over");
        b.touch();
        let first = b.first.unwrap();
        assert_eq!(b.flush_at(&HashMap::new()), Some(b.last.unwrap() + BATCH));
        // Another folder is still on its way: wait for it, but never past BATCH_MAX.
        let mut due = HashMap::new();
        due.insert(PathBuf::from("/g/Big"), first + Duration::from_secs(30));
        assert_eq!(b.flush_at(&due), Some(first + Duration::from_secs(30) + BATCH));
        due.insert(PathBuf::from("/g/Huge"), first + Duration::from_secs(600));
        assert_eq!(b.flush_at(&due), Some(first + BATCH_MAX));
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
