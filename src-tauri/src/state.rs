//! What SteamDrop remembers between runs, kept in %APPDATA%\SteamDrop\data.json.

use crate::exe::Candidate;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub watched: Vec<String>,
    /// Folders inside watched folders that SteamDrop ignores, with everything in them.
    pub excluded: Vec<String>,
    /// The three "automatic mode" switches. The main switch is derived from them in the UI.
    pub auto_add: bool,
    pub auto_restart: bool,
    pub notify: bool,
    pub run_on_startup: bool,
    pub close_to_tray: bool,
    /// Empty means "detect it".
    pub steam_path: String,
    pub art_in_game_folder: bool,
    pub theme: String,
    pub update_on_startup: bool,
    pub update_daily: bool,
    pub update_auto_install: bool,
    /// "Skip this version" in the update window.
    pub skipped_version: String,
    /// The version whose "What's new" was already shown.
    pub last_seen_version: String,
    /// SteamDrop's own Steam shortcut, which opens Couch mode.
    pub steam_shortcut: bool,
    /// "When opened from Steam": "auto" (Big Picture = Couch mode), "couch" or "window".
    pub steam_view: String,
    /// "Add games to": all Steam accounts, or only the ones listed in `accounts`.
    pub all_accounts: bool,
    pub accounts: Vec<String>,
    /// "Play a sound when a game is added", and its volume (0 to 100).
    pub chime: bool,
    pub chime_volume: u8,
    /// "Open Steam in Big Picture when Windows starts".
    pub big_picture_at_sign_in: bool,
    /// "When SteamDrop restarts Steam": "keep", "bigpicture" or "desktop".
    pub steam_restart_mode: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            watched: vec![],
            excluded: vec![],
            auto_add: false,
            auto_restart: false,
            notify: false,
            run_on_startup: false,
            close_to_tray: true,
            steam_path: String::new(),
            art_in_game_folder: false,
            theme: "dark".into(),
            update_on_startup: true,
            update_daily: true,
            update_auto_install: false,
            skipped_version: String::new(),
            last_seen_version: String::new(),
            steam_shortcut: true,
            steam_view: "auto".into(),
            all_accounts: true,
            accounts: vec![],
            chime: true,
            chime_volume: 70,
            big_picture_at_sign_in: false,
            steam_restart_mode: "keep".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    /// Steam's shortcut id; also names the art files.
    pub id: u32,
    pub name: String,
    pub exe: String,
    pub folder: String,
    pub steam_appid: Option<u32>,
    pub added: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub settings: Settings,
    pub games: Vec<Game>,
    /// Folders already dealt with (added, skipped, or there before watching began).
    pub known: Vec<String>,
    /// Shortcuts removed while Steam was running; removed again once Steam is closed,
    /// because Steam can write its old list back.
    pub pending_removals: Vec<u32>,
    /// Games added or changed while Steam was running; added again once Steam is closed,
    /// because Steam can write its old list back.
    pub pending_adds: Vec<u32>,
    /// "Show SteamDrop in my Steam library" was changed while Steam was running.
    pub pending_self: bool,
}

/// A new folder waiting for the user to confirm.
#[derive(Clone, Debug, Serialize)]
pub struct Found {
    pub folder: String,
    pub title: String,
    pub candidates: Vec<Candidate>,
    pub installer_only: bool,
}

pub struct App {
    pub dir: PathBuf,
    pub data: Mutex<Data>,
    pub found: Mutex<Vec<Found>>,
    /// Games whose folder was deleted, waiting for "Remove selected" or "Keep in Steam".
    pub gone: Mutex<Vec<u32>>,
    pub status: Mutex<String>,
    /// Only one Steam change at a time.
    pub busy: tauri::async_runtime::Mutex<()>,
    pub http: reqwest::Client,
    pub watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// A newer version waiting for the user's choice in the update window.
    pub update: Mutex<Option<crate::updates::Available>>,
    /// Set once after an update, for the "What's new" screen.
    pub whats_new: Mutex<Option<crate::changelog::Section>>,
    /// Held while Couch mode is open; the process Steam started waits for it to be released.
    pub couch_lock: Mutex<Option<std::fs::File>>,
    /// When data.json was last read or written by this process.
    seen: Mutex<Option<std::time::SystemTime>>,
    /// "Restart now" was chosen when leaving Couch mode.
    pub restart_after_couch: std::sync::atomic::AtomicBool,
    /// A Steam restart is waiting to run (see library::request_restart).
    pub restart_queued: std::sync::atomic::AtomicBool,
    /// A view was opened from Steam and is still open (Steam shows SteamDrop as running).
    pub steam_session: std::sync::atomic::AtomicBool,
}

/// %APPDATA%\SteamDrop, from Windows' own Roaming AppData location, so every SteamDrop process
/// (tray app, the one Steam starts, startup entries, the installer's helpers) uses the same folder
/// whatever environment it was started with.
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir)
        .join("SteamDrop")
}

