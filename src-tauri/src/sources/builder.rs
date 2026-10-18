//! Daily and experimental (pull request) builds from builder.blender.org.

use super::platform;
use crate::error::Result;
use crate::model::{Channel, RemoteBuild, Version};
use serde::Deserialize;
use std::collections::HashMap;

const DAILY_URL: &str = "https://builder.blender.org/download/daily/?format=json&v=1";
const PATCH_URL: &str = "https://builder.blender.org/download/patch/?format=json&v=1";

/// Builds older than this are leftovers of retired branches.
const MAX_AGE_DAILY_DAYS: i64 = 365;
const MAX_AGE_PATCH_DAYS: i64 = 120;

#[derive(Deserialize)]
struct Entry {
    url: String,
    version: String,
    #[serde(default)]
    branch: String,
    #[serde(default)]
    patch: Option<String>,
    #[serde(default)]
    hash: Option<String>,
    platform: String,
    architecture: String,
    file_mtime: i64,
    file_name: String,
    #[serde(default)]
    file_size: u64,
    file_extension: String,
    #[serde(default)]
    release_cycle: String,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

pub async fn fetch(client: &reqwest::Client, channel: Channel) -> Result<Vec<RemoteBuild>> {
    let url = match channel {
        Channel::Experimental => PATCH_URL,
        _ => DAILY_URL,
    };
    let entries: Vec<Entry> = super::get(client, url).await?.json().await?;
    Ok(convert(entries, channel))
}

fn convert(entries: Vec<Entry>, channel: Channel) -> Vec<RemoteBuild> {
    let target = platform::current();
    let max_age = match channel {
        Channel::Experimental => MAX_AGE_PATCH_DAYS,
        _ => MAX_AGE_DAILY_DAYS,
    } * 86_400;
    let cutoff = crate::model::now() - max_age;

    let mut checksums: HashMap<String, String> = HashMap::new();
    let mut builds = Vec::new();
    for e in entries {
        if e.platform != target.builder_os || e.architecture != target.builder_arch {
            continue;
        }
        if e.file_extension == "sha256" {
            checksums.insert(e.file_name.trim_end_matches(".sha256").to_string(), e.url);
            continue;
        }
        if e.file_extension != target.builder_ext || e.file_mtime < cutoff {
            continue;
        }
        let patch = non_empty(e.patch);
        if channel == Channel::Experimental && patch.is_none() {
            continue;
        }
        builds.push(RemoteBuild {
            id: format!("{}:{}", channel.dir_name(), e.file_name),
            channel,
            lts: Version::parse(&e.version).is_lts(),
            version: e.version,
            release_cycle: if e.release_cycle.is_empty() {
                "alpha".into()
            } else {
                e.release_cycle
            },
            branch: e.branch,
            patch,
            hash: non_empty(e.hash),
            url: e.url,
            file_name: e.file_name,
            file_size: e.file_size,
            date: e.file_mtime,
            sha256_url: None,
        });
    }
    for b in &mut builds {
        b.sha256_url = checksums.get(&b.file_name).cloned();
    }

    // Keep only the newest build per branch (daily) or per pull request (experimental).
    let mut newest: HashMap<String, RemoteBuild> = HashMap::new();
    for b in builds {
        let key = match channel {
            Channel::Experimental => b.patch.clone().unwrap_or_default(),
            _ => b.branch.clone(),
        };
        match newest.get(&key) {
            Some(existing) if existing.date >= b.date => {}
            _ => {
                newest.insert(key, b);
            }
        }
    }
    let mut out: Vec<RemoteBuild> = newest.into_values().collect();
    out.sort_by(crate::model::compare_builds);
    out
}
