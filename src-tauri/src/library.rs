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
    let mut asked = vec![];
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
            to_add.push(Add { folder: folder.clone(), exe, name: g.name, replaces: Some(g.id), fresh_art: false });
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
            to_add.push(Add { folder: folder.clone(), exe: f.candidates[0].path.clone(), name: f.title.clone(), replaces: None, fresh_art: false });
            continue;
        }
        if !auto && !f.installer_only && !f.candidates.is_empty() {
            asked.push(f.title.clone());
        }
        let mut list = state.found.lock().unwrap();
        list.retain(|x| !same_path(&x.folder, &f.folder));
        list.push(f);
    }
    match asked.as_slice() {
        [] => {}
        [one] => notify(app, "New game found", &format!("Open SteamDrop to add \"{one}\" to Steam.")),
        many => notify(app, &format!("{} new games found", many.len()), "Open SteamDrop to add them to Steam."),
    }
    if !auto && !state.found.lock().unwrap().is_empty() {
        crate::couch::bring_up(app);
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

/// Game folders that disappeared (deleted, or moved out of the watched folders), batched by the
/// watcher. Nothing is removed from Steam by itself: the games wait for "Remove selected" or
/// "Keep in Steam". A game whose folder reappeared under the same name in `new` was moved, not deleted.
pub fn gone(app: &AppHandle, folders: Vec<PathBuf>, new: &[PathBuf]) {
    let state = app.state::<App>();
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase());
    let games: Vec<Game> = state
        .data
        .lock()
        .unwrap()
        .games
        .iter()
        .filter(|g| folders.iter().any(|f| same_path(&g.folder, &f.to_string_lossy())) && !state.deleting.lock().unwrap().contains(&g.id))
        .filter(|g| !Path::new(&g.exe).is_file() && !new.iter().any(|n| name(n) == name(Path::new(&g.folder))))
        .cloned()
        .collect();
    if games.is_empty() {
        return;
    }
    for g in &games {
        log::info(format!("The folder of \"{}\" is gone: {}. Asking whether to remove it from Steam.", g.name, g.folder));
    }
    {
        let mut list = state.gone.lock().unwrap();
        list.retain(|id| !games.iter().any(|g| g.id == *id));
        list.extend(games.iter().map(|g| g.id));
    }
    let title = if games.len() == 1 { "A game folder was deleted".to_string() } else { format!("{} game folders were deleted", games.len()) };
    notify(app, &title, "Open SteamDrop to choose whether to remove them from Steam.");
    crate::couch::bring_up(app);
    emit_state(app);
}

