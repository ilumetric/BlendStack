//! IPC commands exposed to the UI.

use crate::download::{self, DownloadTask};
use crate::error::{msg, Result};
use crate::library;
use crate::model::{now, Channel, InstalledBuild, RemoteBuild, Version};
use crate::notifications::AppNotification;
use crate::recent::{self, RecentFile};
use crate::settings::Settings;
use crate::sources::{BuildList, NewsList};
use crate::state::{emit_library_changed, emit_settings_changed, AppState};
use crate::{watcher, window};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

const REPO: &str = "ilumetric/BlendStack";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub os: &'static str,
    pub arch: &'static str,
    pub app_version: &'static str,
    pub translucent: bool,
    pub default_library: String,
    pub library: String,
    pub repository: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub settings: Settings,
    pub system: SystemInfo,
    pub installed: Vec<InstalledBuild>,
    pub downloads: Vec<DownloadTask>,
    pub notifications: Vec<AppNotification>,
}

fn system_info(app: &AppHandle, state: &AppState) -> SystemInfo {
    let settings = state.settings();
    SystemInfo {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        app_version: env!("CARGO_PKG_VERSION"),
        translucent: window::is_translucent(app, &settings),
        default_library: state.paths.default_library().to_string_lossy().into_owned(),
        library: state
            .paths
            .library(&settings)
            .to_string_lossy()
            .into_owned(),
        repository: format!("https://github.com/{REPO}"),
    }
}

#[tauri::command]
pub fn get_bootstrap(app: AppHandle, state: State<'_, AppState>) -> Bootstrap {
    Bootstrap {
        settings: state.settings(),
        system: system_info(&app, &state),
        installed: state.installed(),
        downloads: state.downloads.list(),
        notifications: state.notifications.list(),
    }
}

/// Called once the UI has painted, so the window never flashes empty.
#[tauri::command]
pub fn app_ready(app: AppHandle, state: State<'_, AppState>) {
    if !state.started_hidden {
        crate::tray::show_main(&app);
    }
}

#[tauri::command]
pub fn get_system_info(app: AppHandle, state: State<'_, AppState>) -> SystemInfo {
    system_info(&app, &state)
}

/// Merges a partial settings object into the current settings.
#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: serde_json::Value,
) -> Result<Settings> {
    let before = state.settings();
    let mut merged = serde_json::to_value(&before)?;
    if let (Some(target), Some(patch)) = (merged.as_object_mut(), patch.as_object()) {
        for (k, v) in patch {
            target.insert(k.clone(), v.clone());
        }
    }
    let next: Settings = serde_json::from_value(merged)?;
    let saved = state.update_settings(|s| *s = next)?;

    if before.launch_at_login != saved.launch_at_login {
        let autolaunch = app.autolaunch();
        let res = if saved.launch_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if let Err(e) = res {
            log::warn!("autostart change failed: {e}");
        }
    }
    if before.theme != saved.theme || before.translucency != saved.translucency {
        window::apply_style(&app, &saved);
    }
    if before.check_interval_minutes != saved.check_interval_minutes {
        state.watcher_wake.notify_one();
    }
    if before.library_path != saved.library_path
        || before.external_builds != saved.external_builds
        || before.favorites != saved.favorites
        || before.labels != saved.labels
        || before.default_build != saved.default_build
    {
        emit_library_changed(&app);
    }
    emit_settings_changed(&app, &saved);
    Ok(saved)
}

#[tauri::command]
pub fn list_installed(state: State<'_, AppState>) -> Vec<InstalledBuild> {
    state.installed()
}

#[tauri::command]
pub fn get_cached_builds(state: State<'_, AppState>, channel: Channel) -> BuildList {
    state.cache.cached_builds(channel)
}

#[tauri::command]
pub async fn get_builds(
    state: State<'_, AppState>,
    channel: Channel,
    force: bool,
) -> Result<BuildList> {
    let legacy = state.settings().show_legacy_stable;
    Ok(state
        .cache
        .builds(&state.http, channel, force, legacy)
        .await)
}

#[tauri::command]
pub fn get_cached_news(state: State<'_, AppState>) -> NewsList {
    state.cache.cached_news()
}

#[tauri::command]
pub async fn get_news(state: State<'_, AppState>, force: bool) -> Result<NewsList> {
    Ok(state.cache.news(&state.http, force).await)
}

#[tauri::command]
pub fn start_download(
    app: AppHandle,
    build: RemoteBuild,
    replaces: Option<String>,
) -> DownloadTask {
    download::start(&app, build, replaces)
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, id: String) {
    state.downloads.cancel(&id);
}

#[tauri::command]
pub fn clear_downloads(state: State<'_, AppState>) -> Vec<DownloadTask> {
    state.downloads.clear_finished();
    state.downloads.list()
}

#[tauri::command]
pub fn list_downloads(state: State<'_, AppState>) -> Vec<DownloadTask> {
    state.downloads.list()
}

pub fn launch_by_id(app: &AppHandle, id: &str, file: Option<&str>) -> Result<()> {
    let state = app.state::<AppState>();
    let build = state
        .find_installed(id)
        .ok_or_else(|| msg("build not found"))?;
    let settings = state.settings();
    crate::launch::launch(&build, &settings, file)?;
    let saved = state.update_settings(|s| {
        s.last_launched.insert(build.id.clone(), now());
    })?;
    emit_settings_changed(app, &saved);
    crate::tray::refresh(app);
    if settings.minimize_on_launch {
        if let Some(w) = app.get_webview_window("main") {
            let _ = if settings.close_to_tray {
                w.hide()
            } else {
                w.minimize()
            };
        }
    }
    Ok(())
}

