// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod changelog;
mod couch;
mod exe;
mod library;
mod log;
mod recycle;
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
                couch::sync_lock(&app2, None);
            }
        }
    });
}

#[derive(Serialize)]
struct GameView {
    #[serde(flatten)]
    game: Game,
    portrait: Option<PathBuf>,
    hero: Option<PathBuf>,
    in_steam: bool,
    folder_missing: bool,
}

#[derive(Serialize)]
struct SteamView {
    path: Option<PathBuf>,
    detected: Option<PathBuf>,
    users: usize,
    running: bool,
    accounts: Vec<steam::Account>,
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
    couch_pin_set: bool,
    /// Changes were written while Steam was running and show after a Steam restart.
    restart_needed: bool,
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
            hero: art::cached(&state.dir, g.id, art::Slot::Hero),
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
        steam: SteamView {
            accounts: steam_path.as_deref().map(steam::accounts).unwrap_or_default(),
            path: steam_path,
            detected: steam::detect_path(),
            users: users.len(),
            running: steam::is_running(),
        },
        status: state.status.lock().unwrap().clone(),
        update: state.update.lock().unwrap().clone(),
        whats_new: state.whats_new.lock().unwrap().clone(),
        couch_pin_set: !secrets::couch_pin().is_empty(),
        restart_needed: !data.pending_adds.is_empty() || !data.pending_removals.is_empty(),
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
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || library::restart_steam_applying(&app2, true)).await.map_err(|e| e.to_string())?
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

/// Candidate game files for Couch mode's "Change game file".
#[tauri::command]
fn exe_candidates(folder: String) -> Vec<exe::Candidate> {
    exe::candidates(&PathBuf::from(folder))
}

/// Couch mode's "Delete game" (after holding A). Checks the PIN when one is set.
#[tauri::command]
async fn delete_game(app: AppHandle, id: u32, pin: String) -> Result<String, String> {
    let saved = secrets::couch_pin();
    if !saved.is_empty() && pin != saved {
        log::info("Someone tried to delete a game with a wrong PIN.");
        return Err("That PIN isn't right.".into());
    }
    library::delete_game(&app, id).await
}

/// Sets, changes or removes (empty `new`) the Couch mode PIN. Changing it needs the old one.
#[tauri::command]
fn set_couch_pin(app: AppHandle, old: String, new: String) -> Result<(), String> {
    let saved = secrets::couch_pin();
    if !saved.is_empty() && old != saved {
        return Err("The current PIN isn't right.".into());
    }
    if !new.is_empty() && (new.len() != 4 || !new.chars().all(|c| c.is_ascii_digit())) {
        return Err("The PIN must be 4 digits.".into());
    }
    secrets::set_couch_pin(&new)?;
    log::info(if new.is_empty() { "Couch mode PIN turned off." } else { "Couch mode PIN set." });
    library::emit_state(&app);
    Ok(())
}

/// "Add SteamDrop to Steam" setting.
#[tauri::command]
async fn set_steam_shortcut(app: AppHandle, state: State<'_, App>, on: bool) -> Result<String, String> {
    state.update(|d| d.settings.steam_shortcut = on)?;
    let steam_dir = state.steam_path().ok_or("Steam was not found. Set its folder in Settings.")?;
    let dir = state.dir.clone();
    tauri::async_runtime::spawn_blocking(move || library::set_self_shortcut(&steam_dir, &dir, on))
        .await
        .map_err(|e| e.to_string())??;
    library::emit_state(&app);
    let shown = if on { "SteamDrop is in your Steam library" } else { "SteamDrop was removed from Steam" };
    Ok(if steam::is_running() { format!("{shown} after Steam restarts.") } else { format!("{shown}.") })
}

#[tauri::command]
fn open_couch(app: AppHandle) {
    couch::open(&app, couch::View::Couch);
}