pub struct Add {
    pub folder: PathBuf,
    pub exe: PathBuf,
    pub name: String,
    /// The game this one replaces (the same game, found in a new place).
    pub replaces: Option<u32>,
    /// "Find artwork again": forget the saved art and get it anew.
    pub fresh_art: bool,
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
    add(app, vec![Add { folder, exe, name: game.name, replaces: Some(id), fresh_art: false }], restart).await
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
    // One game that can't be added doesn't stop the others in the same batch.
    let mut skipped = vec![];
    let mut kept = vec![];
    let mut all_known = true;
    for item in &items {
        if !item.exe.is_file() {
            skipped.push(format!("\"{}\" was skipped: its game file {} no longer exists.", item.name, item.exe.display()));
            continue;
        }
        if in_steam_elsewhere(&state, &item.exe) {
            skipped.push(format!("\"{}\" was skipped: it's already in Steam (game file {}), so SteamDrop won't add it twice.", item.name, item.exe.display()));
            continue;
        }
        kept.push(item);
        let id = steam::shortcut_id(&steam::quote(&item.exe), &item.name);
        let folder_str = item.folder.to_string_lossy().into_owned();
        let old = state.data.lock().unwrap().games.iter().find(|g| g.id == id).cloned();
        all_known &= old.is_some() || item.replaces.is_some();
        if item.fresh_art {
            let _ = std::fs::remove_dir_all(art::cache_dir(&state.dir, id));
            let _ = std::fs::remove_dir_all(item.folder.join(art::GAME_FOLDER_ART));
        }
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

    skipped.iter().for_each(|s| log::problem(s));
    if games.is_empty() {
        return Err(skipped.join(" "));
    }
    // Re-adding a folder under a new name or exe gives it a new id; drop the old shortcut.
    let (replaced, snapshot): (Vec<u32>, Data) = {
        let d = state.data.lock().unwrap();
        let mut ids: Vec<u32> = d.games.iter().filter(|o| games.iter().any(|g| g.id != o.id && same_path(&g.folder, &o.folder))).map(|o| o.id).collect();
        ids.extend(kept.iter().filter_map(|i| i.replaces).filter(|old| !games.iter().any(|g| g.id == *old)));
        (ids, d.clone())
    };
    // With Steam open the shortcuts are written now and kept as pending changes, because Steam
    // writes its own list back when it closes; the (shared) restart writes them again.
    let was_running = steam::is_running();
    let dir = state.dir.clone();
    let games2 = games.clone();
    let app2 = app.clone();
    let users2 = users.clone();
    let replaced2 = replaced.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        if !was_running {
            apply_pending(&steam_dir, &dir, &snapshot, &steam::users(&steam_dir), &users2)?;
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
        if !was_running {
            d.clear_pending();
        } else {
            d.pending_adds.extend(games.iter().map(|g| g.id));
            d.pending_removals.extend(&replaced);
        }
    })?;
    drop(_guard);
    state.found.lock().unwrap().retain(|f| !games.iter().any(|g| same_path(&g.folder, &f.folder)));
    for g in &games {
        log::info(format!("{} \"{}\" {} Steam. Game file: {}", if all_known { "Updated" } else { "Added" }, g.name, if all_known { "in" } else { "to" }, g.exe));
    }
    if was_running && !restart {
        log::info("Steam is open and was not restarted, so the change shows after Steam restarts.");
    }
    let names = games.iter().map(|g| format!("\"{}\"", g.name)).collect::<Vec<_>>().join(", ");
    // New artwork or a changed game file for games already added isn't "added" (and gets no sound).
    let mut msg = match (all_known, was_running && !restart) {
        (true, true) => format!("Updated {names}. Restart Steam to see it."),
        (true, false) => format!("Updated {names} in Steam."),
        (false, true) => format!("Added {names}. Restart Steam to see it."),
        (false, false) => format!("Added {names} to Steam."),
    };
    skipped.iter().for_each(|s| msg = format!("{msg} {s}"));
    if !all_known {
        notify(app, "Added to Steam", &msg);
        if settings.chime {
            crate::chime::play(settings.chime_volume, &settings.chime_sound);
        }
    }
    set_status(app, &msg);
    emit_state(app);
    if was_running && restart {
        request_restart(app);
    }
    Ok(msg)
}

/// Removes shortcuts and their art from Steam. Game files are never touched.
pub async fn remove(app: &AppHandle, ids: Vec<u32>, restart: bool) -> Result<String, String> {
    let names: Vec<String> = {
        let d = app.state::<App>().data.lock().unwrap().clone();
        d.games.iter().filter(|g| ids.contains(&g.id)).map(|g| format!("\"{}\"", g.name)).collect()
    };
    let result = remove_inner(app, ids.clone(), restart).await;
    if result.is_ok() {
        app.state::<App>().gone.lock().unwrap().retain(|id| !ids.contains(id));
    }
    match &result {
        Ok(_) => log::info(format!("Removed {} from Steam. Game files were not touched.", names.join(", "))),
        Err(e) => log::problem(format!("Could not remove {}: {e}", names.join(", "))),
    }
    result
}