#[tauri::command]
pub fn launch_build(app: AppHandle, id: String, file: Option<String>) -> Result<()> {
    launch_by_id(&app, &id, file.as_deref())
}

#[tauri::command]
pub async fn delete_build(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let build = state
        .find_installed(&id)
        .ok_or_else(|| msg("build not found"))?;
    if build.external {
        let path = build.path.clone();
        state.update_settings(|s| {
            s.external_builds
                .retain(|p| Path::new(p) != Path::new(&path))
        })?;
    } else {
        let lib = state.library();
        let b = build.clone();
        tauri::async_runtime::spawn_blocking(move || library::remove(&lib, &b))
            .await
            .map_err(|e| msg(e.to_string()))??;
    }
    let saved = state.update_settings(|s| s.forget_build(&id))?;
    emit_settings_changed(&app, &saved);
    emit_library_changed(&app);
    Ok(())
}

/// Switches the library folder and moves the managed builds there.
#[tauri::command]
pub async fn move_library(
    app: AppHandle,
    state: State<'_, AppState>,
    target: Option<String>,
) -> Result<usize> {
    if state.downloads.has_active() {
        return Err(msg("wait for the downloads to finish first"));
    }
    let from = state.library();
    let to = target
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| state.paths.default_library());
    let moved = if from == to {
        0
    } else {
        let (f, t) = (from.clone(), to.clone());
        tauri::async_runtime::spawn_blocking(move || library::move_all(&f, &t))
            .await
            .map_err(|e| msg(e.to_string()))??
    };
    // Settings keep build ids (`<channel>/<folder>`), which do not change when moving.
    let saved = state.update_settings(|s| s.library_path = target.clone())?;
    emit_settings_changed(&app, &saved);
    emit_library_changed(&app);
    Ok(moved)
}

/// Adds an existing Blender installation (folder or executable) to the library.
#[tauri::command]
pub fn import_build(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<InstalledBuild> {
    let mut dir = PathBuf::from(&path);
    if dir.is_file() {
        dir = dir.parent().map(Path::to_path_buf).unwrap_or(dir);
    }
    if cfg!(target_os = "macos") && dir.extension().is_some_and(|e| e == "app") {
        dir = dir.parent().map(Path::to_path_buf).unwrap_or(dir);
    }
    if library::executable(&dir).is_none() {
        return Err(msg("no Blender executable found in this folder"));
    }
    let dir_str = dir.to_string_lossy().into_owned();
    let saved = state.update_settings(|s| {
        if !s.external_builds.iter().any(|p| Path::new(p) == dir) {
            s.external_builds.push(dir_str.clone());
        }
    })?;
    emit_settings_changed(&app, &saved);
    emit_library_changed(&app);
    state
        .find_installed(&format!("external:{dir_str}"))
        .ok_or_else(|| msg("could not read the imported build"))
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> Result<()> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(msg("path does not exist"));
    }
    if p.is_dir() {
        app.opener()
            .open_path(path, None::<&str>)
            .map_err(|e| msg(e.to_string()))
    } else {
        app.opener()
            .reveal_item_in_dir(p)
            .map_err(|e| msg(e.to_string()))
    }
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(msg("only web links can be opened"));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| msg(e.to_string()))
}

#[tauri::command]
pub async fn get_recent_files() -> Result<Vec<RecentFile>> {
    tauri::async_runtime::spawn_blocking(|| recent::list(40))
        .await
        .map_err(|e| msg(e.to_string()))
}

#[tauri::command]
pub async fn get_thumbnail(state: State<'_, AppState>, path: String) -> Result<Option<String>> {
    let cache = state.paths.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        recent::thumbnail(Path::new(&path), &cache).ok().flatten()
    })
    .await
    .map_err(|e| msg(e.to_string()))
}

#[tauri::command]
pub fn get_notifications(state: State<'_, AppState>) -> Vec<AppNotification> {
    state.notifications.list()
}

#[tauri::command]
pub fn mark_notifications_read(app: AppHandle, state: State<'_, AppState>) {
    state.notifications.mark_all_read(&app);
}

#[tauri::command]
pub fn clear_notifications(app: AppHandle, state: State<'_, AppState>) {
    state.notifications.clear(&app);
}

#[tauri::command]
pub async fn check_now(app: AppHandle) -> Vec<AppNotification> {
    watcher::check(&app).await
}

#[tauri::command]
pub fn take_pending_files(state: State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.pending_files.lock().unwrap())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    pub version: String,
    pub url: String,
    pub notes: String,
}

/// Looks for a newer Blend Stack release on GitHub.
#[tauri::command]
pub async fn check_app_update(state: State<'_, AppState>) -> Result<Option<AppUpdate>> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
    }
    let resp = state
        .http
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let release: Release = resp.error_for_status()?.json().await?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let latest = release.tag_name.trim_start_matches('v');
    if Version::parse(latest) > Version::parse(env!("CARGO_PKG_VERSION")) {
        Ok(Some(AppUpdate {
            version: latest.to_string(),
            url: release.html_url,
            notes: release.body.unwrap_or_default(),
        }))
    } else {
        Ok(None)
    }
}