/// Reads and parses one file, retrying briefly: on Windows a file that another process is
/// replacing at that moment can refuse to open for a few milliseconds.
fn parse(path: &Path) -> Result<Option<Data>, String> {
    let mut tries = 0;
    loop {
        match std::fs::read(path) {
            Ok(bytes) => return serde_json::from_slice(&bytes).map(Some).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) if tries >= 20 => return Err(e.to_string()),
            Err(_) => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

/// The saved data. Ok(None) only when there's nothing saved yet (a fresh install).
/// A data.json that can't be read is kept as data.json.broken-<time> and SteamDrop recovers from
/// data.json.bak (the last good copy); it is never silently replaced by defaults.
fn read(dir: &Path) -> Result<Option<Data>, String> {
    let main = dir.join("data.json");
    let error = match parse(&main) {
        Ok(d) => return Ok(d),
        Err(e) => e,
    };
    let kept = dir.join(format!("data.json.broken-{}", crate::steam::now()));
    let _ = std::fs::copy(&main, &kept);
    match parse(&dir.join("data.json.bak")) {
        Ok(Some(d)) => {
            crate::log::problem(format!("SteamDrop's data file couldn't be read ({error}). Your settings and games were recovered from the backup; the unreadable file was kept as {}.", kept.display()));
            Ok(Some(d))
        }
        _ => {
            crate::log::problem(format!("SteamDrop's data file couldn't be read ({error}) and there's no usable backup. The unreadable file was kept as {}.", kept.display()));
            Err(error)
        }
    }
}

/// The saved data, or defaults on a fresh install (or when nothing could be recovered).
pub fn load(dir: &Path) -> Data {
    read(dir).ok().flatten().unwrap_or_default()
}

fn modified(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir.join("data.json")).and_then(|m| m.modified()).ok()
}

/// Saves atomically: written to a temporary file of this process's own, then renamed over data.json.
/// The previous data.json, if it's a good one, becomes data.json.bak first.
pub fn save(dir: &Path, data: &Data) -> Result<(), String> {
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let main = dir.join("data.json");
    if matches!(parse(&main), Ok(Some(_))) {
        let _ = std::fs::copy(&main, dir.join("data.json.bak"));
    }
    let n = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!("data.json.{}-{n}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec_pretty(data).unwrap()).map_err(|e| e.to_string())?;
    let mut tries = 0;
    loop {
        match std::fs::rename(&tmp, &main) {
            Ok(()) => return Ok(()),
            // Another process may hold data.json open for a moment; try again before giving up.
            Err(_) if tries < 20 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("Couldn't save SteamDrop's settings: {e}"));
            }
        }
    }
}

/// Folder paths compare case-insensitively on Windows.
pub fn same_path(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase();
    n(a) == n(b)
}

impl Settings {
    /// These settings with `patch` (only the changed fields, as JSON) applied, so a window holding an
    /// older copy can't put old values back.
    pub fn patched(&self, patch: &serde_json::Value) -> Result<Settings, String> {
        let mut merged = serde_json::to_value(self).map_err(|e| e.to_string())?;
        let (Some(base), Some(changes)) = (merged.as_object_mut(), patch.as_object()) else { return Err("Settings change wasn't understood.".into()) };
        for (k, v) in changes {
            base.insert(k.clone(), v.clone());
        }
        serde_json::from_value(merged).map_err(|e| format!("Settings change wasn't understood: {e}"))
    }
}

impl Data {
    /// Everything written while Steam was running has now been written while it was closed.
    pub fn clear_pending(&mut self) {
        self.pending_removals.clear();
        self.pending_adds.clear();
        self.pending_self = false;
    }

    pub fn is_known(&self, folder: &str) -> bool {
        self.known.iter().any(|k| same_path(k, folder)) || self.games.iter().any(|g| same_path(&g.folder, folder))
    }

    pub fn mark_known(&mut self, folder: &str) {
        if !self.known.iter().any(|k| same_path(k, folder)) {
            self.known.push(folder.to_string());
        }
    }
}

impl App {
    pub fn new() -> App {
        App::open(data_dir())
    }