async fn remove_inner(app: &AppHandle, ids: Vec<u32>, restart: bool) -> Result<String, String> {
    let state = app.state::<App>();
    let _guard = state.busy.lock().await;
    // Without Steam (uninstalled, or not found) a game can still leave SteamDrop's list.
    let steam_dir = state.steam_path();
    let users = steam_dir.as_deref().map(|s| state.target_users(s)).unwrap_or_default();
    let was_running = steam::is_running();
    let dir = state.dir.clone();
    let ids2 = ids.clone();
    let mut snapshot = state.data.lock().unwrap().clone();
    snapshot.pending_adds.retain(|id| !ids.contains(id));
    tauri::async_runtime::spawn_blocking(move || {
        let Some(steam_dir) = steam_dir else {
            ids2.iter().for_each(|id| drop(std::fs::remove_dir_all(art::cache_dir(&dir, *id))));
            return Ok(());
        };
        let r = remove_blocking(&steam_dir, &dir, &ids2, None);
        if !was_running {
            apply_pending(&steam_dir, &dir, &snapshot, &steam::users(&steam_dir), &users)?;
        }
        r
    })
    .await
    .map_err(|e| e.to_string())??;
    state.update(|d| {
        d.games.retain(|g| !ids.contains(&g.id));
        d.pending_adds.retain(|id| !ids.contains(id));
        if was_running {
            // Steam writes its old list back when it closes; the next restart removes them again.
            d.pending_removals.extend(&ids);
        } else {
            d.clear_pending();
        }
    })?;
    drop(_guard);
    let msg = if was_running && !restart { "Removed. Restart Steam to see the change." } else { "Removed from Steam." };
    set_status(app, msg);
    emit_state(app);
    if was_running && restart {
        request_restart(app);
    }
    Ok(msg.into())
}

/// The shared removal steps, also used by the uninstaller (which has no window).
/// With `restart` (the "When SteamDrop restarts Steam" setting), Steam is closed first and reopened after.
fn remove_blocking(steam_dir: &Path, data_dir: &Path, ids: &[u32], restart: Option<&str>) -> Result<(), String> {
    if let Some(mode) = restart {
        return steam::restart_with(steam_dir, mode, || remove_blocking(steam_dir, data_dir, ids, None));
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
    result
}

const SELF_NAME: &str = "SteamDrop";
/// SteamDrop's own artwork (drawn by scripts/render-art.mjs).
const SELF_ART: [(art::Slot, &[u8]); 5] = [
    (art::Slot::Portrait, include_bytes!("../steam-art/portrait.png")),
    (art::Slot::Wide, include_bytes!("../steam-art/wide.png")),
    (art::Slot::Hero, include_bytes!("../steam-art/hero.png")),
    (art::Slot::Logo, include_bytes!("../steam-art/logo.png")),
    (art::Slot::Icon, include_bytes!("../steam-art/icon.png")),
];

/// SteamDrop's own shortcut id: Steam may report it as the running "game" while SteamDrop is open from it.
pub fn self_id() -> Option<u32> {
    self_shortcut().ok().map(|(id, _)| id)
}

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
    if apply_pending(&steam_dir, &state.dir, &snapshot, &steam::users(&steam_dir), &state.target_users(&steam_dir)).is_ok() {
        let _ = state.update(|d| d.clear_pending());
    }
    if state.settings().steam_shortcut {
        let missing = self_shortcut().is_ok_and(|(id, _)| state.target_users(&steam_dir).iter().any(|(_, c)| !steam::present_ids(c).contains(&id)));
        // A newer SteamDrop can bring new art for its own tile.
        let old_art = self_shortcut().is_ok_and(|(id, _)| std::fs::read(art::cache_dir(&state.dir, id).join("portrait.png")).is_ok_and(|b| b != SELF_ART[0].1));
        if missing || old_art {
            if let Err(e) = set_self_shortcut(&steam_dir, &state.dir, true) {
                log::problem(format!("Could not add SteamDrop to Steam: {e}"));
            }
        }
    }
}

/// Writes again what was changed while Steam was running: removals Steam may have undone and
/// games it may have dropped. Call only while Steam is closed.
/// Removals go to every account; adds go to `targets` (the chosen accounts).
pub fn apply_pending(steam_dir: &Path, data_dir: &Path, data: &Data, all: &[(String, PathBuf)], targets: &[(String, PathBuf)]) -> Result<(), String> {
    if data.pending_self {
        set_self_shortcut(steam_dir, data_dir, data.settings.steam_shortcut)?;
    }
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

/// Asks for a Steam restart that applies every pending change. Requests made while one is
/// already waiting share it; changes made during a restart get one follow-up restart.
pub fn request_restart(app: &AppHandle) {
    let state = app.state::<App>();
    if state.restart_queued.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<App>();
        let app3 = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || wait_for_game(&app3)).await;
        let _guard = state.busy.lock().await;
        state.restart_queued.store(false, std::sync::atomic::Ordering::SeqCst);
        let app2 = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || restart_steam_applying(&app2)).await.map_err(|e| e.to_string()).and_then(|r| r);
        if let Err(e) = result {
            log::problem(format!("Could not restart Steam: {e}"));
            notify(&app, "Could not restart Steam", &e);
            set_status(&app, &e);
        }
    });
}

