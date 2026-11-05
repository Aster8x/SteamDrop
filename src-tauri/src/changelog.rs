//! Reads CHANGELOG.md (Keep a Changelog format) into one section per version.

use serde::Serialize;

/// The changelog this build shipped with, for the "What's new" screen.
pub const BUNDLED: &str = include_str!("../../CHANGELOG.md");

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Section {
    pub version: String,
    pub date: String,
    /// The section's Markdown, without its "## [x.y.z]" heading.
    pub body: String,
}

/// Released versions in file order (newest first). "Unreleased" is skipped.
pub fn parse(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = vec![];
    let mut lines: Vec<&str> = vec![];
    let mut current: Option<(String, String)> = None;
    let flush = |current: &mut Option<(String, String)>, lines: &mut Vec<&str>, out: &mut Vec<Section>| {
        if let Some((version, date)) = current.take() {
            out.push(Section { version, date, body: lines.join("\n").trim().to_string() });
        }
        lines.clear();
    };
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("## [") {
            flush(&mut current, &mut lines, &mut out);
            let (version, tail) = rest.split_once(']').unwrap_or((rest, ""));
            if semver::Version::parse(version).is_ok() {
                current = Some((version.to_string(), tail.trim_start_matches([' ', '-']).trim().to_string()));
            }
        } else if line.starts_with('[') && line.contains("]: ") {
            flush(&mut current, &mut lines, &mut out); // link list at the bottom
        } else if current.is_some() {
            lines.push(line);
        }
    }
    flush(&mut current, &mut lines, &mut out);
    out
}

/// Sections newer than `current`, up to and including `new`, newest first.
pub fn between(text: &str, current: &str, new: &str) -> Vec<Section> {
    let (Ok(cur), Ok(new)) = (semver::Version::parse(current), semver::Version::parse(new)) else { return vec![] };
    let mut list: Vec<Section> = parse(text)
        .into_iter()
        .filter(|s| semver::Version::parse(&s.version).is_ok_and(|v| v > cur && v <= new))
        .collect();
    list.sort_by(|a, b| semver::Version::parse(&b.version).unwrap().cmp(&semver::Version::parse(&a.version).unwrap()));
    list
}

pub fn find(text: &str, version: &str) -> Option<Section> {
    parse(text).into_iter().find(|s| s.version == version)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog\n\n## [Unreleased]\n\n- wip\n\n## [0.3.0] - 2027-01-02\n\n### Added\n\n- C\n\n## [0.2.0] - 2026-12-01\r\n\r\n- B\r\n\n## [0.1.0] - 2026-11-05\n\n- A\n\n[Unreleased]: x\n[0.1.0]: y\n";

    #[test]
    fn parses_sections() {
        let all = parse(LOG);
        assert_eq!(all.iter().map(|s| s.version.as_str()).collect::<Vec<_>>(), ["0.3.0", "0.2.0", "0.1.0"]);
        assert_eq!(all[0].body, "### Added\n\n- C");
        assert_eq!(all[1].date, "2026-12-01");
        assert_eq!(all[2].body, "- A");
    }

    #[test]
    fn picks_versions_between() {
        let v: Vec<String> = between(LOG, "0.1.0", "0.3.0").into_iter().map(|s| s.version).collect();
        assert_eq!(v, ["0.3.0", "0.2.0"]);
        assert!(between(LOG, "0.3.0", "0.3.0").is_empty());
        assert_eq!(find(LOG, "0.2.0").unwrap().body, "- B");
    }

    #[test]
    fn bundled_changelog_has_this_version() {
        assert!(find(BUNDLED, env!("CARGO_PKG_VERSION")).is_some());
    }
}
