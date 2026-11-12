//! Update checks (on startup, daily, or "Check now"), the update window's data,
//! quiet automatic installs, and the one-time "What's new" screen.

use crate::changelog::{self, Section};
use crate::log;
use crate::state::App;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Debug, Serialize)]
pub struct Available {
    pub current: String,
    pub version: String,
    /// Every version between the installed one and the new one, newest first.
    pub notes: Vec<Section>,
}

const CHANGELOG_AT_TAG: &str = "https://raw.githubusercontent.com/Aster8x/SteamDrop";
/// While an automatic update waits for a game to close, look again this often.
const GAME_CHECK: Duration = Duration::from_secs(5 * 60);

/// Update notifications are always shown: the user asked for them separately from game notifications.
fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Asks GitHub for a newer version. Failures are logged in plain words and returned.
pub async fn check(app: &AppHandle) -> Result<Option<(Available, tauri_plugin_updater::Update)>, String> {
    let checked = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let update = match checked {
        Ok(Some(u)) => u,
        // No release published yet (no latest.json, the page answers 404): there's nothing newer.
        Ok(None) | Err(tauri_plugin_updater::Error::ReleaseNotFound) => {
            log::info("Checked for updates: you're on the latest version.");
            return Ok(None);
        }
        Err(e) => {
            log::info(format!("Couldn't check for updates, maybe there is no internet connection right now. SteamDrop will try again later. (Details: {e})"));
            return Err("Couldn't check for updates right now. Check your internet connection and try again later.".into());
        }
    };
    log::info(format!("Update available: version {} (you have {}).", update.version, update.current_version));
    let http = &app.state::<App>().http;
    let url = format!("{CHANGELOG_AT_TAG}/v{}/CHANGELOG.md", update.version);
    let text = match http.get(url).send().await.and_then(|r| r.error_for_status()) {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(_) => String::new(),
    };
    let mut notes = changelog::between(&text, &update.current_version, &update.version);
    if notes.is_empty() {
        // The full changelog couldn't be fetched: fall back to the release's own notes.
        notes.push(Section { version: update.version.clone(), date: String::new(), body: update.body.clone().unwrap_or_default() });
    }
    let available = Available { current: update.current_version.clone(), version: update.version.clone(), notes };
    Ok(Some((available, update)))
}

/// The scheduled check: quietly install, or show the update window, as the settings say.
pub async fn auto_check(app: &AppHandle) {
    let Ok(Some((available, update))) = check(app).await else { return };
    let state = app.state::<App>();
    let settings = state.settings();
    if settings.skipped_version == available.version {
        log::info(format!("Not showing update {}: you chose to skip this version.", available.version));
        return;
    }
    if settings.update_auto_install {
        install_quietly(app, update).await;
        return;
    }
    notify(app, &format!("SteamDrop {} is available", available.version), "Open SteamDrop to see what's new and update.");
    *state.update.lock().unwrap() = Some(available);
    // Never a desktop window over Big Picture: the update window shows the next time SteamDrop is opened.
    if !crate::steam::big_picture_open() {
        crate::show_window(app);
    }
    crate::library::emit_state(app);
}

/// Downloads now, installs once no game started from Steam is running. Installing restarts SteamDrop.
async fn install_quietly(app: &AppHandle, update: tauri_plugin_updater::Update) {
    let bytes = match update.download(|_, _| {}, || {}).await {
        Ok(b) => b,
        Err(e) => {
            log::info(format!("Couldn't download update {}. SteamDrop will try again later. (Details: {e})", update.version));
            return;
        }
    };
    let mut told = false;
    let state = app.state::<App>();
    // Not while a game runs, and not while SteamDrop is on screen (it would vanish mid-use).
    while state.couch_lock.lock().unwrap().is_some() || crate::steam::game_running(&state.data.lock().unwrap().games.clone()) {
        if !told {
            log::info(format!("Update {} is ready. It will install once no game is running and SteamDrop's window is closed.", update.version));
            told = true;
        }
        std::thread::sleep(GAME_CHECK);
    }
    log::info(format!("Installing update {} automatically. SteamDrop will restart by itself.", update.version));
    let _ = std::fs::write(quiet_marker(), b"");
    if let Err(e) = update.install(bytes) {
        let _ = std::fs::remove_file(quiet_marker());
        log::problem(format!("Couldn't install update {}: {e}", update.version));
    }
}

/// Exists while an automatic update installs, so SteamDrop starts in the tray afterwards.
pub fn quiet_marker() -> std::path::PathBuf {
    crate::state::data_dir().join("updating-quietly")
}

/// "Update now" in the update window.
pub async fn install_now(app: &AppHandle) -> Result<(), String> {
    let (_, update) = check(app).await?.ok_or("There is no update to install right now.")?;
    log::info(format!("Installing update {}. SteamDrop will restart by itself.", update.version));
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| {
        log::problem(format!("Couldn't install update {}: {e}", update.version));
        "The update couldn't be installed. Please try again later.".to_string()
    })?;
    app.restart();
}

/// On startup: after an update, remember to show "What's new" once and say so in a notification.
pub fn after_start(app: &AppHandle) {
    let state = app.state::<App>();
    let current = app.package_info().version.to_string();
    let last = state.settings().last_seen_version;
    let newer = match (semver::Version::parse(&current), semver::Version::parse(&last)) {
        (Ok(c), Ok(l)) => c > l,
        _ => false,
    };
    if newer {
        log::info(format!("SteamDrop was updated from {last} to {current}."));
        *state.whats_new.lock().unwrap() = changelog::find(changelog::BUNDLED, &current);
        notify(app, &format!("SteamDrop was updated to {current}"), "Open SteamDrop to see what's new.");
    } else if last != current {
        // First run (or a downgrade): nothing to announce.
        let _ = state.update(|d| d.settings.last_seen_version = current);
    }
}

/// Starts the on-startup check (after a short pause) and the once-a-day check.
pub fn schedule(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        if app.state::<App>().settings().update_on_startup {
            tauri::async_runtime::block_on(auto_check(&app));
        }
        loop {
            std::thread::sleep(Duration::from_secs(24 * 60 * 60));
            if app.state::<App>().settings().update_daily {
                tauri::async_runtime::block_on(auto_check(&app));
            }
        }
    });
}
