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
    /// Notify about unread Blender Chat messages.
    pub notify_chat: bool,
    /// The chat tab has been used, so the chat may run in the background.
    pub chat_enabled: bool,

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
    /// .blend files opened from outside always ask which build to use,
    /// instead of opening in the version that saved them.
    pub ask_for_files: bool,

    pub default_build: Option<String>,
    pub favorites: Vec<String>,
    pub labels: HashMap<String, String>,
    pub build_args: HashMap<String, String>,
    pub last_launched: HashMap<String, i64>,
    pub external_builds: Vec<String>,
    /// Last window position and size.
    pub window: Option<WindowGeometry>,
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
            notify_chat: true,
            chat_enabled: false,
            launch_at_login: false,
            start_minimized: true,
            close_to_tray: false,
            minimize_on_launch: false,
            show_legacy_stable: false,
            keep_archives: false,
            replace_on_update: true,
            show_console: false,
            global_args: String::new(),
            ask_for_files: false,
            default_build: None,
            favorites: Vec::new(),
            labels: HashMap::new(),
            build_args: HashMap::new(),
            last_launched: HashMap::new(),
            external_builds: Vec::new(),
            window: None,
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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowGeometry {
    /// Logical (DPI-independent) units.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
}

/// Where the app keeps its files.
///
/// When the folder of the executable is writable (Windows and plain Linux
/// binaries), everything lives next to it: `data/` for settings, cache, logs and
/// the WebView profile, `Builds/` for Blender. Nothing is written elsewhere, so
/// the folder can be copied or moved as a whole. Otherwise (macOS app bundles,
/// AppImages, read-only installs) the standard per-user folders are used.
#[derive(Clone, Debug)]
pub struct AppPaths {
    /// The folder of the executable when running self-contained.
    pub root: Option<PathBuf>,
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve(identifier: &str) -> AppPaths {
        if let Some(root) = self_contained_root() {
            let data = root.join("data");
            return AppPaths {
                config_dir: data.clone(),
                cache_dir: data.join("cache"),
                data_dir: data,
                root: Some(root),
            };
        }
        let base = |dir: Option<PathBuf>| dir.unwrap_or_else(std::env::temp_dir).join(identifier);
        AppPaths {
            root: None,
            config_dir: base(dirs::config_dir()),
            cache_dir: base(dirs::cache_dir()),
            data_dir: base(dirs::data_local_dir()),
        }
    }

    pub fn create_dirs(&self) -> Result<()> {
        for dir in [&self.config_dir, &self.cache_dir, &self.data_dir] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    /// WebView profile (local storage, caches) kept with the app data.
    pub fn webview_dir(&self) -> PathBuf {
        self.data_dir.join("webview")
    }

    pub fn default_library(&self) -> PathBuf {
        if let Some(root) = &self.root {
            return root.join("Builds");
        }
        // Outside AppData: some security policies stop Blender's side-by-side
        // runtime (blender.crt) from loading anywhere under it.
        if cfg!(windows) {
            if let Some(home) = dirs::home_dir() {
                return home.join("BlendStack");
            }
        }
        self.data_dir.join("Library")
    }

    pub fn library(&self, settings: &Settings) -> PathBuf {
        match settings.library_path.as_deref().map(str::trim) {
            Some(p) if !p.is_empty() => {
                let path = PathBuf::from(p);
                match &self.root {
                    Some(root) if path.is_relative() => root.join(path),
                    _ => path,
                }
            }
            _ => self.default_library(),
        }
    }

    /// The value to store for a chosen library folder: `None` for the default,
    /// a relative path inside the app folder (so it survives moving the folder).
    pub fn library_setting(&self, chosen: &Path) -> Option<String> {
        if chosen == self.default_library() {
            return None;
        }
        if let Some(rel) = self.root.as_ref().and_then(|r| chosen.strip_prefix(r).ok()) {
            if !rel.as_os_str().is_empty() {
                return Some(rel.to_string_lossy().into_owned());
            }
        }
        Some(chosen.to_string_lossy().into_owned())
    }
}

/// The executable's folder, if the app can keep its files there.
fn self_contained_root() -> Option<PathBuf> {
    if cfg!(target_os = "macos") || std::env::var_os("APPIMAGE").is_some() {
        return None;
    }
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let data = dir.join("data");
    std::fs::create_dir_all(&data).ok()?;
    let probe = data.join(".write-test");
    std::fs::write(&probe, b"").ok()?;
    let _ = std::fs::remove_file(&probe);
    Some(dir)
}

/// Reads JSON, falling back to the default value when the file is missing or corrupt.
pub fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| {
            // Tolerate a UTF-8 BOM (files edited in Notepad and friends).
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
            serde_json::from_slice(bytes).ok()
        })
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
