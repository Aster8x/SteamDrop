//! Everything that touches Steam: where it lives, its users, shortcuts.vdf and its grid folder.

use crate::vdf::{self, Map, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Steam's install folder, from the registry, or the usual default.
pub fn detect_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let from_reg: Option<String> = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Valve\Steam")
            .and_then(|k| k.get_value("SteamPath"))
            .ok();
        if let Some(p) = from_reg.map(|p| PathBuf::from(p.replace('/', "\\"))) {
            if p.join("steam.exe").exists() {
                return Some(p);
            }
        }
        let default = PathBuf::from(r"C:\Program Files (x86)\Steam");
        default.join("steam.exe").exists().then_some(default)
    }
    #[cfg(not(windows))]
    None
}

/// Steam accounts that have signed in on this PC: (account id, its config folder).
pub fn users(steam: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(steam.join("userdata")) else { return vec![] };
    let mut list: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            let config = e.path().join("config");
            (id != "0" && id.chars().all(|c| c.is_ascii_digit()) && config.is_dir()).then_some((id, config))
        })
        .collect();
    list.sort();
    list
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Account {
    /// The folder name under userdata (Steam's 32-bit account id).
    pub id: String,
    pub persona: String,
    pub login: String,
}

/// Steam accounts on this PC with their display names from config/loginusers.vdf.
pub fn accounts(steam: &Path) -> Vec<Account> {
    let text = std::fs::read_to_string(steam.join("config").join("loginusers.vdf")).unwrap_or_default();
    let root = vdf::parse_text(&text);
    let logins: Vec<(u64, String, String)> = match vdf::text_get(&root, "users") {
        Some(vdf::Text::Map(users)) => users
            .iter()
            .filter_map(|(sid, v)| {
                let vdf::Text::Map(u) = v else { return None };
                let s = |k: &str| match vdf::text_get(u, k) {
                    Some(vdf::Text::Str(x)) => x.clone(),
                    _ => String::new(),
                };
                Some((sid.parse().ok()?, s("PersonaName"), s("AccountName")))
            })
            .collect(),
        _ => vec![],
    };
    users(steam)
        .into_iter()
        .map(|(id, _)| {
            // SteamID64 = 76561197960265728 + the 32-bit account id used for the userdata folder.
            let found = id.parse::<u64>().ok().and_then(|n| logins.iter().find(|(sid, _, _)| *sid == n + 76_561_197_960_265_728));
            let (persona, login) = found.map(|(_, p, l)| (p.clone(), l.clone())).unwrap_or_default();
            Account { persona: if persona.is_empty() { format!("Steam account {id}") } else { persona }, login, id }
        })
        .collect()
}

pub fn grid_dir(config: &Path) -> PathBuf {
    config.join("grid")
}

/// Steam's id for a non-Steam shortcut, the same formula Steam itself uses.
pub fn shortcut_id(exe_quoted: &str, name: &str) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(exe_quoted.as_bytes());
    h.update(name.as_bytes());
    h.finalize() | 0x8000_0000
}

pub fn quote(p: &Path) -> String {
    format!("\"{}\"", p.display())
}

fn shortcuts_file(config: &Path) -> PathBuf {
    config.join("shortcuts.vdf")
}

fn load(config: &Path) -> Result<Map, String> {
    match std::fs::read(shortcuts_file(config)) {
        Ok(bytes) => vdf::parse(&bytes).map_err(|e| format!("Steam's shortcuts file could not be read ({e})")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![("shortcuts".into(), Value::Map(vec![]))]),
        Err(e) => Err(e.to_string()),
    }
}

fn list_mut(root: &mut Map) -> &mut Map {
    if !root.iter().any(|(k, _)| k.eq_ignore_ascii_case("shortcuts")) {
        root.push(("shortcuts".into(), Value::Map(vec![])));
    }
    match root.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case("shortcuts")) {
        Some((_, Value::Map(m))) => m,
        // A "shortcuts" key that isn't a section: replace it rather than corrupt it further.
        Some((_, v)) => {
            *v = Value::Map(vec![]);
            let Value::Map(m) = v else { unreachable!() };
            m
        }
        None => unreachable!(),
    }
}

