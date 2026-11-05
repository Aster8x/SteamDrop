//! Adding and removing games: the steps that tie the folder, the artwork and Steam together.

use crate::art;
use crate::exe;
use crate::log;
use crate::state::{same_path, App, Data, Found, Game};
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
    let settings = state.settings();
    let auto = settings.auto_add;
    let mut to_add = vec![];
    let mut ask = false;
    for folder in folders {
        // A folder created a moment ago and still empty: wait for its files.
        if std::fs::read_dir(&folder).map(|mut d| d.next().is_none()).unwrap_or(true) {
            continue;
        }
        let mut f = inspect(&folder);
        f.candidates.retain(|c| !settings.excluded.iter().any(|x| c.path.starts_with(x)));
        // One of SteamDrop's missing games, moved here: update it rather than adding it twice.
        let name = folder.file_name().map(|n| n.to_string_lossy().to_lowercase());
        let moved = state.data.lock().unwrap().games.iter().find(|g| !Path::new(&g.exe).is_file() && Path::new(&g.folder).file_name().map(|n| n.to_string_lossy().to_lowercase()) == name).cloned();
        if let Some((g, exe)) = moved.and_then(|g| relocated_exe(&g, &folder, &f).map(|e| (g, e))) {
            log::info(format!("Found \"{}\" in its new place: {}", g.name, f.folder));
            to_add.push(Add { folder: folder.clone(), exe, name: g.name, replaces: Some(g.id) });
            continue;
        }
        if f.candidates.first().is_some_and(|c| in_steam_elsewhere(&state, &c.path)) {
            log::info(format!("Skipped folder {}: its game file is already in Steam.", f.folder));
            let _ = state.update(|d| d.mark_known(&f.folder));
            continue;
        }
        if f.installer_only {
            log::info(format!("Skipped folder {}: installer only, not a ready-to-play game.", f.folder));
        } else if f.candidates.is_empty() {
            log::info(format!("Found folder {} but no game file (.exe) in it. Waiting for you to choose one.", f.folder));
        } else if auto {
            log::info(format!("New game found: \"{}\" in {}. Adding it automatically.", f.title, f.folder));
        } else {
            log::info(format!("New game found: \"{}\" in {}. Waiting for you to confirm.", f.title, f.folder));
        }
        if f.installer_only {
            let _ = state.update(|d| d.mark_known(&f.folder));
            notify(app, "Only an installer found", &format!("\"{}\" holds an installer, not a ready-to-play game. Install it first, then put the installed folder here.", f.title));
        }
        if auto && !f.installer_only && !f.candidates.is_empty() {
            to_add.push(Add { folder: folder.clone(), exe: f.candidates[0].path.clone(), name: f.title.clone(), replaces: None });
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
    /// The game this one replaces (the same game, found in a new place).
    pub replaces: Option<u32>,
}

/// True if a Steam shortcut that isn't one of SteamDrop's games already starts this exe.
fn in_steam_elsewhere(state: &App, exe: &Path) -> bool {
    let Some(steam) = state.steam_path() else { return false };
    let ours: Vec<u32> = state.data.lock().unwrap().games.iter().map(|g| g.id).collect();
    let want = steam::norm_exe(&exe.to_string_lossy());
    steam::users(&steam).iter().any(|(_, c)| steam::shortcut_exes(c).iter().any(|(id, e)| *e == want && !ours.contains(id)))
}

/// A moved game's file in its new folder: the same path inside the folder if it's there, else the best guess.
fn relocated_exe(game: &Game, folder: &Path, found: &Found) -> Option<PathBuf> {
    let same = Path::new(&game.exe).strip_prefix(&game.folder).ok().map(|rel| folder.join(rel)).filter(|p| p.is_file());
    same.or_else(|| found.candidates.first().map(|c| c.path.clone()))
}

/// "Find it" for a missing game: point it at the folder it was moved to.
pub async fn find_game(app: &AppHandle, id: u32, folder: PathBuf, restart: bool) -> Result<String, String> {
    let game = app.state::<App>().data.lock().unwrap().games.iter().find(|g| g.id == id).cloned();
    let game = game.ok_or("That game is no longer in the list.")?;
    let exe = relocated_exe(&game, &folder, &inspect(&folder)).ok_or("There's no game file (.exe) in that folder.")?;
    log::info(format!("You found \"{}\" in a new place: {}", game.name, folder.display()));
    add(app, vec![Add { folder, exe, name: game.name, replaces: Some(id) }], restart).await
}

fn steam_and_users(state: &App) -> Result<(PathBuf, Vec<(String, PathBuf)>), String> {
    let steam = state.steam_path().ok_or("Steam was not found. Set its folder in Settings.")?;
    if steam::users(&steam).is_empty() {
        return Err("No Steam account found on this PC. Open Steam and sign in once, then try again.".into());
    }
    let users = state.target_users(&steam);
    if users.is_empty() {
        return Err("No Steam account is chosen. Pick one in Settings → Steam → \"Add games to\".".into());
    }
    Ok((steam, users))
}

/// Adds games to Steam: artwork first (while Steam keeps running), then one quick Steam restart.
pub async fn add(app: &AppHandle, items: Vec<Add>, restart: bool) -> Result<String, String> {
    let names = items.iter().map(|i| format!("\"{}\"", i.name)).collect::<Vec<_>>().join(", ");
    let result = add_inner(app, items, restart).await;
    if let Err(e) = &result {
        log::problem(format!("Could not add {names}: {e}"));
    }
    result
}

async fn add_inner(app: &AppHandle, items: Vec<Add>, restart: bool) -> Result<String, String> {
    let state = app.state::<App>();
    let _guard = state.busy.lock().await;
    let (steam_dir, users) = steam_and_users(&state)?;
    let settings = state.settings();
    let sgdb_key = crate::secrets::sgdb_key();
    let mut games = vec![];
    let mut art_sets = vec![];
    for item in &items {
        if !item.exe.is_file() {
            return Err(format!("The game file {} no longer exists.", item.exe.display()));
        }
        if in_steam_elsewhere(&state, &item.exe) {
            return Err(format!("\"{}\" is already in Steam (game file {}), so SteamDrop won't add it twice.", item.name, item.exe.display()));
        }
        let id = steam::shortcut_id(&steam::quote(&item.exe), &item.name);
        let folder_str = item.folder.to_string_lossy().into_owned();
        let old = state.data.lock().unwrap().games.iter().find(|g| g.id == id).cloned();
        set_status(app, &format!("Looking up \"{}\"...", item.name));
        let mut steam_appid = old.and_then(|g| g.steam_appid).or_else(|| exe::find_appid(&item.folder));
        if steam_appid.is_none() {
            steam_appid = crate::store::search(&state.http, &item.name).await;
        }
        match steam_appid {
            Some(a) => log::info(format!("\"{}\" is Steam App ID {a}.", item.name)),
            None => log::info(format!("\"{}\" was not found in the Steam store.", item.name)),
        }
        set_status(app, &format!("Getting artwork for \"{}\"...", item.name));
        let req = art::Request {
            id,
            title: &item.name,
            folder: &item.folder,
            steam_appid,
            sgdb_key: &sgdb_key,
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

    // Re-adding a folder under a new name or exe gives it a new id; drop the old shortcut.
    let (replaced, snapshot): (Vec<u32>, Data) = {
        let d = state.data.lock().unwrap();
        let mut ids: Vec<u32> = d.games.iter().filter(|o| games.iter().any(|g| g.id != o.id && same_path(&g.folder, &o.folder))).map(|o| o.id).collect();
        ids.extend(items.iter().filter_map(|i| i.replaces).filter(|old| !games.iter().any(|g| g.id == *old)));
        (ids, d.clone())
    };
    let was_running = steam::is_running();
    let restarting = was_running && restart;
    let dir = state.dir.clone();
    let games2 = games.clone();
    let app2 = app.clone();
    let users2 = users.clone();
    let replaced2 = replaced.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        if restarting {
            set_status(&app2, "Closing Steam for a moment...");
            log::info("Closing Steam so it picks up the new shortcuts.");
            steam::shutdown(&steam_dir)?;
        }
        if !was_running || restarting {
            apply_pending(&dir, &snapshot, &steam::users(&steam_dir), &users2)?;
        }
        set_status(&app2, "Adding to Steam...");
        let mut result = Ok(());
        for (user, config) in &users {
            let grid = steam::grid_dir(config);
            if !replaced2.is_empty() {
                if let Err(e) = steam::remove(config, &dir, user, &replaced2) {
                    result = Err(e);
                }
                replaced2.iter().for_each(|id| art::uninstall(&grid, *id));
            }
            let shortcuts: Vec<steam::NewShortcut> = games2
                .iter()
                .zip(&art_sets)
                .map(|(g, art)| {
                    let icon = art::install(&grid, g.id, art).unwrap_or_else(|| PathBuf::from(&g.exe));
                    steam::NewShortcut { id: g.id, name: &g.name, exe: Path::new(&g.exe), icon: icon.to_string_lossy().into_owned(), launch_options: "" }
                })
                .collect();
            if let Err(e) = steam::add(config, &dir, user, &shortcuts) {
                result = Err(e);
            }
        }
        if restarting {
            set_status(&app2, "Starting Steam...");
            log::info("Starting Steam in Big Picture.");
            steam::start(&steam_dir, true)?;
        }
        result
    })
    .await
    .map_err(|e| e.to_string())??;

    state.update(|d| {
        for g in &games {
            d.games.retain(|x| x.id != g.id && !same_path(&x.folder, &g.folder) && !replaced.contains(&x.id));
            d.games.push(g.clone());
            d.mark_known(&g.folder);
        }
        if !was_running || restarting {
            d.pending_removals.clear();
            d.pending_adds.clear();
        } else {
            d.pending_adds.extend(games.iter().map(|g| g.id));
            d.pending_removals.extend(&replaced);
        }
    })?;
    state.found.lock().unwrap().retain(|f| !games.iter().any(|g| same_path(&g.folder, &f.folder)));
    for g in &games {
        log::info(format!("Added \"{}\" to Steam. Game file: {}", g.name, g.exe));
    }
    if was_running && !restart {
        log::info("Steam is open and was not restarted, so the change shows after Steam restarts.");
    }
    let names = games.iter().map(|g| format!("\"{}\"", g.name)).collect::<Vec<_>>().join(", ");
    let msg = if was_running && !restart {
        format!("Added {names}. Restart Steam to see it.")
    } else {
        format!("Added {names} to Steam.")
    };
    notify(app, "Added to Steam", &msg);
    if settings.chime {
        crate::chime::play(settings.chime_volume);
    }
    set_status(app, &msg);
    emit_state(app);
    Ok(msg)
}

/// Removes shortcuts and their art from Steam. Game files are never touched.
pub async fn remove(app: &AppHandle, ids: Vec<u32>, restart: bool) -> Result<String, String> {
    let names: Vec<String> = {
        let d = app.state::<App>().data.lock().unwrap().clone();
        d.games.iter().filter(|g| ids.contains(&g.id)).map(|g| format!("\"{}\"", g.name)).collect()
    };
    let result = remove_inner(app, ids, restart).await;
    match &result {
        Ok(_) => log::info(format!("Removed {} from Steam. Game files were not touched.", names.join(", "))),
        Err(e) => log::problem(format!("Could not remove {}: {e}", names.join(", "))),
    }
    result
}

async fn remove_inner(app: &AppHandle, ids: Vec<u32>, restart: bool) -> Result<String, String> {
    let state = app.state::<App>();
    let _guard = state.busy.lock().await;
    let (steam_dir, users) = steam_and_users(&state)?;
    let was_running = steam::is_running();
    let restarting = was_running && restart;
    let dir = state.dir.clone();
    let ids2 = ids.clone();
    let app2 = app.clone();
    let mut snapshot = state.data.lock().unwrap().clone();
    snapshot.pending_adds.retain(|id| !ids.contains(id));
    tauri::async_runtime::spawn_blocking(move || {
        if restarting {
            set_status(&app2, "Closing Steam for a moment...");
            steam::shutdown(&steam_dir)?;
        }
        let r = remove_blocking(&steam_dir, &dir, &ids2, false);
        if !was_running || restarting {
            apply_pending(&dir, &snapshot, &steam::users(&steam_dir), &users)?;
        }
        if restarting {
            set_status(&app2, "Starting Steam...");
            steam::start(&steam_dir, false)?;
        }
        r
    })
    .await
    .map_err(|e| e.to_string())??;
    state.update(|d| {
        d.games.retain(|g| !ids.contains(&g.id));
        d.pending_adds.retain(|id| !ids.contains(id));
        if was_running && !restart {
            d.pending_removals.extend(&ids);
        } else {
            d.pending_removals.clear();
            d.pending_adds.clear();
        }
    })?;
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

const SELF_NAME: &str = "SteamDrop";
/// SteamDrop's own artwork (sources in assets/steam-art/*.svg).
const SELF_ART: [(art::Slot, &[u8]); 5] = [
    (art::Slot::Portrait, include_bytes!("../steam-art/portrait.png")),
    (art::Slot::Wide, include_bytes!("../steam-art/wide.png")),
    (art::Slot::Hero, include_bytes!("../steam-art/hero.png")),
    (art::Slot::Logo, include_bytes!("../steam-art/logo.png")),
    (art::Slot::Icon, include_bytes!("../steam-art/icon.png")),
];

fn self_shortcut() -> Result<(u32, PathBuf), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok((steam::shortcut_id(&steam::quote(&exe), SELF_NAME), exe))
}

/// Adds or removes SteamDrop's own shortcut for every Steam account. Started from Steam it opens
/// Couch mode in Big Picture and the window in desktop Steam (see couch.rs).
pub fn set_self_shortcut(steam_dir: &Path, data_dir: &Path, on: bool) -> Result<(), String> {
    let settings = crate::state::load(data_dir).settings;
    let (id, exe) = self_shortcut()?;
    let cache = art::cache_dir(data_dir, id);
    let mut files = vec![];
    if on {
        std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        for (slot, bytes) in SELF_ART {
            let path = cache.join(format!("{}.png", slot.name()));
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            files.push((slot, path));
        }
    }
    for (user, config) in steam::users(steam_dir) {
        let grid = steam::grid_dir(&config);
        let chosen = settings.all_accounts || settings.accounts.contains(&user);
        if on && chosen {
            let icon = art::install(&grid, id, &files).unwrap_or_else(|| exe.clone());
            let s = steam::NewShortcut { id, name: SELF_NAME, exe: &exe, icon: icon.to_string_lossy().into_owned(), launch_options: "--from-steam" };
            steam::add(&config, data_dir, &user, &[s])?;
        } else {
            steam::remove(&config, data_dir, &user, &[id])?;
            art::uninstall(&grid, id);
        }
    }
    log::info(if on { "Added SteamDrop to Steam (it opens Couch mode)." } else { "Removed SteamDrop from Steam." });
    Ok(())
}

/// Startup housekeeping while Steam is closed: finish removals Steam may have undone,
/// and put SteamDrop's own shortcut back if the setting wants it and it's missing.
pub fn startup_repair(app: &AppHandle) {
    let state = app.state::<App>();
    let Some(steam_dir) = state.steam_path() else { return };
    if steam::is_running() {
        return;
    }
    let snapshot = state.data.lock().unwrap().clone();
    if apply_pending(&state.dir, &snapshot, &steam::users(&steam_dir), &state.target_users(&steam_dir)).is_ok() {
        let _ = state.update(|d| {
            d.pending_removals.clear();
            d.pending_adds.clear();
        });
    }
    if state.settings().steam_shortcut {
        let missing = self_shortcut().is_ok_and(|(id, _)| state.target_users(&steam_dir).iter().any(|(_, c)| !steam::present_ids(c).contains(&id)));
        if missing {
            if let Err(e) = set_self_shortcut(&steam_dir, &state.dir, true) {
                log::problem(format!("Could not add SteamDrop to Steam: {e}"));
            }
        }
    }
}

/// Writes again what was changed while Steam was running: removals Steam may have undone and
/// games it may have dropped. Call only while Steam is closed.
/// Removals go to every account; adds go to `targets` (the chosen accounts).
pub fn apply_pending(data_dir: &Path, data: &Data, all: &[(String, PathBuf)], targets: &[(String, PathBuf)]) -> Result<(), String> {
    if data.pending_removals.is_empty() && data.pending_adds.is_empty() {
        return Ok(());
    }
    for (user, config) in all {
        if !data.pending_removals.is_empty() {
            steam::remove(config, data_dir, user, &data.pending_removals)?;
            data.pending_removals.iter().for_each(|id| art::uninstall(&steam::grid_dir(config), *id));
        }
    }
    for (user, config) in targets {
        let grid = steam::grid_dir(config);
        let present = steam::present_ids(config);
        let missing: Vec<&Game> = data.games.iter().filter(|g| data.pending_adds.contains(&g.id) && !present.contains(&g.id)).collect();
        let shortcuts: Vec<steam::NewShortcut> = missing
            .iter()
            .map(|g| {
                let art: Vec<(art::Slot, PathBuf)> = art::SLOTS.iter().filter_map(|s| art::cached(data_dir, g.id, *s).map(|p| (*s, p))).collect();
                let icon = art::install(&grid, g.id, &art).unwrap_or_else(|| PathBuf::from(&g.exe));
                steam::NewShortcut { id: g.id, name: &g.name, exe: Path::new(&g.exe), icon: icon.to_string_lossy().into_owned(), launch_options: "" }
            })
            .collect();
        if !shortcuts.is_empty() {
            steam::add(config, data_dir, user, &shortcuts)?;
        }
    }
    log::info(format!(
        "Applied changes made while Steam was running ({} removed, {} added or changed).",
        data.pending_removals.len(),
        data.pending_adds.len()
    ));
    Ok(())
}

/// Closes Steam, applies pending changes, and starts it again (in Big Picture if asked).
pub fn restart_steam_applying(app: &AppHandle, big_picture: bool) -> Result<(), String> {
    let state = app.state::<App>();
    let steam_dir = state.steam_path().ok_or("Steam was not found. Set its folder in Settings.")?;
    set_status(app, "Restarting Steam...");
    steam::shutdown(&steam_dir)?;
    let snapshot = state.data.lock().unwrap().clone();
    apply_pending(&state.dir, &snapshot, &steam::users(&steam_dir), &state.target_users(&steam_dir))?;
    state.update(|d| {
        d.pending_removals.clear();
        d.pending_adds.clear();
    })?;
    steam::start(&steam_dir, big_picture)?;
    log::info(if big_picture { "Restarted Steam in Big Picture." } else { "Restarted Steam." });
    set_status(app, "Steam restarted.");
    emit_state(app);
    Ok(())
}

/// Couch mode's "Delete game": the folder goes to the Recycle Bin, then the shortcut and art go.
pub async fn delete_game(app: &AppHandle, id: u32) -> Result<String, String> {
    let game = app.state::<App>().data.lock().unwrap().games.iter().find(|g| g.id == id).cloned();
    let game = game.ok_or("That game is no longer in the list.")?;
    let folder = PathBuf::from(&game.folder);
    let recycled = tauri::async_runtime::spawn_blocking(move || crate::recycle::recycle(&folder)).await.map_err(|e| e.to_string())?;
    if let Err(e) = recycled {
        log::problem(format!("Could not delete \"{}\": {e}", game.name));
        return Err(e);
    }
    log::info(format!("Moved the folder of \"{}\" to the Recycle Bin: {}", game.name, game.folder));
    remove(app, vec![id], false).await?;
    Ok(format!("\"{}\" was moved to the Recycle Bin and removed from Steam.", game.name))
}

/// `SteamDrop.exe --add-to-steam` (from the installer): turn the setting on and add the shortcut.
pub fn add_to_steam_from_installer() {
    let dir = crate::state::data_dir();
    let mut data = crate::state::load(&dir);
    data.settings.steam_shortcut = true;
    let _ = crate::state::save(&dir, &data);
    let custom = data.settings.steam_path.trim();
    let steam_dir = if custom.is_empty() { steam::detect_path() } else { Some(PathBuf::from(custom)) };
    match steam_dir.map(|s| set_self_shortcut(&s, &dir, true)) {
        Some(Ok(())) => {}
        Some(Err(e)) => log::problem(format!("Installer: could not add SteamDrop to Steam: {e}")),
        None => log::info("Installer: Steam was not found, so SteamDrop wasn't added to it yet."),
    }
}

/// `SteamDrop.exe --remove-from-steam` (from the uninstaller): SteamDrop's own shortcut would point
/// at a deleted program, so it always goes.
pub fn remove_self_from_installer() {
    let dir = crate::state::data_dir();
    let data = crate::state::load(&dir);
    let custom = data.settings.steam_path.trim();
    let steam_dir = if custom.is_empty() { steam::detect_path() } else { Some(PathBuf::from(custom)) };
    if let Some(Err(e)) = steam_dir.map(|s| set_self_shortcut(&s, &dir, false)) {
        log::problem(format!("Uninstall: could not remove SteamDrop from Steam: {e}"));
    }
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
            log::problem(format!("Uninstall: could not remove SteamDrop's games from Steam: {e}"));
            return;
        }
        log::info(format!("Uninstall: removed {} game(s) from Steam.", ids.len()));
        let mut data = data;
        data.games.clear();
        let _ = crate::state::save(&dir, &data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moved_game_keeps_its_file() {
        let base = std::env::temp_dir().join(format!("steamdrop-moved-{}", std::process::id()));
        let new = base.join("Games/Hades");
        std::fs::create_dir_all(new.join("x64")).unwrap();
        std::fs::write(new.join("x64/Hades.exe"), b"x").unwrap();
        let game = Game { id: 1, name: "Hades".into(), exe: r"D:\Old\Hades\x64\Hades.exe".replace('\\', std::path::MAIN_SEPARATOR_STR), folder: r"D:\Old\Hades".replace('\\', std::path::MAIN_SEPARATOR_STR), steam_appid: None, added: 1 };
        let mut found = inspect(&new);
        assert_eq!(relocated_exe(&game, &new, &found), Some(new.join("x64/Hades.exe")));
        // Not at the same place inside the folder: fall back to the best guess.
        found.candidates.truncate(1);
        let other = Game { exe: game.exe.replace("x64", "bin"), ..game };
        assert_eq!(relocated_exe(&other, &new, &found), found.candidates.first().map(|c| c.path.clone()));
    }

    #[test]
    fn pending_changes_are_written_again() {
        let base = std::env::temp_dir().join(format!("steamdrop-pending-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let config = base.join("userdata/42/config");
        std::fs::create_dir_all(&config).unwrap();
        let data_dir = base.join("data");
        let exe = base.join("games/A/a.exe");
        // Steam wrote its old list back: the removed game 5 is there again, the added game 9 is gone.
        let old = steam::NewShortcut { id: 5, name: "Old", exe: &exe, icon: String::new(), launch_options: "" };
        steam::add(&config, &data_dir, "42", &[old]).unwrap();
        std::fs::create_dir_all(art::cache_dir(&data_dir, 9)).unwrap();
        std::fs::write(art::cache_dir(&data_dir, 9).join("portrait.png"), b"png").unwrap();
        let game = Game { id: 9, name: "A".into(), exe: exe.to_string_lossy().into(), folder: String::new(), steam_appid: None, added: 1 };
        let data = Data { games: vec![game], pending_removals: vec![5], pending_adds: vec![9], ..Default::default() };
        let users = vec![("42".to_string(), config.clone())];
        apply_pending(&data_dir, &data, &users, &users).unwrap();
        assert_eq!(steam::present_ids(&config), vec![9]);
        assert!(config.join("grid/9p.png").is_file());
        // Running it again changes nothing.
        apply_pending(&data_dir, &data, &users, &users).unwrap();
        assert_eq!(steam::present_ids(&config), vec![9]);
    }
}
