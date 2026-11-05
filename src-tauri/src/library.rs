//! Adding and removing games: the steps that tie the folder, the artwork and Steam together.

use crate::art;
use crate::exe;
use crate::state::{same_path, App, Found, Game};
use crate::steam;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

pub fn emit_state(app: &AppHandle) {
    let _ = app.emit("changed", ());
}

pub fn set_status(app: &AppHandle, text: &str) {
    *app.state::<App>().status.lock().unwrap() = text.to_string();
    let _ = app.emit("status", text);
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if app.state::<App>().settings().notify {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

pub fn inspect(folder: &Path) -> Found {
    let name = folder.file_name().unwrap_or_default().to_string_lossy();
    Found {
        folder: folder.to_string_lossy().into_owned(),
        title: exe::clean_title(&name),
        candidates: exe::candidates(folder),
        installer_only: exe::installer_only(folder),
    }
}

/// New folders from the watcher: add them right away in automatic mode, otherwise ask.
pub fn found(app: &AppHandle, folders: Vec<PathBuf>) {
    let state = app.state::<App>();
    let auto = state.settings().auto_add;
    let mut to_add = vec![];
    let mut ask = false;
    for folder in folders {
        // A folder created a moment ago and still empty: wait for its files.
        if std::fs::read_dir(&folder).map(|mut d| d.next().is_none()).unwrap_or(true) {
            continue;
        }
        let f = inspect(&folder);
        if f.installer_only {
            let _ = state.update(|d| d.mark_known(&f.folder));
            notify(app, "Only an installer found", &format!("\"{}\" holds an installer, not a ready-to-play game. Install it first, then put the installed folder here.", f.title));
        }
        if auto && !f.installer_only && !f.candidates.is_empty() {
            to_add.push(Add { folder: folder.clone(), exe: f.candidates[0].path.clone(), name: f.title.clone() });
            continue;
        }
        if !auto && !f.installer_only && !f.candidates.is_empty() {
            notify(app, "New game found", &format!("Open SteamDrop to add \"{}\" to Steam.", f.title));
        }
        let mut list = state.found.lock().unwrap();
        list.retain(|x| !same_path(&x.folder, &f.folder));
        list.push(f);
        ask = true;
    }
    if ask && !auto {
        crate::show_window(app);
    }
    emit_state(app);
    if !to_add.is_empty() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let restart = app.state::<App>().settings().auto_restart;
            if let Err(e) = add(&app, to_add, restart).await {
                notify(&app, "Could not add a game", &e);
                set_status(&app, &e);
            }
        });
    }
}

pub struct Add {
    pub folder: PathBuf,
    pub exe: PathBuf,
    pub name: String,
}

fn steam_and_users(state: &App) -> Result<(PathBuf, Vec<(String, PathBuf)>), String> {
    let steam = state.steam_path().ok_or("Steam was not found. Set its folder in Settings.")?;
    let users = steam::users(&steam);
    if users.is_empty() {
        return Err("No Steam account found on this PC. Open Steam and sign in once, then try again.".into());
    }
    Ok((steam, users))
}