fn entry_id(entry: &Value) -> Option<u32> {
    match entry {
        Value::Map(m) => vdf::get_int(m, "appid"),
        _ => None,
    }
}

/// A shortcut's game file as Steam stores it ("C:\\Games\\x.exe" or C:/Games/x.exe), in one comparable form.
pub fn norm_exe(exe: &str) -> String {
    exe.trim().trim_matches('"').replace('/', "\\").to_lowercase()
}

/// Every shortcut's id and game file for this Steam user, whoever added it.
pub fn shortcut_exes(config: &Path) -> Vec<(u32, String)> {
    let Ok(mut root) = load(config) else { return vec![] };
    list_mut(&mut root)
        .iter()
        .filter_map(|(_, e)| match e {
            Value::Map(m) => match vdf::get(m, "exe") {
                Some(Value::Str(exe)) => Some((vdf::get_int(m, "appid").unwrap_or(0), norm_exe(exe))),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The shortcut ids present for this Steam user.
pub fn present_ids(config: &Path) -> Vec<u32> {
    let Ok(mut root) = load(config) else { return vec![] };
    list_mut(&mut root).iter().filter_map(|(_, e)| entry_id(e)).collect()
}

fn save(config: &Path, data_dir: &Path, user: &str, mut root: Map) -> Result<(), String> {
    let path = shortcuts_file(config);
    backup(&path, data_dir, user)?;
    // Steam expects entries numbered 0, 1, 2...
    for (i, (key, _)) in list_mut(&mut root).iter_mut().enumerate() {
        *key = i.to_string();
    }
    let tmp = path.with_extension("vdf.steamdrop-tmp");
    std::fs::write(&tmp, vdf::write(&root)).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

const KEEP_BACKUPS: usize = 30;

/// Copies shortcuts.vdf to %APPDATA%\SteamDrop\backups\<user>\ before every change.
fn backup(path: &Path, data_dir: &Path, user: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let dir = data_dir.join("backups").join(user);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut name = format!("shortcuts-{}.vdf", now());
    let mut n = 1;
    while dir.join(&name).exists() {
        name = format!("shortcuts-{}-{n}.vdf", now());
        n += 1;
    }
    std::fs::copy(path, dir.join(name)).map_err(|e| format!("Could not back up Steam's shortcuts file: {e}"))?;
    let mut old: Vec<PathBuf> = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).collect();
    old.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    for p in old.iter().take(old.len().saturating_sub(KEEP_BACKUPS)) {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

pub struct NewShortcut<'a> {
    pub id: u32,
    pub name: &'a str,
    pub exe: &'a Path,
    pub icon: String,
    pub launch_options: &'a str,
}

/// Adds (or replaces) shortcuts for one Steam user, in one write.
pub fn add(config: &Path, data_dir: &Path, user: &str, items: &[NewShortcut]) -> Result<(), String> {
    let mut root = load(config)?;
    let list = list_mut(&mut root);
    for s in items {
        list.retain(|(_, e)| entry_id(e) != Some(s.id));
        let start_dir = s.exe.parent().map(|p| quote(p)).unwrap_or_default();
        let str_ = |v: &str| Value::Str(v.to_string());
        list.push((
            String::new(),
            Value::Map(vec![
                ("appid".into(), Value::Int(s.id)),
                ("AppName".into(), str_(s.name)),
                ("Exe".into(), str_(&quote(s.exe))),
                ("StartDir".into(), str_(&start_dir)),
                ("icon".into(), str_(&s.icon)),
                ("ShortcutPath".into(), str_("")),
                ("LaunchOptions".into(), str_(s.launch_options)),
                ("IsHidden".into(), Value::Int(0)),
                ("AllowDesktopConfig".into(), Value::Int(1)),
                ("AllowOverlay".into(), Value::Int(1)),
                ("OpenVR".into(), Value::Int(0)),
                ("Devkit".into(), Value::Int(0)),
                ("DevkitGameID".into(), str_("")),
                ("DevkitOverrideAppID".into(), Value::Int(0)),
                // Puts it first under "Recent" in the library.
                ("LastPlayTime".into(), Value::Int(now() as u32)),
                ("FlatpakAppID".into(), str_("")),
                ("tags".into(), Value::Map(vec![])),
            ]),
        ));
    }
    save(config, data_dir, user, root)
}

/// Removes shortcuts by id. Never touches game files.
pub fn remove(config: &Path, data_dir: &Path, user: &str, ids: &[u32]) -> Result<(), String> {
    let mut root = load(config)?;
    let list = list_mut(&mut root);
    let before = list.len();
    list.retain(|(_, e)| !entry_id(e).is_some_and(|id| ids.contains(&id)));
    if list.len() == before {
        return Ok(());
    }
    save(config, data_dir, user, root)
}

/// A running process: its id, its parent's id and its lower-case .exe name.
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
}

/// Running processes, from Windows' process snapshot. Much faster than starting `tasklist`,
/// so Steam can be checked often.
#[cfg(windows)]
pub fn processes() -> Vec<Proc> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    let mut list = Vec::new();
    // SAFETY: the snapshot handle is checked and closed; the entry has its size set as the API requires.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return list;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            list.push(Proc { pid: e.th32ProcessID, parent: e.th32ParentProcessID, name: String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase() });
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    list
}

#[cfg(not(windows))]
pub fn processes() -> Vec<Proc> {
    Vec::new()
}

pub fn is_running() -> bool {
    processes().iter().any(|p| p.name == "steam.exe")
}

/// True while a game started from Steam is running: a Steam game (Steam's RunningAppID)
/// or one of SteamDrop's own games (its .exe is running).
pub fn game_running(games: &[crate::state::Game]) -> bool {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let steam_game: u32 = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Valve\Steam")
            .and_then(|k| k.get_value("RunningAppID"))
            .unwrap_or(0);
        // SteamDrop itself, opened from Steam, is never a game.
        if steam_game != 0 && Some(steam_game) != crate::library::self_id() {
            return true;
        }
    }
    let running = processes();
    games.iter().any(|g| {
        let name = Path::new(&g.exe).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        running.iter().any(|p| p.name == name)
    })
}

/// Apps "Keep Big Picture in front" never fights: Windows' sign-in, lock and permission screens, its
/// Start menu, search, Task Manager, on-screen keyboards and Game Bar, Steam itself, and SteamDrop.
const ALWAYS_ALLOWED: [&str; 17] = [
    "lockapp.exe", "logonui.exe", "consent.exe", "credentialuibroker.exe", "taskmgr.exe", "startmenuexperiencehost.exe",
    "searchhost.exe", "searchapp.exe", "shellexperiencehost.exe", "textinputhost.exe", "tabtip.exe", "osk.exe", "dwm.exe",
    "gamebar.exe", "steam.exe", "steamwebhelper.exe", "gameoverlayui.exe",
];

/// Why the window of process `pid` may keep the focus over Big Picture, or None if Big Picture
/// should come back: SteamDrop itself, the apps above, the user's exceptions, and anything started
/// from Steam (a game, or a game's own launcher).
pub fn may_keep_focus(pid: u32, procs: &[Proc], except: &[String], me: u32) -> Option<String> {
    let find = |id: u32| procs.iter().find(|p| p.pid == id);
    let name = find(pid).map(|p| p.name.clone()).unwrap_or_default();
    if pid == me || name == "steamdrop.exe" {
        return Some("it's SteamDrop".into());
    }
    if name.is_empty() || ALWAYS_ALLOWED.contains(&name.as_str()) {
        return Some(format!("{name} is part of Windows or Steam"));
    }
    if except.iter().any(|x| x.eq_ignore_ascii_case(&name)) {
        return Some(format!("{name} is in your exceptions"));
    }
    from_steam(pid, procs).then(|| format!("{name} was started from Steam"))
}

/// True if Steam started this process, directly or through a launcher (and it isn't Steam's own).
pub fn from_steam(pid: u32, procs: &[Proc]) -> bool {
    let find = |id: u32| procs.iter().find(|p| p.pid == id);
    let mut at = find(pid).filter(|p| !ALWAYS_ALLOWED.contains(&p.name.as_str()) && p.name != "steamdrop.exe");
    for _ in 0..10 {
        let Some(p) = at.and_then(|p| find(p.parent)).filter(|p| p.pid != 0) else { break };
        if p.name == "steam.exe" {
            return true;
        }
        at = Some(p);
    }
    false
}

/// How often Steam is checked while closing, and how long it gets before it is force-closed.
const POLL: Duration = Duration::from_millis(250);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(60);

/// Asks Steam to close and waits until it has, checking every 250 ms.
/// Steam must be closed while shortcuts.vdf changes, or it overwrites them on exit.
/// Once steam.exe has exited its files are closed, so no extra wait is needed.
/// Only if Steam is still running after a minute is it force-closed (and that is logged).
pub fn shutdown(steam: &Path) -> Result<(), String> {
    if !is_running() {
        return Ok(());
    }
    // Steam waits for SteamDrop's own Steam session like for a game; end it first so Steam closes right away.
    end_helpers();
    std::process::Command::new(steam.join("steam.exe"))
        .arg("-shutdown")
        .spawn()
        .map_err(|e| format!("Could not ask Steam to close: {e}"))?;
    let t = Instant::now();
    while t.elapsed() < CLOSE_TIMEOUT {
        std::thread::sleep(POLL);
        if !is_running() {
            return Ok(());
        }
    }
    crate::log::problem(format!("Steam did not close within {} seconds, so SteamDrop force-closed it.", CLOSE_TIMEOUT.as_secs()));
    force_close();
    for _ in 0..20 {
        if !is_running() {
            return Ok(());
        }
        std::thread::sleep(POLL);
    }
    Err("Steam did not close. Close Steam yourself, then try again.".into())
}

/// Ends the processes Steam started for SteamDrop (`--from-steam`, children of steam.exe) and anything
/// they started, except this process. Steam then stops showing SteamDrop as running.
pub fn end_helpers() {
    let doomed = helper_pids(&processes(), std::process::id());
    if !doomed.is_empty() {
        crate::log::info("Ended SteamDrop's Steam session, so Steam no longer shows it as running.");
    }
    doomed.into_iter().for_each(terminate);
}

fn helper_pids(procs: &[Proc], me: u32) -> Vec<u32> {
    let steam: Vec<u32> = procs.iter().filter(|p| p.name == "steam.exe").map(|p| p.pid).collect();
    let helpers: Vec<u32> = procs.iter().filter(|p| p.name == "steamdrop.exe" && steam.contains(&p.parent)).map(|p| p.pid).collect();
    procs.iter().filter(|p| p.pid != me && p.name == "steamdrop.exe" && (helpers.contains(&p.pid) || helpers.contains(&p.parent))).map(|p| p.pid).collect()
}

fn terminate(pid: u32) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
        // SAFETY: the handle is checked before use and closed after.
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !h.is_null() {
                TerminateProcess(h, 1);
                CloseHandle(h);
            }
        }
    }
    let _ = pid;
}