/// Steam won't close while a game runs, so a restart waits for the game rather than force anything.
/// Call it before taking `busy`: adding and removing games must keep working meanwhile.
pub fn wait_for_game(app: &AppHandle) {
    let games = || app.state::<App>().data.lock().unwrap().games.clone();
    if !steam::is_running() || !steam::game_running(&games()) {
        return;
    }
    set_status(app, "A game is running. Steam restarts once you close it.");
    log::info("A game is running, so the Steam restart waits until it closes.");
    let t = std::time::Instant::now();
    while steam::game_running(&games()) {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    log::info(format!("The game closed after {} s; restarting Steam now.", t.elapsed().as_secs()));
}

/// Closes Steam, applies pending changes, and starts it again in the mode the setting asks for.
/// Hold `busy` while calling it. With Steam closed, it only applies the changes.
pub fn restart_steam_applying(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<App>();
    let steam_dir = state.steam_path().ok_or("Steam was not found. Set its folder in Settings.")?;
    let snapshot = state.data.lock().unwrap().clone();
    let write = || apply_pending(&steam_dir, &state.dir, &snapshot, &steam::users(&steam_dir), &state.target_users(&steam_dir));
    let running = steam::is_running();
    if running {
        set_status(app, "Restarting Steam...");
        steam::restart_with(&steam_dir, &snapshot.settings.steam_restart_mode, write)?;
    } else {
        write()?;
    }
    // Clear only what was written: anything queued meanwhile stays for the next restart.
    state.update(|d| {
        d.pending_adds.retain(|id| !snapshot.pending_adds.contains(id));
        d.pending_removals.retain(|id| !snapshot.pending_removals.contains(id));
        if d.settings.steam_shortcut == snapshot.settings.steam_shortcut {
            d.pending_self &= !snapshot.pending_self;
        }
    })?;
    set_status(app, if running { "Steam restarted." } else { "Changes written to Steam." });
    emit_state(app);
    Ok(())
}

fn games_by_id(app: &AppHandle, ids: &[u32]) -> Vec<Game> {
    app.state::<App>().data.lock().unwrap().games.iter().filter(|g| ids.contains(&g.id)).cloned().collect()
}

/// "Delete game" (one, or several selected): each folder goes to the Recycle Bin, then the shortcuts
/// and art of the ones that went are removed in one go. A folder that can't be recycled stays, with its shortcut.
pub async fn delete_games(app: &AppHandle, ids: Vec<u32>, restart: bool) -> Result<String, String> {
    let deleting = &app.state::<App>().deleting;
    deleting.lock().unwrap().extend(&ids);
    let result = delete_games_inner(app, &ids, restart).await;
    // Kept past the watcher's longest batch wait (BATCH_MAX), so the folders' events are handed over by then.
    let (app2, ids2) = (app.clone(), ids);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(120));
        app2.state::<App>().deleting.lock().unwrap().retain(|id| !ids2.contains(id));
    });
    result
}

/// Why a "game folder" must not be deleted: it's a whole drive, or it holds a watched folder, Steam,
/// SteamDrop or the user's own folders (someone added, say, the Desktop as a game). None if it's fine.
fn too_much_to_delete(folder: &str, keep: &[PathBuf]) -> Option<String> {
    let norm = |p: &str| p.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let f = norm(folder);
    if f.len() <= 2 {
        return Some("it is a whole drive".into());
    }
    keep.iter().map(|k| norm(&k.to_string_lossy())).find(|k| *k == f || k.starts_with(&format!("{f}\\"))).map(|k| format!("it holds {k}, which is more than this game"))
}

