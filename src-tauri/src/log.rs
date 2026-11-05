//! A plain-language activity log in %APPDATA%\SteamDrop\logs, shown in Settings > Help and
//! included in bug reports. Never pass secrets (like the SteamGridDB key) to it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_BYTES: u64 = 1024 * 1024;
/// steamdrop.log plus steamdrop.1.log ... steamdrop.4.log
const KEEP: usize = 5;
static LOCK: Mutex<()> = Mutex::new(());

pub fn dir() -> PathBuf {
    crate::state::data_dir().join("logs")
}

fn file(dir: &Path, n: usize) -> PathBuf {
    if n == 0 {
        dir.join("steamdrop.log")
    } else {
        dir.join(format!("steamdrop.{n}.log"))
    }
}

pub fn info(msg: impl AsRef<str>) {
    write(&dir(), &format!("{}  {}", timestamp(), msg.as_ref().replace('\n', " ")), MAX_BYTES);
}

pub fn problem(msg: impl AsRef<str>) {
    info(format!("Problem: {}", msg.as_ref()));
}

fn timestamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.format(time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"))
        .unwrap_or_default()
}

fn write(dir: &Path, line: &str, max: u64) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = std::fs::create_dir_all(dir);
    if std::fs::metadata(file(dir, 0)).is_ok_and(|m| m.len() >= max) {
        let _ = std::fs::remove_file(file(dir, KEEP - 1));
        for n in (0..KEEP - 1).rev() {
            let _ = std::fs::rename(file(dir, n), file(dir, n + 1));
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file(dir, 0)) {
        let _ = writeln!(f, "{line}");
    }
    #[cfg(debug_assertions)]
    eprintln!("{line}");
}

/// The last `n` lines, oldest first, across all log files.
pub fn recent(n: usize) -> Vec<String> {
    recent_in(&dir(), n)
}

fn recent_in(dir: &Path, n: usize) -> Vec<String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut lines: Vec<String> = vec![];
    for i in (0..KEEP).rev() {
        if let Ok(text) = std::fs::read_to_string(file(dir, i)) {
            lines.extend(text.lines().map(String::from));
        }
    }
    let skip = lines.len().saturating_sub(n);
    lines.split_off(skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_and_keeps_five_files() {
        let dir = std::env::temp_dir().join(format!("steamdrop-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..40 {
            write(&dir, &format!("line {i:02}"), 30);
        }
        let files = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(files, KEEP);
        let last = recent_in(&dir, 3);
        assert_eq!(last, vec!["line 37", "line 38", "line 39"]);
        assert!(recent_in(&dir, 1000).len() < 40, "oldest lines were rotated away");
    }
}