fn force_close() {
    processes().into_iter().filter(|p| p.name == "steam.exe").for_each(|p| terminate(p.pid));
}

/// The one way SteamDrop restarts Steam: decide the mode, close Steam, run `write`
/// (the shortcuts.vdf changes), then start Steam again right away, even if `write`
/// failed, so Steam is never left closed. Logs how long each phase took.
pub fn restart_with(steam: &Path, mode: &str, write: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    let big_picture = reopen_in_big_picture(mode);
    let t = Instant::now();
    shutdown(steam)?;
    let closed = t.elapsed();
    let written = write();
    let wrote = t.elapsed() - closed;
    let started = start(steam, big_picture);
    let total = t.elapsed();
    crate::log::info(format!(
        "Steam restart: closing Steam took {:.1} s, writing changes {:.1} s, starting it {:.1} s (total {:.1} s). Steam then loads in its own time.",
        closed.as_secs_f32(),
        wrote.as_secs_f32(),
        (total - closed - wrote).as_secs_f32(),
        total.as_secs_f32()
    ));
    written.and(started)
}

/// True while Steam's Big Picture window is open.
pub fn big_picture_open() -> bool {
    #[cfg(windows)]
    {
        let title: Vec<u16> = "Steam Big Picture Mode".encode_utf16().chain([0]).collect();
        // SAFETY: a NUL-terminated title and no class name.
        let hwnd = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(std::ptr::null(), title.as_ptr()) };
        !hwnd.is_null()
    }
    #[cfg(not(windows))]
    false
}

