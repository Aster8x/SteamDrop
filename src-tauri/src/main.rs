// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod changelog;
mod exe;
mod library;
mod log;
mod report;
mod secrets;
mod state;
mod steam;
mod store;
mod titlecard;
mod updates;
mod vdf;
mod watcher;

use serde::Serialize;
use state::{same_path, App, Found, Game, Settings};
use std::path::PathBuf;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_dialog::DialogExt;

/// Shows the main window, creating it if it was destroyed while in the tray.
pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let app2 = app.clone();
    // Windows must be built on the main thread.
    let _ = app.run_on_main_thread(move || {
        if let Some(cfg) = app2.config().app.windows.first() {
            if let Ok(w) = WebviewWindowBuilder::from_config(&app2, cfg).and_then(|b| b.build()) {
                let _ = w.set_focus();
            }
        }
    });
}

#[derive(Serialize)]
struct GameView {
    #[serde(flatten)]
    game: Game,
    portrait: Option<PathBuf>,
    in_steam: bool,
    folder_missing: bool,
}

#[derive(Serialize)]
struct SteamView {
    path: Option<PathBuf>,
    detected: Option<PathBuf>,
    users: usize,
    running: bool,
}

#[derive(Serialize)]
struct StateView {
    settings: Settings,
    games: Vec<GameView>,
    found: Vec<Found>,
    steam: SteamView,
    status: String,
    version: String,
    update: Option<updates::Available>,
    whats_new: Option<changelog::Section>,
}

#[tauri::command]
fn get_state(app: AppHandle, state: State<App>) -> StateView {
    let data = state.data.lock().unwrap().clone();
    let steam_path = state.steam_path();
    let users = steam_path.as_deref().map(steam::users).unwrap_or_default();
    let present: Vec<u32> = users.iter().flat_map(|(_, c)| steam::present_ids(c)).collect();
    let mut games: Vec<GameView> = data
        .games
        .into_iter()
        .map(|g| GameView {
            portrait: art::cached(&state.dir, g.id, art::Slot::Portrait),
            in_steam: present.contains(&g.id),
            folder_missing: !PathBuf::from(&g.exe).is_file(),
            game: g,
        })
        .collect();
    games.sort_by_key(|g| std::cmp::Reverse(g.game.added));
    StateView {
        settings: data.settings,
        games,
        found: state.found.lock().unwrap().clone(),
        steam: SteamView { path: steam_path, detected: steam::detect_path(), users: users.len(), running: steam::is_running() },
        status: state.status.lock().unwrap().clone(),
        update: state.update.lock().unwrap().clone(),
        whats_new: state.whats_new.lock().unwrap().clone(),
        version: app.package_info().version.to_string(),
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<App>, settings: Settings) -> Result<(), String> {
    let old = state.settings();
    set_run_on_startup(settings.run_on_startup)?;
    let watch_changed = old.watched != settings.watched;
    log_changes(&old, &settings);
    state.update(|d| d.settings = settings)?;
    if watch_changed {
        watcher::restart(&app);
    }
    library::emit_state(&app);
    Ok(())
}

/// Writes "Setting changed: auto_add: false -> true" lines, so bug reports show what was switched.
fn log_changes(old: &Settings, new: &Settings) {
    let (Ok(serde_json::Value::Object(a)), Ok(serde_json::Value::Object(b))) = (serde_json::to_value(old), serde_json::to_value(new)) else { return };
    for (k, v) in &b {
        if a.get(k) != Some(v) {
            log::info(format!("Setting changed: {k}: {} -> {v}", a.get(k).unwrap_or(&serde_json::Value::Null)));
        }
    }
}

fn set_run_on_startup(on: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let (run, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
            .map_err(|e| e.to_string())?;
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            run.set_value("SteamDrop", &format!("\"{}\" --tray", exe.display())).map_err(|e| e.to_string())?;
        } else {
            let _ = run.delete_value("SteamDrop");
        }
    }
    let _ = on;
    Ok(())
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    app.dialog().file().set_title("Choose a folder").blocking_pick_folder().map(|p| p.to_string())
}

#[tauri::command]
async fn pick_exe(app: AppHandle, folder: String) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose the file that starts the game")
        .set_directory(folder)
        .add_filter("Programs", &["exe"])
        .blocking_pick_file()
        .map(|p| p.to_string())
}

/// How many folders are already inside a folder about to be watched.
#[tauri::command]
fn count_subfolders(path: String) -> usize {
    std::fs::read_dir(path).map(|d| d.flatten().filter(|e| e.path().is_dir()).count()).unwrap_or(0)
}

