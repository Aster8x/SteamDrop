//! Library artwork: official Steam art, then SteamGridDB, then a plain title card.
//! Downloads are cached in %APPDATA%\SteamDrop\art\<id>\ and copied into Steam's grid folder.

use crate::exe::similarity;
use crate::titlecard::{self, Kind};
use reqwest::{Client, Url};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    Portrait,
    Wide,
    Hero,
    Logo,
    Icon,
}

pub const SLOTS: [Slot; 5] = [Slot::Portrait, Slot::Wide, Slot::Hero, Slot::Logo, Slot::Icon];
const EXTS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "ico"];
pub const GAME_FOLDER_ART: &str = "SteamDrop art";

impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Slot::Portrait => "portrait",
            Slot::Wide => "wide",
            Slot::Hero => "hero",
            Slot::Logo => "logo",
            Slot::Icon => "icon",
        }
    }

    /// File name (without extension) Steam looks for in its grid folder.
    fn grid_name(self, id: u32) -> String {
        match self {
            Slot::Portrait => format!("{id}p"),
            Slot::Wide => format!("{id}"),
            Slot::Hero => format!("{id}_hero"),
            Slot::Logo => format!("{id}_logo"),
            Slot::Icon => format!("{id}_icon"),
        }
    }

    /// Files on Steam's public CDN, best first.
    fn steam_files(self) -> &'static [&'static str] {
        match self {
            Slot::Portrait => &["library_600x900_2x.jpg", "library_600x900.jpg"],
            Slot::Wide => &["header.jpg"],
            Slot::Hero => &["library_hero.jpg"],
            Slot::Logo => &["logo.png"],
            Slot::Icon => &[],
        }
    }

    /// SteamGridDB endpoint and filters.
    fn sgdb(self) -> (&'static str, &'static str) {
        match self {
            Slot::Portrait => ("grids", "dimensions=600x900,342x482,660x930&mimes=image/png,image/jpeg"),
            Slot::Wide => ("grids", "dimensions=920x430,460x215&mimes=image/png,image/jpeg"),
            Slot::Hero => ("heroes", "mimes=image/png,image/jpeg"),
            Slot::Logo => ("logos", "mimes=image/png"),
            Slot::Icon => ("icons", "mimes=image/png,image/vnd.microsoft.icon"),
        }
    }

    fn title_card(self, title: &str) -> Option<Vec<u8>> {
        match self {
            Slot::Portrait => Some(titlecard::render(title, 600, 900, Kind::Cover)),
            Slot::Wide => Some(titlecard::render(title, 920, 430, Kind::Cover)),
            Slot::Hero => Some(titlecard::render(title, 1920, 620, Kind::Hero)),
            Slot::Logo => Some(titlecard::render(title, 1280, 720, Kind::Logo)),
            // Without a real icon Steam shows the game's own .exe icon, which beats a text square.
            Slot::Icon => None,
        }
    }
}

/// The file extension that matches an image's contents, or None if it isn't an image we can use.
fn image_ext(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [0, 0, 1, 0, ..] => Some("ico"),
        _ => None,
    }
}

fn find(dir: &Path, stem: &str) -> Option<PathBuf> {
    EXTS.iter().map(|e| dir.join(format!("{stem}.{e}"))).find(|p| p.is_file())
}

pub fn cache_dir(data_dir: &Path, id: u32) -> PathBuf {
    data_dir.join("art").join(id.to_string())
}

pub fn cached(data_dir: &Path, id: u32, slot: Slot) -> Option<PathBuf> {
    find(&cache_dir(data_dir, id), slot.name())
}

pub struct Request<'a> {
    pub id: u32,
    pub title: &'a str,
    pub folder: &'a Path,
    pub steam_appid: Option<u32>,
    pub sgdb_key: &'a str,
    pub copy_to_game_folder: bool,
}

/// Makes sure every slot has art in the cache. Returns the cached file per slot.
pub async fn fetch(http: &Client, data_dir: &Path, r: &Request<'_>) -> Vec<(Slot, PathBuf)> {
    let cache = cache_dir(data_dir, r.id);
    let _ = std::fs::create_dir_all(&cache);
    let game_art = r.folder.join(GAME_FOLDER_ART);
    let mut sgdb_game: Option<Option<u64>> = None;
    let mut out = vec![];
    for slot in SLOTS {
        if let Some(p) = find(&cache, slot.name()) {
            out.push((slot, p));
            continue;
        }
        let mut bytes = find(&game_art, slot.name()).and_then(|p| std::fs::read(p).ok());
        if bytes.is_none() {
            if let Some(appid) = r.steam_appid {
                bytes = from_steam(http, appid, slot).await;
            }
        }
        if bytes.is_none() && !r.sgdb_key.trim().is_empty() {
            if sgdb_game.is_none() {
                sgdb_game = Some(sgdb_find(http, r.sgdb_key, r.title).await);
            }
            bytes = from_sgdb(http, r.sgdb_key, r.steam_appid, sgdb_game.flatten(), slot).await;
        }
        let bytes = bytes.or_else(|| slot.title_card(r.title));
        let Some(bytes) = bytes else { continue };
        let Some(ext) = image_ext(&bytes) else { continue };
        let path = cache.join(format!("{}.{ext}", slot.name()));
        if std::fs::write(&path, &bytes).is_ok() {
            out.push((slot, path));
        }
    }
    if r.copy_to_game_folder && std::fs::create_dir_all(&game_art).is_ok() {
        for (slot, p) in &out {
            clear(&game_art, slot.name());
            let _ = std::fs::copy(p, game_art.join(p.file_name().unwrap()));
        }
    }
    out
}

