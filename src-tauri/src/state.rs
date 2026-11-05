//! What SteamDrop remembers between runs, kept in %APPDATA%\SteamDrop\data.json.

use crate::exe::Candidate;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub watched: Vec<String>,
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            watched: vec![],
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
}

/// %APPDATA%\SteamDrop
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir());
    base.join("SteamDrop")
}

pub fn load(dir: &Path) -> Data {
    std::fs::read(dir.join("data.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, data: &Data) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("data.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(data).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, dir.join("data.json")).map_err(|e| e.to_string())
}

/// Folder paths compare case-insensitively on Windows.
pub fn same_path(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase();
    n(a) == n(b)
}

impl Data {
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
        let dir = data_dir();
        App {
            data: Mutex::new(load(&dir)),
            dir,
            found: Mutex::new(vec![]),
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
        }
    }

    pub fn settings(&self) -> Settings {
        self.data.lock().unwrap().settings.clone()
    }

    /// Runs `f` on the data and saves it.
    pub fn update<T>(&self, f: impl FnOnce(&mut Data) -> T) -> Result<T, String> {
        let mut d = self.data.lock().unwrap();
        let out = f(&mut d);
        save(&self.dir, &d)?;
        Ok(out)
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

    #[test]
    fn paths_compare_loosely() {
        assert!(same_path(r"C:\Games\Foo\", "c:/games/foo"));
        assert!(!same_path(r"C:\Games\Foo", r"C:\Games\Foobar"));
    }
}
