use crate::download::Downloads;
use crate::error::Result;
use crate::library;
use crate::model::InstalledBuild;
use crate::notifications::Notifications;
use crate::settings::{write_json, AppPaths, Settings};
use crate::sources::RemoteCache;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
use tauri::{AppHandle, Emitter};

pub struct AppState {
    pub paths: AppPaths,
    settings: RwLock<Settings>,
    pub http: reqwest::Client,
    pub cache: RemoteCache,
    pub downloads: Downloads,
    pub notifications: Notifications,
    /// Wakes the background checker early (settings changed, manual check).
    pub watcher_wake: tokio::sync::Notify,
    /// `.blend` files passed on the command line, waiting for the UI to pick them up.
    pub pending_files: Mutex<Vec<String>>,
    pub started_hidden: bool,
}

impl AppState {
    pub fn new(paths: AppPaths, settings: Settings, started_hidden: bool) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "BlendStack/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/ilumetric/BlendStack)"
            ))
            .connect_timeout(std::time::Duration::from_secs(15))
            .read_timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(AppState {
            cache: RemoteCache::new(paths.cache_dir.clone()),
            notifications: Notifications::load(paths.data_dir.join("notifications.json")),
            paths,
            settings: RwLock::new(settings),
            http,
            downloads: Downloads::default(),
            watcher_wake: tokio::sync::Notify::new(),
            pending_files: Mutex::new(Vec::new()),
            started_hidden,
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    /// Applies a change and persists it.
    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut guard = self.settings.write().unwrap();
        f(&mut guard);
        write_json(&self.paths.settings_file(), &*guard)?;
        Ok(guard.clone())
    }

    pub fn library(&self) -> PathBuf {
        self.paths.library(&self.settings.read().unwrap())
    }

    pub fn installed(&self) -> Vec<InstalledBuild> {
        let settings = self.settings();
        library::scan(&self.paths.library(&settings), &settings.external_builds)
    }

    pub fn find_installed(&self, id: &str) -> Option<InstalledBuild> {
        self.installed().into_iter().find(|b| b.id == id)
    }
}

pub fn emit_library_changed(app: &AppHandle) {
    let _ = app.emit("library:changed", ());
    crate::tray::refresh(app);
}

pub fn emit_settings_changed(app: &AppHandle, settings: &Settings) {
    let _ = app.emit("settings:changed", settings);
}