#[tauri::command]
fn add_watch_folder(app: AppHandle, state: State<App>, path: String, include_existing: bool) -> Result<(), String> {
    state.update(|d| {
        if !include_existing {
            for e in std::fs::read_dir(&path).into_iter().flatten().flatten().filter(|e| e.path().is_dir()) {
                d.mark_known(&e.path().to_string_lossy());
            }
        }
        if !d.settings.watched.iter().any(|w| same_path(w, &path)) {
            d.settings.watched.push(path.clone());
        }
    })?;
    let existing = if include_existing { "games already there will be added" } else { "games already there were left alone" };
    log::info(format!("Started watching {path} ({existing})."));
    watcher::restart(&app);
    library::emit_state(&app);
    Ok(())
}

/// "Add a game" button: look at one folder and show the confirm card.
#[tauri::command]
fn inspect_folder(app: AppHandle, state: State<App>, path: String) -> Found {
    let f = library::inspect(&PathBuf::from(&path));
    let mut list = state.found.lock().unwrap();
    list.retain(|x| !same_path(&x.folder, &f.folder));
    list.push(f.clone());
    drop(list);
    library::emit_state(&app);
    f
}

#[tauri::command]
async fn add_game(app: AppHandle, folder: String, exe: String, name: String, restart: bool) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Please type a name for the game.".into());
    }
    let item = library::Add { folder: folder.into(), exe: exe.into(), name };
    library::add(&app, vec![item], restart).await
}

/// "Not now": forget the card and don't ask about this folder again.
#[tauri::command]
fn dismiss_found(app: AppHandle, state: State<App>, folder: String) -> Result<(), String> {
    state.found.lock().unwrap().retain(|f| !same_path(&f.folder, &folder));
    state.update(|d| d.mark_known(&folder))?;
    log::info(format!("You skipped {folder}. SteamDrop won't ask about it again."));
    library::emit_state(&app);
    Ok(())
}

#[tauri::command]
async fn remove_game(app: AppHandle, id: u32, restart: bool) -> Result<String, String> {
    library::remove(&app, vec![id], restart).await
}

/// Re-downloads a game's art (optionally for a corrected Steam App ID) and puts it back in Steam.
#[tauri::command]
async fn refresh_game(app: AppHandle, state: State<'_, App>, id: u32, steam_appid: Option<u32>, restart: bool) -> Result<String, String> {
    let game = state.update(|d| {
        let g = d.games.iter_mut().find(|g| g.id == id)?;
        if steam_appid.is_some() {
            g.steam_appid = steam_appid;
        }
        Some(g.clone())
    })?;
    let game = game.ok_or("That game is no longer in the list.")?;
    let _ = std::fs::remove_dir_all(art::cache_dir(&state.dir, id));
    let _ = std::fs::remove_dir_all(PathBuf::from(&game.folder).join(art::GAME_FOLDER_ART));
    let item = library::Add { folder: game.folder.into(), exe: game.exe.into(), name: game.name };
    library::add(&app, vec![item], restart).await
}

#[tauri::command]
async fn get_info(state: State<'_, App>, id: u32) -> Result<Option<store::Info>, String> {
    let game = state.data.lock().unwrap().games.iter().find(|g| g.id == id).cloned();
    let Some(game) = game else { return Ok(None) };
    let appid = match game.steam_appid {
        Some(a) => Some(a),
        None => {
            let found = store::search(&state.http, &game.name).await;
            if found.is_some() {
                state.update(|d| d.games.iter_mut().filter(|g| g.id == id).for_each(|g| g.steam_appid = found))?;
            }
            found
        }
    };
    match appid {
        Some(a) => store::details(&state.http, &state.dir, a).await.map(Some),
        None => Ok(None),
    }
}

#[tauri::command]
fn get_sgdb_key() -> String {
    secrets::sgdb_key()
}

#[tauri::command]
fn set_sgdb_key(key: String) -> Result<(), String> {
    let key = key.trim();
    secrets::set_sgdb_key(key)?;
    log::info(if key.is_empty() { "SteamGridDB key removed." } else { "SteamGridDB key saved in Windows Credential Manager." });
    Ok(())
}

#[tauri::command]
async fn test_sgdb_key(state: State<'_, App>, key: String) -> Result<String, String> {
    let result = art::sgdb_key_check(&state.http, &key).await;
    match &result {
        Ok(_) => log::info("Tested the SteamGridDB key: it works."),
        Err(e) => log::info(format!("Tested the SteamGridDB key: {e}")),
    }
    result
}

#[tauri::command]
fn get_activity() -> Vec<String> {
    log::recent(300)
}

#[tauri::command]
fn bug_report(app: AppHandle, state: State<App>) -> String {
    report::build(&state, &app.package_info().version.to_string())
}

