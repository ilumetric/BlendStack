use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Daily,
    Experimental,
    Custom,
}

impl Channel {
    pub const REMOTE: [Channel; 3] = [Channel::Stable, Channel::Daily, Channel::Experimental];

    pub fn dir_name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Daily => "daily",
            Channel::Experimental => "experimental",
            Channel::Custom => "custom",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Channel::Stable => 0,
            Channel::Daily => 1,
            Channel::Experimental => 2,
            Channel::Custom => 3,
        }
    }
}

/// A build that can be downloaded from an official Blender server.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBuild {
    /// Stable unique id: `<channel>:<file name>`.
    pub id: String,
    pub channel: Channel,
    pub version: String,
    /// `stable`, `candidate`, `beta` or `alpha`.
    pub release_cycle: String,
    pub branch: String,
    pub patch: Option<String>,
    pub hash: Option<String>,
    pub url: String,
    pub file_name: String,
    pub file_size: u64,
    /// Unix seconds.
    pub date: i64,
    pub sha256_url: Option<String>,
    pub lts: bool,
}

/// A build that lives on disk and can be launched.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstalledBuild {
    /// `<channel>/<folder>` for managed builds, `external:<path>` for imported ones.
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub executable: String,
    pub channel: Channel,
    pub version: String,
    pub release_cycle: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub patch: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    /// Build date (unix seconds) when known.
    #[serde(default)]
    pub date: Option<i64>,
    #[serde(default)]
    pub installed_at: i64,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub archive_size: Option<u64>,
    #[serde(default)]
    pub lts: bool,
    #[serde(default)]
    pub external: bool,
}

/// Blender LTS series. Kept as data so new series are a one-line change.
pub const LTS_SERIES: &[(u32, u32)] = &[(2, 83), (2, 93), (3, 3), (3, 6), (4, 2), (4, 5), (5, 2)];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Letter suffix of old releases such as `2.79b`.
    pub suffix: u8,
}

impl Version {
    pub fn parse(s: &str) -> Version {
        fn take(p: Option<&str>, suffix: &mut u8) -> u32 {
            let p = p.unwrap_or("");
            let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Some(c) = p.chars().find(|c| c.is_ascii_alphabetic()) {
                *suffix = c as u8;
            }
            digits.parse().unwrap_or(0)
        }
        let mut v = Version::default();
        let mut parts = s.trim().split('.');
        let mut sfx = 0u8;
        v.major = take(parts.next(), &mut sfx);
        v.minor = take(parts.next(), &mut sfx);
        v.patch = take(parts.next(), &mut sfx);
        v.suffix = sfx;
        v
    }

    pub fn is_lts(&self) -> bool {
        LTS_SERIES.contains(&(self.major, self.minor))
    }
}

pub fn cycle_rank(cycle: &str) -> u8 {
    match cycle {
        "stable" => 4,
        "candidate" | "rc" => 3,
        "beta" => 2,
        "alpha" => 1,
        _ => 0,
    }
}

/// Newest first: version, then maturity, then date.
pub fn compare_builds(a: &RemoteBuild, b: &RemoteBuild) -> Ordering {
    Version::parse(&b.version)
        .cmp(&Version::parse(&a.version))
        .then(cycle_rank(&b.release_cycle).cmp(&cycle_rank(&a.release_cycle)))
        .then(b.date.cmp(&a.date))
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(
            Version::parse("5.1.2"),
            Version {
                major: 5,
                minor: 1,
                patch: 2,
                suffix: 0
            }
        );
        assert_eq!(Version::parse("2.79b").suffix, b'b');
        assert!(Version::parse("2.80") > Version::parse("2.79b"));
        assert!(Version::parse("4.10.0") > Version::parse("4.9.3"));
        assert!(Version::parse("4.2").is_lts());
    }
}
