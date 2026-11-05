//! Looking inside a game folder: which .exe starts the game, is it only an
//! installer, and does it say which Steam game it is.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub size: u64,
    pub score: f64,
}

const MAX_DEPTH: usize = 4;
const MAX_ENTRIES: usize = 20_000;

/// File names (lowercase, without .exe) that are never the game.
const EXCLUDE_WORDS: &[&str] = &[
    "unins", "uninstall", "setup", "install", "crashhandler", "crashreport", "crashsender",
    "crashpad", "crash_", "reporter", "redist", "vcredist",
    "vc_redist", "dxsetup", "dxwebsetup", "directx", "dotnet", "dotnetfx", "ndp4", "physx",
    "prereq", "oalinst", "easyanticheat", "eac_setup", "beservice", "battleye", "helper",
    "update", "patcher", "cefprocess", "cef_", "subprocess", "webhelper", "qtwebengine",
    "notification", "dxdiag", "touchup", "7z", "python", "register", "activation", "cleanup",
    "repair", "benchmark", "bugreport", "sendrpt", "elevate",
];

/// Folder names (lowercase) whose contents are never the game.
const EXCLUDE_DIRS: &[&str] = &[
    "_commonredist", "commonredist", "redist", "redistributables", "__installer", "installer",
    "directx", "dotnet", "vcredist", "prerequisites", "prereqs", "support", "easyanticheat",
    "battleye", "crashreporter", "crashpad", "tools", "sdk", "docs", "manual", "extras",
];

fn walk(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, u64, usize)>, budget: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if ft.is_dir() && depth < MAX_DEPTH && !EXCLUDE_DIRS.contains(&name.as_str()) {
            walk(&path, depth + 1, out, budget);
        } else if ft.is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push((path, size, depth));
        }
    }
}