#[tauri::command]
fn open_logs() -> Result<(), String> {
    let dir = log::dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open(dir.to_string_lossy().into_owned())
}

#[tauri::command]
async fn restart_steam(app: AppHandle, state: State<'_, App>) -> Result<(), String> {
    let _guard = state.busy.lock().await;
    let steam_dir = state.steam_path().ok_or("Steam was not found. Set its folder in Settings.")?;
    library::set_status(&app, "Restarting Steam...");
    tauri::async_runtime::spawn_blocking(move || {
        steam::shutdown(&steam_dir)?;
        steam::start(&steam_dir, true)
    })
    .await
    .map_err(|e| e.to_string())??;
    log::info("Restarted Steam.");
    library::set_status(&app, "Steam restarted.");
    Ok(())
}

/// Opens web pages (https only) and folders with the system's default handler.
#[tauri::command]
fn open(target: String) -> Result<(), String> {
    let is_url = target.starts_with("https://");
    if !is_url && !PathBuf::from(&target).is_dir() {
        return Err("That folder no longer exists.".into());
    }
    #[cfg(windows)]
    {
        let program = if is_url { "rundll32" } else { "explorer" };
        let mut cmd = std::process::Command::new(program);
        if is_url {
            cmd.args(["url.dll,FileProtocolHandler", &target]);
        } else {
            cmd.arg(&target);
        }
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// "Check now": shows the update window if there's a newer version (even one that was skipped).
#[tauri::command]
async fn check_now(app: AppHandle, state: State<'_, App>) -> Result<Option<updates::Available>, String> {
    let found = updates::check(&app).await?.map(|(a, _)| a);
    *state.update.lock().unwrap() = found.clone();
    library::emit_state(&app);
    Ok(found)
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    updates::install_now(&app).await
}

/// "Remind me later" (skip = false) or "Skip this version" (skip = true).
#[tauri::command]
fn dismiss_update(app: AppHandle, state: State<App>, skip: bool) -> Result<(), String> {
    let Some(update) = state.update.lock().unwrap().take() else { return Ok(()) };
    if skip {
        state.update(|d| d.settings.skipped_version = update.version.clone())?;
        log::info(format!("You chose to skip update {}.", update.version));
    } else {
        log::info(format!("You chose to be reminded about update {} later.", update.version));
    }
    library::emit_state(&app);
    Ok(())
}

/// The "What's new" screen was closed: don't show it again for this version.
#[tauri::command]
fn ack_whats_new(app: AppHandle, state: State<App>) -> Result<(), String> {
    state.whats_new.lock().unwrap().take();
    let current = app.package_info().version.to_string();
    state.update(|d| d.settings.last_seen_version = current)?;
    library::emit_state(&app);
    Ok(())
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn main() {
    if std::env::args().any(|a| a == "--uninstall-cleanup") {
        library::uninstall_cleanup();
        return;
    }
    // Used by the uninstaller's "Delete SteamDrop settings and cache".
    if std::env::args().any(|a| a == "--forget-secrets") {
        let _ = secrets::set_sgdb_key("");
        return;
    }
    let start_hidden = std::env::args().any(|a| a == "--tray");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_window(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(App::new())
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            pick_folder,
            pick_exe,
            count_subfolders,
            add_watch_folder,
            inspect_folder,
            add_game,
            dismiss_found,
            remove_game,
            refresh_game,
            get_info,
            get_sgdb_key,
            set_sgdb_key,
            test_sgdb_key,
            get_activity,
            bug_report,
            open_logs,
            restart_steam,
            open,
            check_now,
            install_update,
            dismiss_update,
            ack_whats_new,
            quit
        ])
        .setup(move |app| {
            log::info(format!(
                "SteamDrop {} started{}.",
                app.package_info().version,
                if start_hidden { " with Windows (in the tray)" } else { "" }
            ));
            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "open", "Open SteamDrop", true, None::<&str>)?,
                    &MenuItem::with_id(app, "quit", "Quit SteamDrop", true, None::<&str>)?,
                ],
            )?;
            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().cloned().expect("app icon"))
                .tooltip(format!("SteamDrop {}", app.package_info().version))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => show_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        show_window(tray.app_handle());
                    }
                })
                .build(app)?;
            watcher::restart(app.handle());
            updates::after_start(app.handle());
            updates::schedule(app.handle());
            if !start_hidden {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // With "minimize to tray" on, closing destroys the window (freeing its memory) and we keep running.
            if let WindowEvent::CloseRequested { .. } = event {
                if !window.state::<App>().settings().close_to_tray {
                    window.app_handle().exit(0);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while starting SteamDrop");

    app.run(|_, event| {
        // The last window closing asks to exit with no code; stay in the tray instead.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