async fn delete_games_inner(app: &AppHandle, ids: &[u32], restart: bool) -> Result<String, String> {
    let state = app.state::<App>();
    let mut keep: Vec<PathBuf> = state.settings().watched.iter().map(PathBuf::from).collect();
    keep.extend([state.steam_path(), Some(state.dir.clone()), dirs::home_dir(), std::env::current_exe().ok()].into_iter().flatten());
    let (games, refused): (Vec<Game>, Vec<Game>) = games_by_id(app, ids).into_iter().partition(|g| too_much_to_delete(&g.folder, &keep).is_none());
    if let Some(g) = refused.first() {
        let why = too_much_to_delete(&g.folder, &keep).unwrap_or_default();
        log::problem(format!("Refused to delete the folder of \"{}\" ({}): {why}.", g.name, g.folder));
        return Err(format!("SteamDrop won't delete the folder of \"{}\" because {why}. Nothing was deleted. You can remove the game from Steam instead.", g.name));
    }
    if games.is_empty() {
        return Err("That game is no longer in the list.".into());
    }
    let mut gone = vec![];
    let mut failed = vec![];
    for game in games {
        let folder = PathBuf::from(&game.folder);
        match tauri::async_runtime::spawn_blocking(move || crate::recycle::recycle(&folder)).await.map_err(|e| e.to_string())? {
            Ok(()) => {
                log::info(format!("Moved the folder of \"{}\" to the Recycle Bin: {}", game.name, game.folder));
                gone.push(game);
            }
            Err(e) => {
                log::problem(format!("Could not delete \"{}\": {e}", game.name));
                failed.push((game.name, e));
            }
        }
    }
    if !gone.is_empty() {
        remove(app, gone.iter().map(|g| g.id).collect(), restart).await?;
    }
    let names = |list: Vec<String>| list.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
    match (gone.is_empty(), failed.is_empty()) {
        (_, true) => Ok(format!("{} moved to the Recycle Bin and removed from Steam.", names(gone.into_iter().map(|g| g.name).collect()))),
        (true, false) if failed.len() == 1 => Err(failed.remove(0).1),
        _ => Err(format!(
            "Deleted {}. Not deleted: {} ({}).",
            if gone.is_empty() { "nothing".into() } else { names(gone.into_iter().map(|g| g.name).collect()) },
            names(failed.iter().map(|f| f.0.clone()).collect()),
            failed[0].1
        )),
    }
}

/// "Re-fetch art" for one or several games: forget the cached art and add them again, in one go.
pub async fn refetch_art(app: &AppHandle, ids: Vec<u32>, restart: bool) -> Result<String, String> {
    let items: Vec<Add> = games_by_id(app, &ids)
        .into_iter()
        .map(|g| Add { folder: g.folder.into(), exe: g.exe.into(), name: g.name, replaces: None, fresh_art: true })
        .collect();
    if items.is_empty() {
        return Err("Those games are no longer in the list.".into());
    }
    add(app, items, restart).await
}

/// The file a re-check settles on: the current one while it exists (it may be the user's own pick),
/// otherwise the best match in the folder.
fn rechecked_exe(game: &Game, candidates: &[exe::Candidate]) -> Option<PathBuf> {
    let current = PathBuf::from(&game.exe);
    if current.is_file() {
        return Some(current);
    }
    candidates.first().map(|c| c.path.clone())
}

