//! In-app notification center, mirrored to the OS notification area.

use crate::model::{now, Channel};
use crate::settings::{read_json, write_json};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

const MAX_STORED: usize = 100;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppNotification {
    pub id: String,
    /// `release`, `build`, `update` or `app`.
    pub kind: String,
    pub title: String,
    pub body: String,
    pub date: i64,
    pub read: bool,
    pub channel: Option<Channel>,
    pub build_id: Option<String>,
}

impl AppNotification {
    pub fn new(kind: &str, title: String, body: String) -> Self {
        AppNotification {
            id: format!("{}-{}", now(), rand_suffix()),
            kind: kind.into(),
            title,
            body,
            date: now(),
            read: false,
            channel: None,
            build_id: None,
        }
    }
}

fn rand_suffix() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(now() as u64);
    h.finish() as u32
}

pub struct Notifications {
    file: PathBuf,
    items: Mutex<Vec<AppNotification>>,
}

impl Notifications {
    pub fn load(file: PathBuf) -> Self {
        let items: Vec<AppNotification> = read_json(&file);
        Notifications {
            file,
            items: Mutex::new(items),
        }
    }

    pub fn list(&self) -> Vec<AppNotification> {
        self.items.lock().unwrap().clone()
    }

    fn save(&self, items: &[AppNotification]) {
        if let Err(e) = write_json(&self.file, &items) {
            log::warn!("could not save notifications: {e}");
        }
    }

    pub fn push(&self, app: &AppHandle, new: Vec<AppNotification>, show_os: bool) {
        if new.is_empty() {
            return;
        }
        if show_os {
            let (title, body) = if new.len() == 1 {
                (new[0].title.clone(), new[0].body.clone())
            } else {
                let lines: Vec<String> = new.iter().take(4).map(|n| n.title.clone()).collect();
                (
                    format!("{} new Blender builds", new.len()),
                    lines.join("\n"),
                )
            };
            show_system(app, &title, &body);
        }
        let mut items = self.items.lock().unwrap();
        for n in new.into_iter().rev() {
            items.insert(0, n);
        }
        items.truncate(MAX_STORED);
        self.save(&items);
        let _ = app.emit("notifications:changed", ());
    }

    pub fn mark_all_read(&self, app: &AppHandle) {
        #[cfg(windows)]
        crate::toast::clear();
        let mut items = self.items.lock().unwrap();
        items.iter_mut().for_each(|n| n.read = true);
        self.save(&items);
        let _ = app.emit("notifications:changed", ());
    }

    pub fn clear(&self, app: &AppHandle) {
        let mut items = self.items.lock().unwrap();
        items.clear();
        self.save(&items);
        let _ = app.emit("notifications:changed", ());
    }
}

#[cfg(windows)]
fn show_system(_app: &AppHandle, title: &str, body: &str) {
    crate::toast::show(title, body);
}

#[cfg(not(windows))]
fn show_system(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("os notification failed: {e}");
    }
}
