use crate::error::Result;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Where managed builds are installed. `None` means the default location.
    pub library_path: Option<String>,
    /// `system`, `light` or `dark`.
    pub theme: String,
    /// UI language code, empty for automatic.
    pub language: String,
    pub translucency: bool,

    /// Minutes between background checks; 0 disables them.
    pub check_interval_minutes: u32,
    pub notify_stable: bool,
    pub notify_daily: bool,
    /// Notify about every new daily build, not only new versions and updates of installed ones.
    pub notify_daily_all: bool,
    pub notify_experimental: bool,

    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    /// Minimize the launcher once Blender has started.
    pub minimize_on_launch: bool,

    pub show_legacy_stable: bool,
    pub keep_archives: bool,
    /// Remove the previous daily/experimental build after installing its update.
    pub replace_on_update: bool,
    /// Windows: start Blender with its console window.
    pub show_console: bool,
    pub global_args: String,

    pub default_build: Option<String>,
    pub favorites: Vec<String>,
    pub labels: HashMap<String, String>,
    pub build_args: HashMap<String, String>,
    pub last_launched: HashMap<String, i64>,
    pub external_builds: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            library_path: None,
            theme: "system".into(),
            language: String::new(),
            translucency: true,
            check_interval_minutes: 60,
            notify_stable: true,
            notify_daily: true,
            notify_daily_all: false,
            notify_experimental: false,
            launch_at_login: false,
            start_minimized: true,
            close_to_tray: false,
            minimize_on_launch: false,
            show_legacy_stable: false,
            keep_archives: false,
            replace_on_update: true,
            show_console: false,
            global_args: String::new(),
            default_build: None,
            favorites: Vec::new(),
            labels: HashMap::new(),
            build_args: HashMap::new(),
            last_launched: HashMap::new(),
            external_builds: Vec::new(),
        }
    }
}

impl Settings {
    /// Moves per-build preferences from one build id to another (used after an update).
    pub fn transfer_build(&mut self, from: &str, to: &str) {
        if self.default_build.as_deref() == Some(from) {
            self.default_build = Some(to.to_string());
        }
        if let Some(pos) = self.favorites.iter().position(|f| f == from) {
            self.favorites[pos] = to.to_string();
        }
        if let Some(v) = self.labels.remove(from) {
            self.labels.insert(to.to_string(), v);
        }
        if let Some(v) = self.build_args.remove(from) {
            self.build_args.insert(to.to_string(), v);
        }
        if let Some(v) = self.last_launched.remove(from) {
            self.last_launched.insert(to.to_string(), v);
        }
    }

    pub fn forget_build(&mut self, id: &str) {
        if self.default_build.as_deref() == Some(id) {
            self.default_build = None;
        }
        self.favorites.retain(|f| f != id);
        self.labels.remove(id);
        self.build_args.remove(id);
        self.last_launched.remove(id);
    }
}

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppPaths {
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    /// On Windows builds live in the user folder: some security policies stop
    /// Blender's side-by-side runtime from loading anywhere under AppData.
    pub fn default_library(&self) -> PathBuf {
        if cfg!(windows) {
            if let Some(home) = dirs::home_dir() {
                return home.join("BlendStack");
            }
        }
        self.data_dir.join("Library")
    }

    pub fn library(&self, settings: &Settings) -> PathBuf {
        match &settings.library_path {
            Some(p) if !p.trim().is_empty() => PathBuf::from(p),
            _ => self.default_library(),
        }
    }
}

/// Reads JSON, falling back to the default value when the file is missing or corrupt.
pub fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes JSON atomically so a crash never leaves a half-written file behind.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
