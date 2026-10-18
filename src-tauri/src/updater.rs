//! Updates of Blend Stack itself from GitHub releases.
//!
//! The self-contained Windows build updates in place: the new executable is
//! downloaded, verified against the published SHA-256, swapped in next to the
//! running one and started. Other builds link to the release page.

use crate::error::{msg, Result};
use crate::model::Version;
use crate::state::AppState;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

pub const REPO: &str = "ilumetric/BlendStack";
/// Release asset with the self-contained Windows build.
const WINDOWS_ASSET_SUFFIX: &str = "-windows-x64.zip";
const CHECKSUMS_ASSET: &str = "SHA256SUMS.txt";
/// Passed to the new process so it waits for the old one to quit first.
pub const WAIT_FLAG: &str = "--wait-pid";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    pub version: String,
    pub url: String,
    pub notes: String,
    /// The update can be installed from inside the app.
    pub can_install: bool,
    pub size: u64,
}

#[derive(Deserialize, Clone)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    /// `sha256:<hex>`, provided by GitHub for newer uploads.
    #[serde(default)]
    digest: Option<String>,
}

fn self_contained_windows(state: &AppState) -> bool {
    cfg!(all(windows, target_arch = "x86_64")) && state.paths.root.is_some()
}

async fn latest_release(client: &reqwest::Client) -> Result<Option<Release>> {
    let resp = client
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let release: Release = resp.error_for_status()?.json().await?;
    Ok((!release.draft && !release.prerelease).then_some(release))
}

fn windows_asset(release: &Release) -> Option<&Asset> {
    release
        .assets
        .iter()
        .find(|a| a.name.ends_with(WINDOWS_ASSET_SUFFIX))
}

/// A newer release than the running version, if any.
pub async fn available(state: &AppState) -> Result<Option<AppUpdate>> {
    let Some(release) = latest_release(&state.http).await? else {
        return Ok(None);
    };
    let latest = release.tag_name.trim_start_matches('v').to_string();
    if Version::parse(&latest) <= Version::parse(env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let asset = windows_asset(&release);
    Ok(Some(AppUpdate {
        version: latest,
        url: release.html_url.clone(),
        notes: release.body.clone().unwrap_or_default(),
        can_install: self_contained_windows(state) && asset.is_some(),
        size: asset.map(|a| a.size).unwrap_or(0),
    }))
}

#[tauri::command]
pub async fn check_app_update(state: State<'_, AppState>) -> Result<Option<AppUpdate>> {
    available(&state).await
}

/// Downloads, verifies and swaps in the new executable, then restarts.
#[tauri::command]
pub async fn install_app_update(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    if !self_contained_windows(&state) {
        return Err(msg(
            "updates are installed from the release page on this platform",
        ));
    }
    let release = latest_release(&state.http)
        .await?
        .ok_or_else(|| msg("no release found"))?;
    let asset = windows_asset(&release)
        .cloned()
        .ok_or_else(|| msg("the release has no Windows build"))?;
    let expected = match asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
    {
        Some(h) => h.to_ascii_lowercase(),
        None => published_checksum(&state.http, &release, &asset.name)
            .await?
            .ok_or_else(|| msg("the release has no checksum for the Windows build"))?,
    };

    let dir = state.paths.cache_dir.join("update");
    tokio::fs::create_dir_all(&dir).await?;
    let archive = dir.join(&asset.name);
    let digest = download(&app, &state.http, &asset, &archive).await?;
    if !digest.eq_ignore_ascii_case(&expected) {
        let _ = tokio::fs::remove_file(&archive).await;
        return Err(msg("checksum mismatch — the update was not installed"));
    }

    let exe = std::env::current_exe()?;
    let staged = exe.with_extension("exe.new");
    {
        let (archive, staged) = (archive.clone(), staged.clone());
        tauri::async_runtime::spawn_blocking(move || extract_exe(&archive, &staged))
            .await
            .map_err(|e| msg(e.to_string()))??;
    }
    // A running executable can be renamed but not overwritten.
    let previous = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&previous);
    std::fs::rename(&exe, &previous)?;
    if let Err(e) = std::fs::rename(&staged, &exe) {
        let _ = std::fs::rename(&previous, &exe);
        return Err(e.into());
    }
    let _ = std::fs::remove_file(&archive);

    std::process::Command::new(&exe)
        .arg(WAIT_FLAG)
        .arg(std::process::id().to_string())
        .spawn()
        .map_err(|e| msg(format!("updated, but could not restart: {e}")))?;
    app.exit(0);
    Ok(())
}

async fn published_checksum(
    client: &reqwest::Client,
    release: &Release,
    name: &str,
) -> Result<Option<String>> {
    let Some(sums) = release.assets.iter().find(|a| a.name == CHECKSUMS_ASSET) else {
        return Ok(None);
    };
    let text = client
        .get(&sums.browser_download_url)
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    }))
}

async fn download(
    app: &AppHandle,
    client: &reqwest::Client,
    asset: &Asset,
    to: &Path,
) -> Result<String> {
    let resp = client
        .get(&asset.browser_download_url)
        .send()
        .await?
        .error_for_status()?;
    let total = resp.content_length().unwrap_or(asset.size).max(1);
    let mut file = tokio::fs::File::create(to).await?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut stream = resp.bytes_stream();
    let mut last = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        received += chunk.len() as u64;
        let pct = received * 100 / total;
        if pct != last {
            last = pct;
            let _ = app.emit("app-update:progress", received as f64 / total as f64);
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    Ok(hex::encode(hasher.finalize()))
}

fn extract_exe(archive: &Path, to: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(archive)?))?;
    let index = (0..zip.len())
        .find(|&i| {
            zip.by_index_raw(i)
                .map(|f| f.name().to_ascii_lowercase().ends_with(".exe"))
                .unwrap_or(false)
        })
        .ok_or_else(|| msg("the update archive has no executable"))?;
    let mut entry = zip.by_index(index)?;
    let mut out = std::fs::File::create(to)?;
    std::io::copy(&mut entry, &mut out)?;
    Ok(())
}

/// Called first thing on startup: waits for the process being replaced and
/// removes the executable it left behind.
pub fn cleanup_previous() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pid) = args
        .iter()
        .position(|a| a == WAIT_FLAG)
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse::<u32>().ok())
    {
        wait_for_exit(pid);
    }
    if let Ok(exe) = std::env::current_exe() {
        for leftover in [exe.with_extension("exe.old"), exe.with_extension("exe.new")] {
            remove_with_retry(&leftover);
        }
    }
}

fn remove_with_retry(path: &PathBuf) {
    for _ in 0..10 {
        if !path.exists() || std::fs::remove_file(path).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

#[cfg(windows)]
fn wait_for_exit(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !handle.is_null() {
            WaitForSingleObject(handle, 15_000);
            CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
fn wait_for_exit(_pid: u32) {
    std::thread::sleep(Duration::from_secs(1));
}
