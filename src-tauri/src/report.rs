//! "Copy bug report": one Markdown block with what's useful for fixing a problem and nothing private.

use crate::state::App;

/// Replaces the folder name after `\Users\` (any user, any drive) with `<user>`.
pub fn hide_user(text: &str) -> String {
    // ASCII-only lowercasing keeps byte offsets identical to `text`.
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let rest = &lower[i..];
        let hit = ["\\users\\", "/users/"].iter().find(|p| rest.starts_with(*p));
        match hit {
            Some(p) => {
                out.push_str(&text[i..i + p.len()]);
                i += p.len();
                let end = text[i..].find(['\\', '/', '"', '\n']).map_or(text.len(), |e| i + e);
                if end > i {
                    out.push_str("<user>");
                }
                i = end;
            }
            None => {
                let ch = text[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

fn windows_version() -> String {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
        if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") {
            let get = |name: &str| k.get_value::<String, _>(name).unwrap_or_default();
            let build = get("CurrentBuild");
            let mut name = get("ProductName");
            // Windows 11 still reports "Windows 10" here; the build number tells them apart.
            if build.parse::<u32>().is_ok_and(|b| b >= 22000) {
                name = name.replace("Windows 10", "Windows 11");
            }
            return format!("{name} {} (build {build})", get("DisplayVersion")).replace("  ", " ");
        }
    }
    std::env::consts::OS.to_string()
}

pub fn build(app: &App, version: &str) -> String {
    let data = app.data.lock().unwrap().clone();
    let steam = app.steam_path();
    let users = steam.as_deref().map(crate::steam::users).unwrap_or_default();
    let key = crate::secrets::sgdb_key();
    // Settings never contain secrets (the key is in Credential Manager), but be strict anyway.
    let mut settings = serde_json::to_value(&data.settings).unwrap_or_default();
    if let Some(map) = settings.as_object_mut() {
        map.retain(|k, _| !k.contains("key") && !k.contains("token") && !k.contains("password"));
    }
    let report = format!(
        "### SteamDrop bug report\n\n\
         - **SteamDrop:** {version}\n\
         - **Windows:** {}\n\
         - **Steam found:** {}\n\
         - **Steam accounts:** {}\n\
         - **Steam running:** {}\n\
         - **SteamGridDB key:** {}\n\
         - **Games added by SteamDrop:** {}\n\
         - **Waiting for confirmation:** {}\n\n\
         **Settings**\n\n```json\n{}\n```\n\n\
         **Last 200 log lines**\n\n```text\n{}\n```\n",
        windows_version(),
        steam.as_ref().map_or("no".to_string(), |p| format!("yes ({})", p.display())),
        users.len(),
        if crate::steam::is_running() { "yes" } else { "no" },
        if key.is_empty() { "not set" } else { "set" },
        data.games.len(),
        app.found.lock().unwrap().len(),
        serde_json::to_string_pretty(&settings).unwrap_or_default(),
        crate::log::recent(200).join("\n"),
    );
    let report = hide_user(&report);
    if key.len() >= 4 { report.replace(&key, "<hidden>") } else { report }
}

#[cfg(test)]
mod tests {
    use super::hide_user;

    /// Uses the real Credential Manager, so it only runs on Windows.
    #[cfg(windows)]
    #[test]
    fn report_never_contains_the_key() {
        let key = "very-secret-key-42";
        crate::secrets::set_sgdb_key(key).unwrap();
        assert_eq!(crate::secrets::sgdb_key(), key);
        crate::log::info(format!(r"Test line from C:\Users\Leon\Games\X with {key}"));
        let report = super::build(&crate::state::App::new(), "9.9.9");
        crate::secrets::set_sgdb_key("").unwrap();
        assert_eq!(crate::secrets::sgdb_key(), "");
        assert!(!report.contains(key), "{report}");
        assert!(report.contains(r"C:\Users\<user>\Games\X with <hidden>"), "{report}");
        assert!(report.contains("**SteamGridDB key:** set"));
        assert!(report.contains("**SteamDrop:** 9.9.9"));
    }

    #[test]
    fn hides_user_names_in_paths() {
        assert_eq!(hide_user(r"C:\Users\John Smith\Games\X"), r"C:\Users\<user>\Games\X");
        assert_eq!(hide_user(r#""D:\users\leon""#), r#""D:\users\<user>""#);
        assert_eq!(hide_user("c:/Users/ana/AppData"), "c:/Users/<user>/AppData");
        assert_eq!(hide_user(r"Added Größe from C:\Users\Zoë\Spiele"), r"Added Größe from C:\Users\<user>\Spiele");
        assert_eq!(hide_user(r"C:\Program Files (x86)\Steam"), r"C:\Program Files (x86)\Steam");
    }
}