#[tauri::command]
fn close_couch(app: AppHandle, state: State<App>, restart: bool) {
    // The restart happens after the window (and the process Steam started) is gone; see couch::closed.
    state.restart_after_couch.store(restart, std::sync::atomic::Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(couch::COUCH) {
        let _ = w.destroy();
    }
}

/// The view switch button and F11.
#[tauri::command]
fn switch_view(app: AppHandle, from: String) {
    couch::switch(&app, &from);
}

/// "Hide": the desktop window goes to the tray; Couch mode minimizes, back to Steam.
/// Either way SteamDrop keeps its Steam session, so Steam can bring it back.
#[tauri::command]
fn hide_window(app: AppHandle, label: String) {
    if let Some(w) = app.get_webview_window(&label) {
        let _ = if label == couch::COUCH { w.minimize() } else { w.hide() };
    }
}

/// "Close": the window goes away; SteamDrop keeps watching from the tray.
#[tauri::command]
fn close_window(app: AppHandle, label: String) {
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.destroy();
    }
}

/// "Scan now": look at the watched folders right away.
#[tauri::command]
fn scan_now(app: AppHandle) -> String {
    let found = watcher::scan_now(&app);
    log::info(format!("Scan now: {found} new folder(s) found."));
    match found {
        0 => "No new game folders found.".into(),
        1 => "Found 1 new folder.".into(),
        n => format!("Found {n} new folders."),
    }
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
        let _ = secrets::set_couch_pin("");
        return;
    }
    // Used by the installer's "Add SteamDrop to Steam" and by the uninstaller.
    if std::env::args().any(|a| a == "--add-to-steam") {
        library::add_to_steam_from_installer();
        return;
    }
    if std::env::args().any(|a| a == "--remove-from-steam") {
        library::remove_self_from_installer();
        return;
    }
    let args: Vec<String> = std::env::args().collect();
    // "--couch" is what the Steam shortcut used before views could be chosen.
    let from_steam = args.iter().any(|a| a == "--from-steam" || a == "--couch");
    #[cfg(windows)]
    if from_steam {
        couch::run_from_steam();
        return;
    }
    let big_picture = args.iter().any(|a| a == "--big-picture") || (from_steam && couch::started_in_big_picture());
    // Elsewhere (development builds) the Steam launch opens its view in this same process.
    let open_view = couch::parse_view(&args).or_else(|| {
        from_steam.then(|| couch::view_for(&state::load(&state::data_dir()).settings.steam_view, big_picture))
    });
    let start_hidden = args.iter().any(|a| a == "--tray");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if args.iter().any(|a| a == "--big-picture") {
                app.state::<App>().big_picture.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            match couch::parse_view(&args) {
                Some(view) => couch::open(app, view),
                None => show_window(app),
            }
        }))
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
            exe_candidates,
            delete_game,
            set_couch_pin,
            set_steam_shortcut,
            open_couch,
            close_couch,
            switch_view,
            hide_window,
            close_window,
            scan_now,
            quit
        ])
        .setup(move |app| {
            app.state::<App>().big_picture.store(big_picture, std::sync::atomic::Ordering::SeqCst);
            log::info(format!(
                "SteamDrop {} started{}.",
                app.package_info().version,
                if start_hidden { " with Windows (in the tray)" } else { "" }
            ));
            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "open", "Open SteamDrop", true, None::<&str>)?,
                    &MenuItem::with_id(app, "couch", "Couch mode", true, None::<&str>)?,
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
                    "couch" => couch::open(app, couch::View::Couch),
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
            let handle = app.handle().clone();
            std::thread::spawn(move || library::startup_repair(&handle));
            if let Some(view) = open_view {
                couch::open(app.handle(), view);
            } else if !start_hidden {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // With "minimize to tray" on, closing destroys the window (freeing its memory) and we keep running.
            let app = window.app_handle();
            let label = window.label();
            match event {
                WindowEvent::CloseRequested { .. } => {
                    let other = if label == couch::COUCH { couch::MAIN } else { couch::COUCH };
                    if !window.state::<App>().settings().close_to_tray && app.get_webview_window(other).is_none() {
                        app.exit(0);
                    }
                }
                WindowEvent::Destroyed => couch::closed(app, label),
                _ => {}
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