async fn get_image(req: reqwest::RequestBuilder) -> Option<Vec<u8>> {
    let res = req.send().await.ok()?.error_for_status().ok()?;
    let bytes = res.bytes().await.ok()?;
    // Tiny files are Steam's "no image" placeholders.
    (bytes.len() > 1024 && image_ext(&bytes).is_some()).then(|| bytes.to_vec())
}

async fn from_steam(http: &Client, appid: u32, slot: Slot) -> Option<Vec<u8>> {
    for file in slot.steam_files() {
        for base in ["https://shared.steamstatic.com/store_item_assets/steam/apps", "https://cdn.cloudflare.steamstatic.com/steam/apps"] {
            if let Some(b) = get_image(http.get(format!("{base}/{appid}/{file}"))).await {
                return Some(b);
            }
        }
    }
    None
}

const SGDB: &str = "https://www.steamgriddb.com/api/v2";

/// SteamGridDB's id for a game title, only if the best match really is that game.
async fn sgdb_find(http: &Client, key: &str, title: &str) -> Option<u64> {
    let mut url = Url::parse(&format!("{SGDB}/search/autocomplete")).ok()?;
    url.path_segments_mut().ok()?.push(title);
    let json: serde_json::Value = http.get(url).bearer_auth(key.trim()).send().await.ok()?.json().await.ok()?;
    let first = json["data"].as_array()?.first()?;
    let name = first["name"].as_str()?;
    (similarity(title, name) >= 0.6).then(|| first["id"].as_u64()).flatten()
}

async fn from_sgdb(http: &Client, key: &str, appid: Option<u32>, game: Option<u64>, slot: Slot) -> Option<Vec<u8>> {
    let (kind, filters) = slot.sgdb();
    let target = match (appid, game) {
        (Some(a), _) => format!("steam/{a}"),
        (None, Some(g)) => format!("game/{g}"),
        _ => return None,
    };
    let url = format!("{SGDB}/{kind}/{target}?types=static&nsfw=false&humor=false&epilepsy=false&{filters}");
    let json: serde_json::Value = http.get(url).bearer_auth(key.trim()).send().await.ok()?.json().await.ok()?;
    for item in json["data"].as_array()?.iter().take(3) {
        if let Some(b) = get_image(http.get(item["url"].as_str()?)).await {
            return Some(b);
        }
    }
    None
}

/// True if the key works, for the settings screen.
pub async fn sgdb_key_ok(http: &Client, key: &str) -> bool {
    let res = http.get(format!("{SGDB}/grids/steam/220")).bearer_auth(key.trim()).send().await;
    res.is_ok_and(|r| r.status().is_success())
}

fn clear(dir: &Path, stem: &str) {
    for e in EXTS {
        let _ = std::fs::remove_file(dir.join(format!("{stem}.{e}")));
    }
}

/// Copies cached art into one Steam user's grid folder. Returns the icon's path, if there is one.
pub fn install(grid: &Path, id: u32, art: &[(Slot, PathBuf)]) -> Option<PathBuf> {
    let _ = std::fs::create_dir_all(grid);
    let mut icon = None;
    for (slot, src) in art {
        let stem = slot.grid_name(id);
        clear(grid, &stem);
        let dest = grid.join(format!("{stem}.{}", src.extension()?.to_string_lossy()));
        if std::fs::copy(src, &dest).is_ok() && *slot == Slot::Icon {
            icon = Some(dest);
        }
    }
    icon
}

/// Removes a game's art from one Steam user's grid folder.
pub fn uninstall(grid: &Path, id: u32) {
    for slot in SLOTS {
        clear(grid, &slot.grid_name(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_names_match_steam() {
        assert_eq!(Slot::Portrait.grid_name(5), "5p");
        assert_eq!(Slot::Wide.grid_name(5), "5");
        assert_eq!(Slot::Hero.grid_name(5), "5_hero");
        assert_eq!(Slot::Logo.grid_name(5), "5_logo");
    }

    #[test]
    fn title_cards_fill_the_cache_and_grid() {
        let base = std::env::temp_dir().join(format!("steamdrop-art-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let folder = base.join("game");
        std::fs::create_dir_all(&folder).unwrap();
        let r = Request { id: 9, title: "Test Game", folder: &folder, steam_appid: None, sgdb_key: "", copy_to_game_folder: true };
        let rt = tauri::async_runtime::block_on(fetch(&Client::new(), &base, &r));
        assert_eq!(rt.len(), 4, "no icon without a real source");
        assert!(folder.join(GAME_FOLDER_ART).join("portrait.png").is_file());
        let grid = base.join("grid");
        assert_eq!(install(&grid, 9, &rt), None);
        assert!(grid.join("9p.png").is_file() && grid.join("9_logo.png").is_file());
        uninstall(&grid, 9);
        assert_eq!(std::fs::read_dir(&grid).unwrap().count(), 0);
    }
}