/// Adds games to Steam: artwork first (while Steam keeps running), then one quick Steam restart.
pub async fn add(app: &AppHandle, items: Vec<Add>, restart: bool) -> Result<String, String> {
    let state = app.state::<App>();
    let _guard = state.busy.lock().await;
    let (steam_dir, users) = steam_and_users(&state)?;
    let settings = state.settings();
    let mut games = vec![];
    let mut art_sets = vec![];
    for item in &items {
        if !item.exe.is_file() {
            return Err(format!("The game file {} no longer exists.", item.exe.display()));
        }
        let id = steam::shortcut_id(&steam::quote(&item.exe), &item.name);
        let folder_str = item.folder.to_string_lossy().into_owned();
        let old = state.data.lock().unwrap().games.iter().find(|g| g.id == id).cloned();
        set_status(app, &format!("Looking up \"{}\"...", item.name));
        let mut steam_appid = old.and_then(|g| g.steam_appid).or_else(|| exe::find_appid(&item.folder));
        if steam_appid.is_none() {
            steam_appid = crate::store::search(&state.http, &item.name).await;
        }
        set_status(app, &format!("Getting artwork for \"{}\"...", item.name));
        let req = art::Request {
            id,
            title: &item.name,
            folder: &item.folder,
            steam_appid,
            sgdb_key: &settings.sgdb_key,
            copy_to_game_folder: settings.art_in_game_folder,
        };
        art_sets.push(art::fetch(&state.http, &state.dir, &req).await);
        games.push(Game {
            id,
            name: item.name.clone(),
            exe: item.exe.to_string_lossy().into_owned(),
            folder: folder_str,
            steam_appid,
            added: steam::now(),
        });
    }

    let was_running = steam::is_running();
    let restarting = was_running && restart;
    let dir = state.dir.clone();
    let games2 = games.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        if restarting {
            set_status(&app2, "Closing Steam for a moment...");
            steam::shutdown(&steam_dir)?;
        }
        set_status(&app2, "Adding to Steam...");
        let mut result = Ok(());
        for (user, config) in &users {
            let grid = steam::grid_dir(config);
            let shortcuts: Vec<steam::NewShortcut> = games2
                .iter()
                .zip(&art_sets)
                .map(|(g, art)| {
                    let icon = art::install(&grid, g.id, art).unwrap_or_else(|| PathBuf::from(&g.exe));
                    steam::NewShortcut { id: g.id, name: &g.name, exe: Path::new(&g.exe), icon: icon.to_string_lossy().into_owned() }
                })
                .collect();
            if let Err(e) = steam::add(config, &dir, user, &shortcuts) {
                result = Err(e);
            }
        }
        if restarting {
            set_status(&app2, "Starting Steam...");
            steam::start(&steam_dir, true)?;
        }
        result
    })
    .await
    .map_err(|e| e.to_string())??;

    state.update(|d| {
        for g in &games {
            d.games.retain(|x| x.id != g.id && !same_path(&x.folder, &g.folder));
            d.games.push(g.clone());
            d.mark_known(&g.folder);
        }
    })?;
    state.found.lock().unwrap().retain(|f| !games.iter().any(|g| same_path(&g.folder, &f.folder)));
    let names = games.iter().map(|g| format!("\"{}\"", g.name)).collect::<Vec<_>>().join(", ");
    let msg = if was_running && !restart {
        format!("Added {names}. Restart Steam to see it.")
    } else {
        format!("Added {names} to Steam.")
    };
    notify(app, "Added to Steam", &msg);
    set_status(app, &msg);
    emit_state(app);
    Ok(msg)
}

/// Removes shortcuts and their art from Steam. Game files are never touched.
pub async fn remove(app: &AppHandle, ids: Vec<u32>, restart: bool) -> Result<String, String> {
    let state = app.state::<App>();
    let _guard = state.busy.lock().await;
    let (steam_dir, _) = steam_and_users(&state)?;
    let was_running = steam::is_running();
    let restarting = was_running && restart;
    let dir = state.dir.clone();
    let ids2 = ids.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if restarting {
            set_status(&app2, "Closing Steam for a moment...");
        }
        let r = remove_blocking(&steam_dir, &dir, &ids2, restarting);
        if restarting {
            set_status(&app2, "Starting Steam...");
        }
        r
    })
    .await
    .map_err(|e| e.to_string())??;
    state.update(|d| d.games.retain(|g| !ids.contains(&g.id)))?;
    let msg = if was_running && !restart { "Removed. Restart Steam to see the change." } else { "Removed from Steam." };
    set_status(app, msg);
    emit_state(app);
    Ok(msg.into())
}

/// The shared removal steps, also used by the uninstaller (which has no window).
fn remove_blocking(steam_dir: &Path, data_dir: &Path, ids: &[u32], restart: bool) -> Result<(), String> {
    if restart {
        steam::shutdown(steam_dir)?;
    }
    let mut result = Ok(());
    for (user, config) in steam::users(steam_dir) {
        if let Err(e) = steam::remove(&config, data_dir, &user, ids) {
            result = Err(e);
        }
        for id in ids {
            art::uninstall(&steam::grid_dir(&config), *id);
        }
    }
    for id in ids {
        let _ = std::fs::remove_dir_all(art::cache_dir(data_dir, *id));
    }
    if restart {
        steam::start(steam_dir, false)?;
    }
    result
}

/// `SteamDrop.exe --uninstall-cleanup`: removes every game SteamDrop added, and their art.
pub fn uninstall_cleanup() {
    let dir = crate::state::data_dir();
    let data = crate::state::load(&dir);
    let custom = data.settings.steam_path.trim();
    let steam_dir = if custom.is_empty() { steam::detect_path() } else { Some(PathBuf::from(custom)) };
    let ids: Vec<u32> = data.games.iter().map(|g| g.id).collect();
    if let (Some(steam_dir), false) = (steam_dir, ids.is_empty()) {
        if let Err(e) = remove_blocking(&steam_dir, &dir, &ids, steam::is_running()) {
            eprintln!("{e}");
            return;
        }
        let mut data = data;
        data.games.clear();
        let _ = crate::state::save(&dir, &data);
    }
}
