//! Background check for new builds and the notifications it produces.

use crate::model::{Channel, InstalledBuild, RemoteBuild, Version};
use crate::notifications::AppNotification;
use crate::settings::{read_json, write_json};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Let the UI settle before the first network round-trip.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(6);

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Seen {
    channels: HashMap<String, Vec<String>>,
    last_check: i64,
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            let state = app.state::<AppState>();
            let interval = state.settings().check_interval_minutes;
            if interval > 0 {
                check(&app).await;
            }
            let wait = Duration::from_secs(u64::from(interval.max(5)) * 60);
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = state.watcher_wake.notified() => {}
            }
        }
    });
}

/// Refreshes every list and notifies about what is new. Returns the new notifications.
pub async fn check(app: &AppHandle) -> Vec<AppNotification> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let installed = state.installed();
    let seen_file = state.paths.data_dir.join("seen.json");
    let mut seen: Seen = read_json(&seen_file);
    let mut fresh = Vec::new();

    let wants_experimental = settings.notify_experimental
        || installed.iter().any(|b| b.channel == Channel::Experimental);

    for channel in Channel::REMOTE {
        if channel == Channel::Experimental && !wants_experimental {
            continue;
        }
        let list = state
            .cache
            .builds(&state.http, channel, true, settings.show_legacy_stable)
            .await;
        if list.error.is_some() || list.builds.is_empty() {
            continue;
        }
        let _ = app.emit("builds:updated", channel);

        let keys = keys_for(channel, &list.builds);
        let name = channel.dir_name().to_string();
        let Some(previous) = seen.channels.get(&name) else {
            // First run for this channel: remember everything silently.
            seen.channels.insert(name, keys.into_iter().collect());
            continue;
        };
        let previous: HashSet<&String> = previous.iter().collect();
        let new_keys: HashSet<&String> = keys.iter().filter(|k| !previous.contains(k)).collect();
        if !new_keys.is_empty() {
            fresh.extend(describe(
                channel,
                &list.builds,
                &new_keys,
                &installed,
                &settings,
            ));
        }
        let mut merged: Vec<String> = keys.iter().cloned().collect();
        if channel == Channel::Stable {
            // Stable keys never repeat; keep history so a re-listed version is not "new".
            merged.extend(previous.into_iter().cloned());
            merged.sort();
            merged.dedup();
        }
        seen.channels.insert(name, merged);
    }

    seen.last_check = crate::model::now();
    let _ = write_json(&seen_file, &seen);
    state.notifications.push(app, fresh.clone(), true);
    fresh
}

fn series(version: &str) -> String {
    let v = Version::parse(version);
    format!("{}.{}", v.major, v.minor)
}

/// Keys that identify "something new" in a channel.
fn keys_for(channel: Channel, builds: &[RemoteBuild]) -> HashSet<String> {
    let mut keys = HashSet::new();
    for b in builds {
        match channel {
            Channel::Stable => {
                keys.insert(format!("v:{}", b.version));
            }
            Channel::Daily => {
                keys.insert(format!("m:{}-{}", series(&b.version), b.release_cycle));
                keys.insert(format!("b:{}", b.id));
            }
            _ => {
                keys.insert(format!("p:{}", b.patch.clone().unwrap_or_default()));
                keys.insert(format!("b:{}", b.id));
            }
        }
    }
    keys
}

pub fn cycle_label(cycle: &str) -> &'static str {
    match cycle {
        "alpha" => "Alpha",
        "beta" => "Beta",
        "candidate" | "rc" => "Release Candidate",
        _ => "",
    }
}

fn title_for(b: &RemoteBuild) -> String {
    let label = cycle_label(&b.release_cycle);
    if label.is_empty() {
        format!("Blender {}", b.version)
    } else {
        format!("Blender {} {label}", series(&b.version))
    }
}

fn describe(
    channel: Channel,
    builds: &[RemoteBuild],
    new_keys: &HashSet<&String>,
    installed: &[InstalledBuild],
    settings: &crate::settings::Settings,
) -> Vec<AppNotification> {
    let mut out = Vec::new();
    let mut make = |kind: &str, title: String, body: String, b: &RemoteBuild| {
        let mut n = AppNotification::new(kind, title, body);
        n.channel = Some(channel);
        n.build_id = Some(b.id.clone());
        out.push(n);
    };
    let installed_update = |b: &RemoteBuild| {
        installed.iter().any(|i| {
            i.channel == channel
                && match channel {
                    Channel::Experimental => i.patch.is_some() && i.patch == b.patch,
                    _ => series(&i.version) == series(&b.version),
                }
                && i.date.is_some_and(|d| d < b.date)
        })
    };

    for b in builds {
        match channel {
            Channel::Stable => {
                if settings.notify_stable && new_keys.contains(&format!("v:{}", b.version)) {
                    let body = if b.lts {
                        "New LTS release is available".into()
                    } else {
                        "New stable release is available".into()
                    };
                    make("release", format!("Blender {}", b.version), body, b);
                }
            }
            Channel::Daily => {
                if !settings.notify_daily {
                    continue;
                }
                let milestone =
                    new_keys.contains(&format!("m:{}-{}", series(&b.version), b.release_cycle));
                let new_build = new_keys.contains(&format!("b:{}", b.id));
                if milestone {
                    make(
                        "build",
                        format!("{} is available", title_for(b)),
                        format!("New daily build from {}", b.branch),
                        b,
                    );
                } else if new_build && installed_update(b) {
                    make(
                        "update",
                        format!("Update for {}", title_for(b)),
                        "A newer daily build is ready to install".into(),
                        b,
                    );
                } else if new_build && settings.notify_daily_all {
                    make(
                        "build",
                        format!("New {} build", title_for(b)),
                        format!("{} · {}", b.branch, b.hash.clone().unwrap_or_default()),
                        b,
                    );
                }
            }
            _ => {
                let patch = b.patch.clone().unwrap_or_default();
                let new_build = new_keys.contains(&format!("b:{}", b.id));
                if new_build && installed_update(b) {
                    make(
                        "update",
                        format!("Update for experimental {patch}"),
                        title_for(b),
                        b,
                    );
                } else if settings.notify_experimental && new_keys.contains(&format!("p:{patch}")) {
                    make(
                        "build",
                        format!("New experimental build {patch}"),
                        title_for(b),
                        b,
                    );
                }
            }
        }
    }
    out
}