    /// The app state for a data folder (`new` uses %APPDATA%\SteamDrop; tests use their own).
    pub fn open(dir: PathBuf) -> App {
        App {
            data: Mutex::new(load(&dir)),
            seen: Mutex::new(modified(&dir)),
            dir,
            found: Mutex::new(vec![]),
            gone: Mutex::new(vec![]),
            status: Mutex::new(String::new()),
            busy: tauri::async_runtime::Mutex::new(()),
            http: reqwest::Client::builder()
                .user_agent(concat!("SteamDrop/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("http client"),
            watcher: Mutex::new(None),
            update: Mutex::new(None),
            whats_new: Mutex::new(None),
            couch_lock: Mutex::new(None),
            restart_after_couch: Default::default(),
            restart_queued: Default::default(),
            steam_session: Default::default(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.data.lock().unwrap().settings.clone()
    }

    /// Picks up data.json when something else (another SteamDrop process) changed it, so every
    /// window lists the same games.
    pub fn reload_if_changed(&self) {
        let on_disk = modified(&self.dir);
        let mut seen = self.seen.lock().unwrap();
        if on_disk.is_some() && on_disk != *seen {
            if let Ok(Some(d)) = read(&self.dir) {
                *self.data.lock().unwrap() = d;
                crate::log::info("Reloaded SteamDrop's data because it changed on disk.");
            }
            *seen = on_disk;
        }
    }

    /// Runs `f` on the data and saves it.
    pub fn update<T>(&self, f: impl FnOnce(&mut Data) -> T) -> Result<T, String> {
        self.reload_if_changed();
        let mut d = self.data.lock().unwrap();
        let out = f(&mut d);
        save(&self.dir, &d)?;
        *self.seen.lock().unwrap() = modified(&self.dir);
        Ok(out)
    }

/// The Steam accounts games are added to: all of them, or the chosen ones.
    pub fn target_users(&self, steam: &std::path::Path) -> Vec<(String, PathBuf)> {
        let s = self.settings();
        crate::steam::users(steam).into_iter().filter(|(id, _)| s.all_accounts || s.accounts.contains(id)).collect()
    }

    pub fn steam_path(&self) -> Option<PathBuf> {
        let custom = self.settings().steam_path;
        if !custom.trim().is_empty() {
            return Some(PathBuf::from(custom.trim()));
        }
        crate::steam::detect_path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_or_partial_files_still_load() {
        let d: Data = serde_json::from_str(r#"{"settings":{"auto_add":true}}"#).unwrap();
        assert!(d.settings.auto_add && d.settings.close_to_tray);
        assert_eq!(d.settings.theme, "dark");
    }

    /// Settings, watched folders, exclusions, baselines and games survive every way SteamDrop restarts.
    #[test]
    fn everything_survives_restarts() {
        let dir = std::env::temp_dir().join(format!("steamdrop-persist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let app = App::open(dir.clone());
        app.update(|d| {
            d.settings.watched = vec![r"D:\Games".into(), r"D:\Games\Torrent".into()];
            d.settings.excluded = vec![r"D:\Games\Torrent\X".into()];
            d.settings.auto_add = true;
            d.settings.steam_restart_mode = "desktop".into();
            d.known.push(r"D:\Games\Old".into());
            d.games.push(Game { id: 7, name: "Sandustry".into(), exe: r"D:\Games\Sandustry\s.exe".into(), folder: r"D:\Games\Sandustry".into(), steam_appid: None, added: 1 });
        })
        .unwrap();
        let saved = serde_json::to_value(&*app.data.lock().unwrap()).unwrap();
        let same = |d: &Data, why: &str| assert_eq!(serde_json::to_value(d).unwrap(), saved, "lost data: {why}");

        // Tray app restart, Windows restart, or a new build installed over the old one: a new process opens the same folder.
        same(&App::open(dir.clone()).data.lock().unwrap(), "after a restart");
        // The process Steam starts and the startup entries read the same file.
        same(&load(&dir), "in another process");
        // The installer's helper (--add-to-steam) reads, changes one setting and saves.
        let mut d = load(&dir);
        d.settings.steam_shortcut = true;
        save(&dir, &d).unwrap();
        same(&load(&dir), "after the installer's helper");

        // A window holding an old copy changes one setting: nothing else moves.
        let s = app.settings().patched(&serde_json::json!({ "theme": "light" })).unwrap();
        assert_eq!((s.watched.len(), s.excluded.len(), s.auto_add, s.theme.as_str()), (2, 1, true, "light"));
        assert!(app.settings().patched(&serde_json::json!({ "auto_add": "maybe" })).is_err(), "a bad value is refused, not saved");

        // Two processes saving at the same moment never leave a broken file.
        let writers: Vec<_> = (0..2)
            .map(|_| {
                let dir = dir.clone();
                std::thread::spawn(move || (0..40).for_each(|_| save(&dir, &load(&dir)).unwrap()))
            })
            .collect();
        writers.into_iter().for_each(|w| w.join().unwrap());
        same(&load(&dir), "after saving from two processes at once");

        // A crash in the middle of a save leaves only a stray temporary file.
        std::fs::write(dir.join("data.json.999-0.tmp"), b"{ half writ").unwrap();
        same(&load(&dir), "after a crash during a save");

        // A data.json that can't be read is kept and recovered from the backup, never replaced by defaults.
        std::fs::write(dir.join("data.json"), b"{\"settings\": {\"watched\": [").unwrap();
        same(&App::open(dir.clone()).data.lock().unwrap(), "after data.json broke");
        let kept = std::fs::read_dir(&dir).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("data.json.broken-"));
        assert!(kept, "the unreadable file is kept");

        // Another process changes something: a running app picks it up before showing or saving anything.
        std::thread::sleep(std::time::Duration::from_millis(20));
        App::open(dir.clone()).update(|d| d.settings.theme = "light".into()).unwrap();
        app.reload_if_changed();
        assert_eq!(app.settings().theme, "light");
        assert_eq!(app.data.lock().unwrap().games.len(), 1);
    }

    #[test]
    fn paths_compare_loosely() {
        assert!(same_path(r"C:\Games\Foo\", "c:/games/foo"));
        assert!(!same_path(r"C:\Games\Foo", r"C:\Games\Foobar"));
    }
}
