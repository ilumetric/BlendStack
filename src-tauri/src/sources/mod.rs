pub mod builder;
pub mod news;
pub mod platform;
pub mod releases;

use crate::error::Result;
use crate::model::{now, Channel, RemoteBuild};
use crate::settings::{read_json, write_json};
use news::NewsItem;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// GET with a short timeout and retries for transient failures (DNS hiccups, resets).
pub async fn get(client: &reqwest::Client, url: &str) -> Result<reqwest::Response> {
    let mut attempt = 0u32;
    loop {
        let result = client
            .get(url)
            .timeout(Duration::from_secs(30))
            .send()
            .await;
        match result.and_then(|r| r.error_for_status()) {
            Ok(r) => return Ok(r),
            Err(e)
                if attempt < 2
                    && (e.is_connect()
                        || e.is_timeout()
                        || e.is_request()
                        || e.status().is_some_and(|s| s.is_server_error())) =>
            {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(800 * u64::from(attempt * attempt))).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Lists younger than this are served from cache unless a refresh is forced.
const BUILDS_TTL_SECS: i64 = 10 * 60;
const NEWS_TTL_SECS: i64 = 30 * 60;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct BuildList {
    pub builds: Vec<RemoteBuild>,
    pub fetched_at: i64,
    /// Set when the last refresh failed and cached data is shown instead.
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewsList {
    pub items: Vec<NewsItem>,
    pub fetched_at: i64,
    pub error: Option<String>,
}

/// In-memory + on-disk cache of remote lists. One async lock per list
/// guarantees concurrent callers share a single request.
pub struct RemoteCache {
    dir: PathBuf,
    builds: [tokio::sync::Mutex<Option<BuildList>>; 3],
    news: tokio::sync::Mutex<Option<NewsList>>,
}

impl RemoteCache {
    pub fn new(dir: PathBuf) -> Self {
        RemoteCache {
            dir,
            builds: Default::default(),
            news: Default::default(),
        }
    }

    fn builds_file(&self, channel: Channel) -> PathBuf {
        self.dir.join(format!("builds-{}.json", channel.dir_name()))
    }

    pub async fn builds(
        &self,
        client: &reqwest::Client,
        channel: Channel,
        force: bool,
        include_legacy: bool,
    ) -> BuildList {
        let mut slot = self.builds[channel.index()].lock().await;
        if slot.is_none() {
            let disk: BuildList = read_json(&self.builds_file(channel));
            if disk.fetched_at > 0 {
                *slot = Some(disk);
            }
        }
        if let Some(cached) = slot.as_ref() {
            if !force && now() - cached.fetched_at < BUILDS_TTL_SECS && cached.error.is_none() {
                return cached.clone();
            }
        }

        let fetched: Result<Vec<RemoteBuild>> = match channel {
            Channel::Stable => releases::fetch(client, &self.dir, include_legacy).await,
            _ => builder::fetch(client, channel).await,
        };
        let list = match fetched {
            Ok(builds) => {
                let list = BuildList {
                    builds,
                    fetched_at: now(),
                    error: None,
                };
                let _ = write_json(&self.builds_file(channel), &list);
                list
            }
            Err(e) => {
                log::warn!("refresh of {channel:?} failed: {e}");
                let mut stale = slot.clone().unwrap_or_default();
                stale.error = Some(e.to_string());
                stale
            }
        };
        *slot = Some(list.clone());
        list
    }

    /// Cached list without touching the network (used for instant first paint).
    /// Never waits for a refresh in flight: falls back to the disk copy instead.
    pub fn cached_builds(&self, channel: Channel) -> BuildList {
        if let Ok(slot) = self.builds[channel.index()].try_lock() {
            if let Some(l) = slot.as_ref() {
                return l.clone();
            }
        }
        read_json(&self.builds_file(channel))
    }

    pub fn cached_news(&self) -> NewsList {
        if let Ok(slot) = self.news.try_lock() {
            if let Some(l) = slot.as_ref() {
                return l.clone();
            }
        }
        read_json(&self.dir.join("news.json"))
    }

    pub async fn news(&self, client: &reqwest::Client, force: bool) -> NewsList {
        let file = self.dir.join("news.json");
        let mut slot = self.news.lock().await;
        if slot.is_none() {
            let disk: NewsList = read_json(&file);
            if disk.fetched_at > 0 {
                *slot = Some(disk);
            }
        }
        if let Some(cached) = slot.as_ref() {
            if !force && now() - cached.fetched_at < NEWS_TTL_SECS && cached.error.is_none() {
                return cached.clone();
            }
        }
        let list = match news::fetch_all(client).await {
            Ok(items) => {
                let list = NewsList {
                    items,
                    fetched_at: now(),
                    error: None,
                };
                let _ = write_json(&file, &list);
                list
            }
            Err(e) => {
                let mut stale = slot.clone().unwrap_or_default();
                stale.error = Some(e.to_string());
                stale
            }
        };
        *slot = Some(list.clone());
        list
    }
}

#[cfg(test)]
mod net_tests {
    /// `cargo test live_fetch -- --ignored --nocapture` hits the real servers.
    #[tokio::test]
    #[ignore]
    async fn live_fetch() {
        let client = reqwest::Client::builder()
            .user_agent("BlendStack-test")
            .build()
            .unwrap();
        for url in [
            "https://download.blender.org/release/",
            "https://builder.blender.org/download/daily/?format=json&v=1",
        ] {
            match client.get(url).send().await {
                Ok(r) => println!("{url} -> {} {:?}", r.status(), r.version()),
                Err(e) => {
                    let mut msg = format!("{e}");
                    let mut src = std::error::Error::source(&e);
                    while let Some(s) = src {
                        msg.push_str(&format!(" | {s}"));
                        src = s.source();
                    }
                    println!("{url} -> ERROR {msg}");
                }
            }
        }
    }
}
