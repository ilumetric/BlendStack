//! Stable releases scraped from the download.blender.org directory listing.

use super::platform;
use crate::error::Result;
use crate::model::{Channel, RemoteBuild, Version};
use crate::settings::{read_json, write_json};
use futures_util::{stream, StreamExt};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

const RELEASE_ROOT: &str = "https://download.blender.org/release/";
/// The newest series are re-scanned on every refresh; older ones never change.
const ALWAYS_REFRESH: usize = 3;

static FOLDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="Blender(\d+)\.(\d+)/""#).unwrap());
static ROW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a href="([^"]+)">[^<]*</a>\s+(\d{2}-[A-Za-z]{3}-\d{4} \d{2}:\d{2})\s+(\d+)"#)
        .unwrap()
});

type FolderCache = HashMap<String, Vec<RemoteBuild>>;

pub async fn fetch(
    client: &reqwest::Client,
    cache_dir: &Path,
    include_legacy: bool,
) -> Result<Vec<RemoteBuild>> {
    let index = super::get(client, RELEASE_ROOT).await?.text().await?;

    let mut folders: Vec<(u32, u32)> = FOLDER_RE
        .captures_iter(&index)
        .filter_map(|c| Some((c[1].parse().ok()?, c[2].parse().ok()?)))
        .filter(|&(major, minor)| include_legacy || (major, minor) >= (2, 80))
        .collect();
    folders.sort_unstable_by(|a, b| b.cmp(a));
    folders.dedup();

    let cache_file = cache_dir.join("stable-folders.json");
    let mut cache: FolderCache = read_json(&cache_file);

    let stale: Vec<(u32, u32)> = folders
        .iter()
        .enumerate()
        .filter(|(i, f)| *i < ALWAYS_REFRESH || !cache.contains_key(&folder_name(**f)))
        .map(|(_, f)| *f)
        .collect();

    let fetched: Vec<(String, Result<Vec<RemoteBuild>>)> = stream::iter(stale)
        .map(|f| async move {
            let name = folder_name(f);
            let res = fetch_folder(client, &name).await;
            (name, res)
        })
        .buffer_unordered(6)
        .collect()
        .await;

    let mut first_error = None;
    for (name, res) in fetched {
        match res {
            Ok(builds) => {
                cache.insert(name, builds);
            }
            Err(e) => {
                log::warn!("failed to read {name}: {e}");
                first_error.get_or_insert(e);
            }
        }
    }
    // A failed refresh of every folder means we are offline; let the caller fall back.
    if cache.is_empty() {
        if let Some(e) = first_error {
            return Err(e);
        }
    }
    let _ = write_json(&cache_file, &cache);

    let mut out: Vec<RemoteBuild> = folders
        .iter()
        .filter_map(|f| cache.get(&folder_name(*f)))
        .flatten()
        .cloned()
        .collect();
    out.sort_by(crate::model::compare_builds);
    Ok(out)
}

fn folder_name((major, minor): (u32, u32)) -> String {
    format!("Blender{major}.{minor}")
}

async fn fetch_folder(client: &reqwest::Client, folder: &str) -> Result<Vec<RemoteBuild>> {
    let base = format!("{RELEASE_ROOT}{folder}/");
    let html = super::get(client, &base).await?.text().await?;
    Ok(parse_folder(&html, &base))
}

fn parse_folder(html: &str, base: &str) -> Vec<RemoteBuild> {
    let target = platform::current();
    let rows: Vec<(String, i64, u64)> = ROW_RE
        .captures_iter(html)
        .map(|c| {
            let date = chrono::NaiveDateTime::parse_from_str(&c[2], "%d-%b-%Y %H:%M")
                .map(|d| d.and_utc().timestamp())
                .unwrap_or(0);
            (c[1].to_string(), date, c[3].parse().unwrap_or(0))
        })
        .collect();

    let mut builds = Vec::new();
    for (file, date, size) in &rows {
        let Some(caps) = target.release_re.captures(file) else {
            continue;
        };
        let version = caps[1].to_string();
        let checksum = format!("blender-{version}.sha256");
        let sha256_url = rows
            .iter()
            .any(|(f, _, _)| f == &checksum)
            .then(|| format!("{base}{checksum}"));
        builds.push(RemoteBuild {
            id: format!("stable:{file}"),
            channel: Channel::Stable,
            lts: Version::parse(&version).is_lts(),
            version,
            release_cycle: "stable".into(),
            branch: String::new(),
            patch: None,
            hash: None,
            url: format!("{base}{file}"),
            file_name: file.clone(),
            file_size: *size,
            date: *date,
            sha256_url,
        });
    }
    builds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_listing_rows() {
        let html = r#"<a href="blender-5.1.2-windows-x64.zip">blender-5.1.2-windows-x64.zip</a>                      19-May-2026 07:10           414370237
<a href="blender-5.1.2-linux-x64.tar.xz">blender-5.1.2-linux-x64.tar.xz</a>                      19-May-2026 07:10           314370237
<a href="blender-5.1.2-macos-arm64.dmg">blender-5.1.2-macos-arm64.dmg</a>                      19-May-2026 07:10           214370237
<a href="blender-5.1.2.sha256">blender-5.1.2.sha256</a>                               19-May-2026 07:10                 777"#;
        let builds = parse_folder(html, "https://example/Blender5.1/");
        assert_eq!(builds.len(), 1);
        assert_eq!(builds[0].version, "5.1.2");
        assert!(builds[0].sha256_url.is_some());
        assert!(builds[0].date > 0);
    }
}