/// "Re-check game file": games whose file is gone (an update renamed it, say) get the best match in
/// their folder; the others keep theirs. All changes go to Steam in one go.
pub async fn recheck(app: &AppHandle, ids: Vec<u32>, restart: bool) -> Result<String, String> {
    let excluded = app.state::<App>().settings().excluded;
    let mut changes = vec![];
    let mut notes = vec![];
    for g in games_by_id(app, &ids) {
        let mut found = exe::candidates(Path::new(&g.folder));
        found.retain(|c| !excluded.iter().any(|x| c.path.starts_with(x)));
        match rechecked_exe(&g, &found) {
            Some(exe) if exe.to_string_lossy() == g.exe => notes.push(format!("\"{}\" is fine", g.name)),
            Some(exe) => {
                log::info(format!("Re-check: \"{}\" now starts with {}", g.name, exe.display()));
                notes.push(format!("\"{}\" now uses {}", g.name, exe.file_name().unwrap_or_default().to_string_lossy()));
                changes.push(Add { folder: g.folder.clone().into(), exe, name: g.name, replaces: Some(g.id), fresh_art: false });
            }
            None => notes.push(format!("\"{}\" has no game file", g.name)),
        }
    }
    if !changes.is_empty() {
        add(app, changes, restart).await?;
    }
    Ok(format!("Checked: {}.", notes.join(", ")))
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
        let restart = steam::is_running().then_some(data.settings.steam_restart_mode.as_str());
        if let Err(e) = remove_blocking(&steam_dir, &dir, &ids, restart) {
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
    fn a_recheck_keeps_a_working_file_and_replaces_a_missing_one() {
        let base = std::env::temp_dir().join(format!("steamdrop-recheck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("Game")).unwrap();
        std::fs::write(base.join("Game/launcher.exe"), b"x").unwrap();
        std::fs::write(base.join("Game/Game.exe"), b"x").unwrap();
        let found = inspect(&base.join("Game")).candidates;
        let mine = Game { id: 1, name: "Game".into(), exe: base.join("Game/launcher.exe").to_string_lossy().into(), folder: base.join("Game").to_string_lossy().into(), steam_appid: None, added: 1 };
        assert_eq!(rechecked_exe(&mine, &found), Some(base.join("Game/launcher.exe")), "the user's pick stays while it exists");
        let renamed = Game { exe: base.join("Game/Game-old.exe").to_string_lossy().into(), ..mine };
        assert_eq!(rechecked_exe(&renamed, &found), found.first().map(|c| c.path.clone()), "a missing file gets the best match");
        assert_eq!(rechecked_exe(&renamed, &[]), None);
    }

    #[test]
    fn only_a_games_own_folder_can_be_deleted() {
        let keep = vec![PathBuf::from(r"D:\Games"), PathBuf::from(r"C:\Users\Leon"), PathBuf::from(r"C:\Program Files (x86)\Steam")];
        assert!(too_much_to_delete(r"D:\Games\Hades", &keep).is_none());
        assert!(too_much_to_delete(r"E:\Stuff\Celeste\", &keep).is_none());
        assert!(too_much_to_delete(r"D:\", &keep).is_some(), "a whole drive");
        assert!(too_much_to_delete(r"d:\games", &keep).is_some(), "a watched folder itself");
        assert!(too_much_to_delete(r"C:\Users", &keep).is_some(), "it holds the user's folders");
        assert!(too_much_to_delete(r"C:/Program Files (x86)", &keep).is_some(), "it holds Steam");
        assert!(too_much_to_delete(r"D:\Gam", &keep).is_none(), "only whole folder names count");
    }

    #[test]
    fn own_tile_on_and_off_touches_nothing_else() {
        let base = std::env::temp_dir().join(format!("steamdrop-self-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let config = base.join("userdata/7/config");
        std::fs::create_dir_all(config.join("grid")).unwrap();
        let data_dir = base.join("data");
        let exe = base.join("games/B/b.exe");
        let game = steam::NewShortcut { id: 0x8000_0005, name: "B", exe: &exe, icon: String::new(), launch_options: "" };
        steam::add(&config, &data_dir, "7", &[game]).unwrap();
        std::fs::write(config.join("grid/2147483653p.png"), b"game art").unwrap();
        let (own, _) = self_shortcut().unwrap();
        set_self_shortcut(&base, &data_dir, true).unwrap();
        assert_eq!(steam::present_ids(&config), vec![0x8000_0005, own]);
        assert!(config.join(format!("grid/{own}p.png")).is_file(), "its own art is installed");
        set_self_shortcut(&base, &data_dir, false).unwrap();
        assert_eq!(steam::present_ids(&config), vec![0x8000_0005], "only SteamDrop's own tile goes");
        assert!(!config.join(format!("grid/{own}p.png")).exists());
        assert_eq!(std::fs::read(config.join("grid/2147483653p.png")).unwrap(), b"game art", "the game's art is untouched");
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
        apply_pending(&base, &data_dir, &data, &users, &users).unwrap();
        assert_eq!(steam::present_ids(&config), vec![9]);
        assert!(config.join("grid/9p.png").is_file());
        // Running it again changes nothing.
        apply_pending(&base, &data_dir, &data, &users, &users).unwrap();
        assert_eq!(steam::present_ids(&config), vec![9]);
    }
}
