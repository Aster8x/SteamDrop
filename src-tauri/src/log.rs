//! A plain-language activity log in %APPDATA%\SteamDrop\logs, one file per day
//! (steamdrop-YYYY-MM-DD.log), shown in Settings > Help and included in bug reports.
//! Never pass secrets (like the SteamGridDB key) to it.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Days of logs kept, and the most all log files may take together (a safety net).
const KEEP_DAYS: i64 = 14;
const MAX_TOTAL: u64 = 20 * 1024 * 1024;
/// Today's file, kept open so writing a line is one append; reopened when the day changes.
static OPEN: Mutex<Option<(PathBuf, File)>> = Mutex::new(None);

pub fn dir() -> PathBuf {
    crate::state::data_dir().join("logs")
}

fn file(dir: &Path, day: &str) -> PathBuf {
    dir.join(format!("steamdrop-{day}.log"))
}

pub fn info(msg: impl AsRef<str>) {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let stamp = now.format(time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]")).unwrap_or_default();
    let line = format!("{stamp}  {}", msg.as_ref().replace('\n', " "));
    write(&dir(), &stamp[..10.min(stamp.len())], &line, MAX_TOTAL);
}

pub fn problem(msg: impl AsRef<str>) {
    info(format!("Problem: {}", msg.as_ref()));
}

fn write(dir: &Path, day: &str, line: &str, max_total: u64) {
    let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
    let path = file(dir, day);
    if open.as_ref().is_none_or(|(p, _)| *p != path) {
        // A new day (or the first line since SteamDrop started): tidy up once, then open today's file.
        *open = None;
        let _ = std::fs::create_dir_all(dir);
        clean(dir, day, max_total);
        *open = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok().map(|f| (path, f));
    }
    if let Some((_, f)) = open.as_mut() {
        // A runaway day can't fill the disk: start today's file over past the cap.
        if f.metadata().is_ok_and(|m| m.len() > max_total) {
            let _ = f.set_len(0);
        }
        let _ = writeln!(f, "{line}");
    }
    #[cfg(debug_assertions)]
    eprintln!("{line}");
}

/// The day files, oldest first, as (day, path).
fn days(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut list: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let day = name.strip_prefix("steamdrop-")?.strip_suffix(".log")?.to_string();
            Some((day, e.path()))
        })
        .collect();
    list.sort();
    list
}

/// Deletes day files older than KEEP_DAYS, then the oldest ones while all together are over the cap
/// (never today's), and the rotating files older versions used.
fn clean(dir: &Path, today: &str, max_total: u64) {
    for old in ["steamdrop.log", "steamdrop.1.log", "steamdrop.2.log", "steamdrop.3.log", "steamdrop.4.log"] {
        let _ = std::fs::remove_file(dir.join(old));
    }
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let cutoff = time::Date::parse(today, format).ok().and_then(|d| d.checked_sub(time::Duration::days(KEEP_DAYS))).and_then(|d| d.format(format).ok());
    let mut list = days(dir);
    list.retain(|(day, path)| {
        let old = cutoff.as_deref().is_some_and(|c| day.as_str() < c);
        if old {
            let _ = std::fs::remove_file(path);
        }
        !old
    });
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let mut total: u64 = list.iter().map(|(_, p)| size(p)).sum();
    for (day, path) in &list {
        if total <= max_total || day == today {
            break;
        }
        total -= size(path);
        let _ = std::fs::remove_file(path);
    }
}

/// The last `n` lines, oldest first, across the most recent day files.
pub fn recent(n: usize) -> Vec<String> {
    recent_in(&dir(), n)
}

fn recent_in(dir: &Path, n: usize) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for (_, path) in days(dir).iter().rev() {
        if lines.len() >= n {
            break;
        }
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let mut older: Vec<String> = text.lines().map(String::from).collect();
        older.append(&mut lines);
        lines = older;
    }
    let skip = lines.len().saturating_sub(n);
    lines.split_off(skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_file_per_day_kept_for_two_weeks_and_under_the_cap() {
        let dir = std::env::temp_dir().join(format!("steamdrop-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("steamdrop.log"), "old rotating file").unwrap();
        for day in ["2026-10-01", "2026-10-22", "2026-10-23"] {
            write(&dir, day, &format!("{day} line"), 1_000_000);
        }
        write(&dir, "2026-11-05", "a", 1_000_000);
        write(&dir, "2026-11-05", "b", 1_000_000);
        let names: Vec<String> = days(&dir).into_iter().map(|(d, _)| d).collect();
        assert_eq!(names, ["2026-10-22", "2026-10-23", "2026-11-05"], "older than 14 days is gone, one file per day");
        assert!(!dir.join("steamdrop.log").exists());
        assert_eq!(recent_in(&dir, 3), ["2026-10-23 line", "a", "b"], "recent lines span the day files");
        // Over the total cap: the oldest days go first, never today's.
        write(&dir, "2026-11-06", "today", 3);
        let names: Vec<String> = days(&dir).into_iter().map(|(d, _)| d).collect();
        assert_eq!(names, ["2026-11-06"]);
        assert_eq!(recent_in(&dir, 10), ["today"]);
    }
}
