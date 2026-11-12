//! Secrets live in Windows Credential Manager, never in data.json or the logs:
//! the SteamGridDB key.

const SERVICE: &str = "SteamDrop";
const SGDB: &str = "SteamGridDB API key";

/// The saved value, or "" when there is none.
fn get(account: &str) -> String {
    keyring::Entry::new(SERVICE, account).and_then(|e| e.get_password()).unwrap_or_default()
}

/// Saves the value; an empty value removes it.
fn set(account: &str, value: &str) -> Result<(), String> {
    let e = keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
    let result = if value.is_empty() { e.delete_credential() } else { e.set_password(value) };
    match result {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("Windows could not save it ({err}).")),
    }
}

pub fn sgdb_key() -> String {
    get(SGDB)
}

pub fn set_sgdb_key(key: &str) -> Result<(), String> {
    set(SGDB, key)
}

/// Removes the PIN older builds could save (deleting games no longer asks for one).
pub fn forget_old_pin() {
    let _ = set("Couch mode PIN", "");
}