/// "When SteamDrop restarts Steam": whether Steam comes back in Big Picture, and why.
/// "keep" (the default) reopens Big Picture only if its window was open right before.
pub fn restart_mode(setting: &str, big_picture_was_open: bool) -> (bool, &'static str) {
    match setting {
        "bigpicture" => (true, "the setting says always Big Picture"),
        "desktop" => (false, "the setting says always desktop mode"),
        _ if big_picture_was_open => (true, "Big Picture was open right before the restart"),
        _ => (false, "Big Picture wasn't open right before the restart"),
    }
}

/// Decides and logs how Steam reopens. Call it right before closing Steam.
pub fn reopen_in_big_picture(setting: &str) -> bool {
    let (big_picture, why) = restart_mode(setting, big_picture_open());
    crate::log::info(format!("Steam will reopen in {} because {why}.", if big_picture { "Big Picture" } else { "desktop mode" }));
    big_picture
}

/// `SteamDrop.exe --start-big-picture`, run at Windows sign-in when the setting is on:
/// opens Steam in Big Picture, unless it's already there.
pub fn big_picture_at_sign_in() {
    #[cfg(windows)]
    {
        // Give sign-in (and Steam's own start, if it has one) a moment.
        std::thread::sleep(Duration::from_secs(8));
        if is_running() && big_picture_open() {
            crate::log::info("Windows sign-in: Steam is already in Big Picture.");
            return;
        }
        // steam://open/bigpicture starts Steam if needed, or switches a running Steam to Big Picture.
        let opened = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", "steam://open/bigpicture"]).spawn();
        match opened {
            Ok(_) => crate::log::info("Windows sign-in: opened Steam in Big Picture."),
            Err(e) => crate::log::problem(format!("Windows sign-in: could not open Big Picture: {e}")),
        }
    }
}

