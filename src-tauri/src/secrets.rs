//! The SteamGridDB key lives in Windows Credential Manager, never in data.json or the logs.

const SERVICE: &str = "SteamDrop";
const ACCOUNT: &str = "SteamGridDB API key";

fn entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT)
}

/// The saved key, or "" when there is none.
pub fn sgdb_key() -> String {
    entry().and_then(|e| e.get_password()).unwrap_or_default()
}

/// Saves the key; an empty key removes it.
pub fn set_sgdb_key(key: &str) -> Result<(), String> {
    let e = entry().map_err(|e| e.to_string())?;
    let result = if key.is_empty() { e.delete_credential() } else { e.set_password(key) };
    match result {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("Windows could not save the key ({err}).")),
    }
}
