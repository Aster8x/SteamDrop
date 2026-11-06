//! Everything that touches Steam: where it lives, its users, shortcuts.vdf and its grid folder.

use crate::vdf::{self, Map, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

#[cfg(windows)]
fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}

pub fn is_running() -> bool {
    #[cfg(windows)]
    {
        let out = hidden(std::process::Command::new("tasklist").args(["/FI", "IMAGENAME eq steam.exe", "/NH"])).output();
        out.map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("steam.exe")).unwrap_or(false)
    }
    #[cfg(not(windows))]
    false
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
        if steam_game != 0 {
            return true;
        }
        let Ok(out) = hidden(std::process::Command::new("tasklist").args(["/FO", "CSV", "/NH"])).output() else { return false };
        let running: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split("\",\"").next().map(|n| n.trim_matches('"').to_lowercase()))
            .collect();
        games.iter().any(|g| {
            let name = Path::new(&g.exe).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            running.contains(&name)
        })
    }
    #[cfg(not(windows))]
    {
        let _ = games;
        false
    }
}

/// Asks Steam to close and waits (up to a minute) until it has.
/// Steam must be closed while shortcuts.vdf changes, or it overwrites them on exit.
pub fn shutdown(steam: &Path) -> Result<(), String> {
    if !is_running() {
        return Ok(());
    }
    std::process::Command::new(steam.join("steam.exe"))
        .arg("-shutdown")
        .spawn()
        .map_err(|e| format!("Could not ask Steam to close: {e}"))?;
    for _ in 0..120 {
        std::thread::sleep(Duration::from_millis(500));
        if !is_running() {
            // Give Steam a moment to finish writing its own files.
            std::thread::sleep(Duration::from_secs(2));
            return Ok(());
        }
    }
    Err("Steam did not close within a minute. Close Steam yourself, then try again.".into())
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