fn files(folder: &Path) -> Vec<(PathBuf, u64, usize)> {
    let mut out = Vec::new();
    walk(folder, 0, &mut out, &mut MAX_ENTRIES.clone());
    out
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn is_exe(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

fn excluded(stem: &str) -> bool {
    EXCLUDE_WORDS.iter().any(|w| stem.contains(w))
}

/// Every plausible game .exe in the folder, best guess first.
pub fn candidates(folder: &Path) -> Vec<Candidate> {
    let title = clean_title(&folder.file_name().unwrap_or_default().to_string_lossy());
    let mut list: Vec<Candidate> = files(folder)
        .into_iter()
        .filter(|(p, _, _)| is_exe(p) && !excluded(&stem(p)))
        .map(|(path, size, depth)| {
            let score = score(&title, &stem(&path), size, depth);
            Candidate { path, size, score }
        })
        .collect();
    list.sort_by(|a, b| b.score.total_cmp(&a.score));
    list
}

fn score(title: &str, stem: &str, size: u64, depth: usize) -> f64 {
    let name = similarity(title, stem) * 50.0;
    // Real game binaries are rarely tiny; ~1 MB scores 0, ~100 MB scores ~13.
    let mb = size as f64 / 1_048_576.0;
    let size_pts = (mb.max(0.01).log2() * 2.0).clamp(-10.0, 16.0);
    let depth_pts = depth as f64 * -12.0;
    let launcher = if stem.contains("launcher") || stem.contains("config") { -6.0 } else { 0.0 };
    name + size_pts + depth_pts + launcher
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// 0..1: how much an exe name looks like the game title.
pub fn similarity(title: &str, stem: &str) -> f64 {
    let (t, s) = (norm(title), norm(stem));
    if t.is_empty() || s.is_empty() {
        return 0.0;
    }
    if t == s {
        return 1.0;
    }
    let initials: String = title
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_lowercase)
        .collect();
    // "GTA5.exe" for "Grand Theft Auto V": initials, with the sequel number written either way.
    let core = s.trim_end_matches(|c: char| c.is_ascii_digit());
    if initials.len() >= 2 && (s.starts_with(&initials) || (core.len() >= 2 && initials.starts_with(core))) {
        return 0.85;
    }
    if t.contains(&s) || s.contains(&t) {
        return 0.6 + 0.3 * (t.len().min(s.len()) as f64 / t.len().max(s.len()) as f64);
    }
    dice(&t, &s)
}

fn dice(a: &str, b: &str) -> f64 {
    let pairs = |s: &str| -> Vec<(char, char)> {
        let c: Vec<char> = s.chars().collect();
        c.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (pa, mut pb) = (pairs(a), pairs(b));
    if pa.is_empty() || pb.is_empty() {
        return 0.0;
    }
    let total = (pa.len() + pb.len()) as f64;
    let mut hits = 0.0;
    for p in &pa {
        if let Some(i) = pb.iter().position(|q| q == p) {
            pb.swap_remove(i);
            hits += 2.0;
        }
    }
    hits / total
}

/// Folder name -> readable game title: drops version numbers, bracketed tags and separators.
pub fn clean_title(folder: &str) -> String {
    let mut s = String::new();
    let mut depth = 0;
    let mut chars = folder.chars().peekable();
    while let Some(c) = chars.next() {
        // A dot only separates when a version number follows ("v1.5"), so "S.T.A.L.K.E.R." survives.
        let version_dot = c == '.' && chars.peek().is_some_and(|n| n.is_ascii_digit());
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = (depth - 1).max(0),
            '_' if depth == 0 => s.push(' '),
            '.' if depth == 0 && version_dot => s.push(' '),
            _ if depth == 0 => s.push(c),
            _ => {}
        }
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    let keep = words
        .iter()
        .position(|w| {
            let l = w.to_lowercase();
            let versionish = (l.starts_with('v') && l[1..].starts_with(|c: char| c.is_ascii_digit()))
                || l == "build"
                || l == "update"
                || (l.chars().all(|c| c.is_ascii_digit()) && l.len() >= 4 && !l.starts_with("19") && !l.starts_with("20"));
            versionish
        })
        .unwrap_or(words.len());
    let title = words[..keep].join(" ");
    let title = title.trim_matches(|c: char| c == '-' || c.is_whitespace()).to_string();
    if title.is_empty() { folder.trim().to_string() } else { title }
}

/// True when the folder holds an installer but nothing that looks like a playable game.
pub fn installer_only(folder: &Path) -> bool {
    let all = files(folder);
    let installer = all.iter().any(|(p, _, d)| {
        let s = stem(p);
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        *d <= 1 && ((ext == "exe" && (s.starts_with("setup") || s.contains("install"))) || ext == "msi")
    });
    installer && candidates(folder).is_empty()
}

/// The Steam App ID from a steam_appid.txt anywhere near the top of the folder.
pub fn find_appid(folder: &Path) -> Option<u32> {
    files(folder)
        .into_iter()
        .filter(|(p, _, d)| *d <= 3 && p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steam_appid.txt")))
        .find_map(|(p, _, _)| std::fs::read_to_string(p).ok()?.trim().parse().ok())
        .filter(|&id| id > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("steamdrop-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn file(dir: &Path, rel: &str, size: usize) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![0u8; size]).unwrap();
    }

    #[test]
    fn titles() {
        assert_eq!(clean_title("Hollow_Knight_v1.5.78"), "Hollow Knight");
        assert_eq!(clean_title("Stardew Valley [GOG] (1.6)"), "Stardew Valley");
        assert_eq!(clean_title("Celeste - build 1234"), "Celeste");
        assert_eq!(clean_title("Doom 1993"), "Doom 1993");
        assert_eq!(clean_title("[weird]"), "[weird]");
        assert_eq!(clean_title("S.T.A.L.K.E.R. Shadow of Chernobyl"), "S.T.A.L.K.E.R. Shadow of Chernobyl");
    }

    #[test]
    fn similarity_prefers_matching_names() {
        assert!(similarity("Hollow Knight", "hollow_knight") > 0.99);
        assert!(similarity("Grand Theft Auto V", "GTA5") >= 0.85);
        assert!(similarity("Hollow Knight", "UnityCrash") < 0.3);
    }

    #[test]
    fn picks_the_game_not_the_junk() {
        let d = tmp("pick").join("Hollow Knight");
        file(&d, "hollow_knight.exe", 600_000);
        file(&d, "UnityCrashHandler64.exe", 1_500_000);
        file(&d, "unins000.exe", 1_200_000);
        file(&d, "UE4PrereqSetup_x64.exe", 1_200_000);
        file(&d, "_CommonRedist/vcredist_x64.exe", 9_000_000);
        file(&d, "tools/editor.exe", 50_000_000);
        let c = candidates(&d);
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(c[0].path.ends_with("hollow_knight.exe"));
        assert!(!installer_only(&d));
    }

    #[test]
    fn unreal_prefers_root_bootstrap() {
        let d = tmp("ue").join("Some Game");
        file(&d, "SomeGame.exe", 300_000);
        file(&d, "SomeGame/Binaries/Win64/SomeGame-Win64-Shipping.exe", 80_000_000);
        file(&d, "Engine/Binaries/Win64/CrashReportClient.exe", 20_000_000);
        let c = candidates(&d);
        assert!(c[0].path.ends_with("SomeGame.exe"), "{c:?}");
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn detects_installer_only() {
        let d = tmp("inst").join("Cool Game");
        file(&d, "setup_cool_game_1.0.exe", 2_000_000);
        file(&d, "setup_cool_game_1.0-1.bin", 10_000);
        assert!(installer_only(&d));
        assert!(candidates(&d).is_empty());
    }

    #[test]
    fn reads_appid() {
        let d = tmp("appid");
        file(&d, "game.exe", 10);
        fs::create_dir_all(d.join("steam_settings")).unwrap();
        fs::write(d.join("steam_settings/steam_appid.txt"), " 413150\r\n").unwrap();
        assert_eq!(find_appid(&d), Some(413150));
    }
}
