//! Game details from Steam's public store API, fetched on demand and cached.

use crate::exe::similarity;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Info {
    pub appid: u32,
    pub name: String,
    pub description: String,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    pub release_date: String,
    pub store_url: String,
}

/// Looks a title up in the Steam store; only an exact (ignoring punctuation and case) match counts.
pub async fn search(http: &Client, title: &str) -> Option<u32> {
    let mut url = Url::parse("https://store.steampowered.com/api/storesearch/").ok()?;
    url.query_pairs_mut().append_pair("term", title).append_pair("l", "english").append_pair("cc", "US");
    let json: serde_json::Value = http.get(url).send().await.ok()?.json().await.ok()?;
    json["items"]
        .as_array()?
        .iter()
        .find(|i| i["name"].as_str().is_some_and(|n| similarity(title, n) >= 1.0))
        .and_then(|i| i["id"].as_u64())
        .map(|id| id as u32)
}

pub async fn details(http: &Client, data_dir: &Path, appid: u32) -> Result<Info, String> {
    let cache = data_dir.join("info").join(format!("{appid}.json"));
    if let Some(info) = std::fs::read(&cache).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        return Ok(info);
    }
    let url = format!("https://store.steampowered.com/api/appdetails?appids={appid}&l=english");
    let json: serde_json::Value = http
        .get(url)
        .send()
        .await
        .map_err(|_| "Could not reach the Steam store. Check your internet connection.")?
        .json()
        .await
        .map_err(|_| "The Steam store gave an answer SteamDrop couldn't read. Try again later.")?;
    let entry = &json[appid.to_string()];
    if entry["success"].as_bool() != Some(true) {
        return Err("The Steam store has no page for this game.".into());
    }
    let d = &entry["data"];
    let list = |v: &serde_json::Value, key: Option<&str>| -> Vec<String> {
        v.as_array()
            .map(|a| a.iter().filter_map(|x| key.map_or(x.as_str(), |k| x[k].as_str())).map(String::from).collect())
            .unwrap_or_default()
    };
    let info = Info {
        appid,
        name: d["name"].as_str().unwrap_or_default().to_string(),
        description: plain_text(d["short_description"].as_str().unwrap_or_default()),
        developers: list(&d["developers"], None),
        publishers: list(&d["publishers"], None),
        genres: list(&d["genres"], Some("description")),
        release_date: d["release_date"]["date"].as_str().unwrap_or_default().to_string(),
        store_url: format!("https://store.steampowered.com/app/{appid}/"),
    };
    let _ = std::fs::create_dir_all(cache.parent().unwrap());
    let _ = std::fs::write(&cache, serde_json::to_vec(&info).unwrap());
    Ok(info)
}

/// Store descriptions can carry HTML tags and entities.
fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn strips_html() {
        assert_eq!(super::plain_text("<b>Tom &amp; Jerry&#39;s</b> game"), "Tom & Jerry's game");
    }
}