pub fn start(steam: &Path, big_picture: bool) -> Result<(), String> {
    let mut cmd = std::process::Command::new(steam.join("steam.exe"));
    if big_picture {
        cmd.arg("-tenfoot");
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Could not start Steam: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_mode_keeps_what_was_open() {
        assert!(restart_mode("keep", true).0);
        assert!(!restart_mode("keep", false).0, "desktop Steam must not come back in Big Picture");
        assert!(restart_mode("bigpicture", false).0);
        assert!(!restart_mode("desktop", true).0);
        assert!(!restart_mode("", false).0, "older settings files default to keeping the mode");
    }

    #[test]
    fn a_restart_always_writes_and_reports_the_write_error_first() {
        // No Steam here: closing is a no-op and starting fails, but the write still runs.
        let missing = Path::new("/no/steam/here");
        let mut wrote = false;
        let r = restart_with(missing, "desktop", || {
            wrote = true;
            Ok(())
        });
        assert!(wrote);
        assert!(r.unwrap_err().starts_with("Could not start Steam"));
        let r = restart_with(missing, "desktop", || Err("disk full".into()));
        assert_eq!(r.unwrap_err(), "disk full");
    }

    #[test]
    fn big_picture_is_only_brought_back_over_other_apps() {
        let p = |pid, parent, name: &str| Proc { pid, parent, name: name.into() };
        let procs = vec![p(1, 0, "explorer.exe"), p(2, 1, "steam.exe"), p(3, 2, "launcher.exe"), p(4, 3, "game.exe"), p(5, 1, "discord.exe"), p(6, 1, "lockapp.exe"), p(7, 1, "steamdrop.exe"), p(8, 1, "notepad.exe")];
        let except = vec!["Notepad.exe".to_string()];
        assert!(may_keep_focus(5, &procs, &except, 99).is_none(), "an app that took the focus");
        assert!(may_keep_focus(4, &procs, &except, 99).is_some(), "a game started from Steam, through its launcher");
        assert!(may_keep_focus(6, &procs, &except, 99).is_some(), "the lock screen");
        assert!(may_keep_focus(7, &procs, &except, 99).is_some(), "SteamDrop");
        assert!(may_keep_focus(5, &procs, &except, 5).is_some(), "this process");
        assert!(may_keep_focus(8, &procs, &except, 99).is_some(), "an exception, whatever its case");
        assert!(may_keep_focus(42, &procs, &except, 99).is_some(), "a process that's already gone");
        assert!(from_steam(4, &procs) && from_steam(3, &procs), "a game and its launcher");
        assert!(!from_steam(2, &procs) && !from_steam(5, &procs) && !from_steam(7, &procs), "not Steam itself, other apps or SteamDrop");
    }

    #[test]
    fn only_steamdrops_steam_session_is_ended() {
        let p = |pid, parent, name: &str| Proc { pid, parent, name: name.into() };
        // 2 is Steam; 3 is the helper it started, 4 what the helper started; 5 is the tray app; 6 a game.
        let procs = vec![p(1, 0, "explorer.exe"), p(2, 1, "steam.exe"), p(3, 2, "steamdrop.exe"), p(4, 3, "steamdrop.exe"), p(5, 1, "steamdrop.exe"), p(6, 2, "game.exe")];
        assert_eq!(helper_pids(&procs, 5), vec![3, 4]);
        // The tray app started by the helper (when it wasn't running yet) never ends itself.
        assert_eq!(helper_pids(&procs, 4), vec![3]);
    }

    #[test]
    fn shortcut_ids_have_the_high_bit() {
        let id = shortcut_id("\"C:\\Games\\A\\a.exe\"", "A");
        assert!(id & 0x8000_0000 != 0);
        assert_ne!(id, shortcut_id("\"C:\\Games\\B\\b.exe\"", "B"));
    }

    #[test]
    fn add_replace_remove() {
        let base = std::env::temp_dir().join(format!("steamdrop-steam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let config = base.join("userdata/123/config");
        std::fs::create_dir_all(&config).unwrap();
        let data = base.join("data");
        // Someone else's shortcut must survive untouched.
        let other: Map = vec![(
            "shortcuts".into(),
            Value::Map(vec![("0".into(), Value::Map(vec![("appid".into(), Value::Int(7)), ("appname".into(), Value::Str("Other".into()))]))]),
        )];
        std::fs::write(config.join("shortcuts.vdf"), vdf::write(&other)).unwrap();

        let exe = PathBuf::from("/games/A/a.exe");
        let s = NewShortcut { id: 0x8000_0001, name: "A", exe: &exe, icon: String::new(), launch_options: "" };
        add(&config, &data, "123", &[s]).unwrap();
        let s = NewShortcut { id: 0x8000_0001, name: "A", exe: &exe, icon: String::new(), launch_options: "" };
        add(&config, &data, "123", &[s]).unwrap();
        assert_eq!(present_ids(&config), vec![7, 0x8000_0001]);
        assert_eq!(shortcut_exes(&config), vec![(0x8000_0001, norm_exe(&quote(&exe)))], "shortcuts without a game file are skipped");
        assert_eq!(norm_exe("\"C:/Games/A/A.EXE\""), norm_exe("c:\\games\\a\\a.exe"));
        assert_eq!(users(&base), vec![("123".to_string(), config.clone())]);
        std::fs::create_dir_all(base.join("config")).unwrap();
        std::fs::write(base.join("config/loginusers.vdf"), "\"users\" { \"76561197960265851\" { \"AccountName\" \"leon\" \"PersonaName\" \"Leon\" } }").unwrap();
        assert_eq!(accounts(&base), vec![Account { id: "123".into(), persona: "Leon".into(), login: "leon".into() }]);

        remove(&config, &data, "123", &[0x8000_0001]).unwrap();
        assert_eq!(present_ids(&config), vec![7]);
        let root = vdf::parse(&std::fs::read(config.join("shortcuts.vdf")).unwrap()).unwrap();
        assert_eq!(root, other);
        assert_eq!(std::fs::read_dir(data.join("backups/123")).unwrap().count(), 3);
    }
}
