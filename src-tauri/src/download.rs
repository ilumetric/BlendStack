//! Download → verify → extract pipeline with resume, retries and cancellation.

use crate::error::{msg, Error, Result};
use crate::extract::{self, CANCELLED};
use crate::library;
use crate::model::{now, InstalledBuild, RemoteBuild};
use crate::state::{emit_library_changed, emit_settings_changed, AppState};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

const MAX_PARALLEL: usize = 2;
const MAX_RETRIES: u32 = 4;
const EMIT_EVERY: Duration = Duration::from_millis(200);

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Queued,
    Downloading,
    Verifying,
    Extracting,
    Done,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Downloading | Self::Verifying | Self::Extracting
        )
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: String,
    pub build: RemoteBuild,
    pub state: TaskState,
    pub received: u64,
    pub total: u64,
    /// Bytes per second.
    pub speed: f64,
    /// Extraction progress, 0..1.
    pub progress: f64,
    pub error: Option<String>,
    pub installed_id: Option<String>,
    /// Installed build that this download updates.
    pub replaces: Option<String>,
    pub started_at: i64,
}

struct Entry {
    task: DownloadTask,
    cancel: Arc<AtomicBool>,
}

pub struct Downloads {
    entries: Mutex<Vec<Entry>>,
    slots: Arc<tokio::sync::Semaphore>,
}

impl Default for Downloads {
    fn default() -> Self {
        Downloads {
            entries: Mutex::new(Vec::new()),
            slots: Arc::new(tokio::sync::Semaphore::new(MAX_PARALLEL)),
        }
    }
}

impl Downloads {
    pub fn list(&self) -> Vec<DownloadTask> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.task.clone())
            .collect()
    }

    pub fn has_active(&self) -> bool {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.task.state.is_active())
    }

    pub fn cancel(&self, id: &str) {
        if let Some(e) = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.task.id == id)
        {
            e.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn clear_finished(&self) {
        self.entries
            .lock()
            .unwrap()
            .retain(|e| e.task.state.is_active());
    }

    fn update(&self, app: &AppHandle, id: &str, f: impl FnOnce(&mut DownloadTask)) {
        let snapshot = {
            let mut entries = self.entries.lock().unwrap();
            let Some(e) = entries.iter_mut().find(|e| e.task.id == id) else {
                return;
            };
            f(&mut e.task);
            e.task.clone()
        };
        let _ = app.emit("download:progress", snapshot);
    }
}

pub fn start(app: &AppHandle, build: RemoteBuild, replaces: Option<String>) -> DownloadTask {
    let state = app.state::<AppState>();
    let task = {
        let mut entries = state.downloads.entries.lock().unwrap();
        if let Some(e) = entries
            .iter()
            .find(|e| e.task.id == build.id && e.task.state.is_active())
        {
            return e.task.clone();
        }
        entries.retain(|e| e.task.id != build.id);
        let task = DownloadTask {
            id: build.id.clone(),
            total: build.file_size,
            build,
            state: TaskState::Queued,
            received: 0,
            speed: 0.0,
            progress: 0.0,
            error: None,
            installed_id: None,
            replaces,
            started_at: now(),
        };
        entries.push(Entry {
            task: task.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
        });
        task
    };
    let _ = app.emit("download:progress", task.clone());

    let app = app.clone();
    let id = task.id.clone();
    tauri::async_runtime::spawn(async move { run(app, id).await });
    task
}

async fn run(app: AppHandle, id: String) {
    let state = app.state::<AppState>();
    let cancel = {
        let entries = state.downloads.entries.lock().unwrap();
        match entries.iter().find(|e| e.task.id == id) {
            Some(e) => e.cancel.clone(),
            None => return,
        }
    };
    let _permit = state.downloads.slots.clone().acquire_owned().await;

    let result = if cancel.load(Ordering::Relaxed) {
        Err(msg(CANCELLED))
    } else {
        install(&app, &id, &cancel).await
    };

    match result {
        Ok(installed) => {
            state.downloads.update(&app, &id, |t| {
                t.state = TaskState::Done;
                t.progress = 1.0;
                t.speed = 0.0;
                t.installed_id = Some(installed.id.clone());
            });
            emit_library_changed(&app);
            // An update may have removed the build registered for .blend files.
            crate::assoc::repair_in_background(&app);
        }
        Err(e) => {
            let cancelled = cancel.load(Ordering::Relaxed);
            if !cancelled {
                log::error!("install of {id} failed: {e}");
            }
            state.downloads.update(&app, &id, |t| {
                t.speed = 0.0;
                if cancelled {
                    t.state = TaskState::Cancelled;
                } else {
                    t.state = TaskState::Failed;
                    t.error = Some(e.to_string());
                }
            });
        }
    }
}

async fn install(app: &AppHandle, id: &str, cancel: &Arc<AtomicBool>) -> Result<InstalledBuild> {
    let state = app.state::<AppState>();
    let (build, replaces) = {
        let entries = state.downloads.entries.lock().unwrap();
        let e = entries
            .iter()
            .find(|e| e.task.id == id)
            .ok_or_else(|| msg("task vanished"))?;
        (e.task.build.clone(), e.task.replaces.clone())
    };
    let settings = state.settings();
    let library_dir = state.paths.library(&settings);
    let downloads_dir = library_dir.join(".downloads");
    tokio::fs::create_dir_all(&downloads_dir).await?;
    let part = downloads_dir.join(format!("{}.part", build.file_name));
    let archive = downloads_dir.join(&build.file_name);

    // 1. Download (resumable).
    state
        .downloads
        .update(app, id, |t| t.state = TaskState::Downloading);
    let digest = download_with_retries(app, id, &build, &part, cancel)
        .await
        .inspect_err(|_| {
            if cancel.load(Ordering::Relaxed) {
                let _ = std::fs::remove_file(&part);
            }
        })?;
    tokio::fs::rename(&part, &archive).await?;

    // 2. Verify against the official checksum when one is published.
    if let Some(url) = &build.sha256_url {
        state
            .downloads
            .update(app, id, |t| t.state = TaskState::Verifying);
        match fetch_checksum(&state.http, url, &build.file_name).await {
            Ok(Some(expected)) if !expected.eq_ignore_ascii_case(&digest) => {
                let _ = tokio::fs::remove_file(&archive).await;
                return Err(msg(
                    "checksum mismatch — the download was corrupted, please retry",
                ));
            }
            Ok(_) => {}
            Err(e) => log::warn!("checksum unavailable for {}: {e}", build.file_name),
        }
    }

    // 3. Extract.
    state.downloads.update(app, id, |t| {
        t.state = TaskState::Extracting;
        t.progress = 0.0;
        t.speed = 0.0;
    });
    let dest_root = library_dir.join(build.channel.dir_name());
    let final_dir = {
        let app = app.clone();
        let id = id.to_string();
        let cancel = cancel.clone();
        let archive = archive.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let last = AtomicU64::new(0);
            let report = |p: f64| {
                // Emit at most once per percent.
                let pct = (p * 100.0) as u64;
                if last.fetch_max(pct, Ordering::Relaxed) < pct {
                    app.state::<AppState>()
                        .downloads
                        .update(&app, &id, |t| t.progress = p);
                }
            };
            extract::extract(&archive, &dest_root, &cancel, &report)
        })
        .await
        .map_err(|e| msg(format!("extraction task failed: {e}")))??
    };

    // 4. Record what was installed.
    let folder = final_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let installed = InstalledBuild {
        id: format!("{}/{}", build.channel.dir_name(), folder),
        path: final_dir.to_string_lossy().into_owned(),
        executable: library::executable(&final_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        channel: build.channel,
        version: build.version.clone(),
        release_cycle: build.release_cycle.clone(),
        branch: build.branch.clone(),
        patch: build.patch.clone(),
        hash: build.hash.clone(),
        date: Some(build.date),
        installed_at: now(),
        source_id: Some(build.id.clone()),
        archive_size: Some(build.file_size),
        lts: build.lts,
        external: false,
    };
    if installed.executable.is_empty() {
        return Err(msg("the archive does not contain a Blender executable"));
    }
    library::write_manifest(&final_dir, &installed)?;
    if !settings.keep_archives {
        let _ = tokio::fs::remove_file(&archive).await;
    }

    // 5. Replace the build this one updates.
    if let Some(old_id) = replaces.filter(|old| *old != installed.id) {
        let new_id = installed.id.clone();
        if let Ok(s) = state.update_settings(|s| s.transfer_build(&old_id, &new_id)) {
            emit_settings_changed(app, &s);
        }
        if settings.replace_on_update {
            if let Some(old) = state.find_installed(&old_id) {
                let lib = library_dir.clone();
                let res =
                    tauri::async_runtime::spawn_blocking(move || library::remove(&lib, &old)).await;
                if let Ok(Err(e)) = res {
                    log::warn!("could not remove replaced build {old_id}: {e}");
                }
            }
        }
    }
    Ok(installed)
}

async fn download_with_retries(
    app: &AppHandle,
    id: &str,
    build: &RemoteBuild,
    part: &Path,
    cancel: &AtomicBool,
) -> Result<String> {
    let mut attempt = 0;
    loop {
        match download(app, id, build, part, cancel).await {
            Ok(d) => return Ok(d),
            Err(e) if cancel.load(Ordering::Relaxed) => return Err(e),
            Err(e) if attempt < MAX_RETRIES && is_transient(&e) => {
                attempt += 1;
                log::warn!("download of {id} interrupted ({e}), retry {attempt}");
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
            }
            Err(e) => return Err(e),
        }
    }
}

fn is_transient(e: &Error) -> bool {
    match e {
        Error::Http(h) => {
            h.is_timeout()
                || h.is_connect()
                || h.is_body()
                || h.is_request()
                || h.status().is_some_and(|s| s.is_server_error())
        }
        Error::Io(_) => false,
        Error::Msg(m) => m.starts_with("incomplete"),
        _ => false,
    }
}

/// Streams the file to `part`, resuming from what is already there. Returns the SHA-256.
async fn download(
    app: &AppHandle,
    id: &str,
    build: &RemoteBuild,
    part: &Path,
    cancel: &AtomicBool,
) -> Result<String> {
    let state = app.state::<AppState>();
    let existing = tokio::fs::metadata(part)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let mut request = state.http.get(&build.url);
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let response = request.send().await?;
    if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // The partial file is stale or already complete; start over.
        tokio::fs::remove_file(part).await?;
        return Err(msg("incomplete partial download, restarting"));
    }
    let response = response.error_for_status()?;
    let resumed = existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;

    let (mut hasher, file, mut received) = if resumed {
        let path = part.to_path_buf();
        let hasher = tauri::async_runtime::spawn_blocking(move || hash_file(&path))
            .await
            .map_err(|e| msg(e.to_string()))??;
        let file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(part)
            .await?;
        (hasher, file, existing)
    } else {
        (Sha256::new(), tokio::fs::File::create(part).await?, 0)
    };
    let total = response
        .content_length()
        .map(|l| l + received)
        .unwrap_or(build.file_size);
    state.downloads.update(app, id, |t| {
        t.total = total;
        t.received = received;
    });

    let mut writer = tokio::io::BufWriter::with_capacity(1 << 20, file);
    let mut stream = response.bytes_stream();
    let mut last_emit = Instant::now();
    let mut window_bytes = 0u64;
    let mut speed = 0.0f64;
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(msg(CANCELLED));
        }
        let chunk = chunk?;
        hasher.update(&chunk);
        writer.write_all(&chunk).await?;
        received += chunk.len() as u64;
        window_bytes += chunk.len() as u64;
        let elapsed = last_emit.elapsed();
        if elapsed >= EMIT_EVERY {
            let instant = window_bytes as f64 / elapsed.as_secs_f64();
            speed = if speed == 0.0 {
                instant
            } else {
                speed * 0.7 + instant * 0.3
            };
            state.downloads.update(app, id, |t| {
                t.received = received;
                t.speed = speed;
            });
            last_emit = Instant::now();
            window_bytes = 0;
        }
    }
    writer.flush().await?;
    drop(writer);
    state.downloads.update(app, id, |t| t.received = received);
    if total > 0 && received < total {
        return Err(msg(format!(
            "incomplete download ({received} of {total} bytes)"
        )));
    }
    Ok(hex::encode(hasher.finalize()))
}

fn hash_file(path: &PathBuf) -> Result<Sha256> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher)
}

async fn fetch_checksum(
    client: &reqwest::Client,
    url: &str,
    file_name: &str,
) -> Result<Option<String>> {
    let text = client
        .get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_checksum(&text, file_name))
}

fn parse_checksum(text: &str, file_name: &str) -> Option<String> {
    let is_hash = |s: &str| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit());
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), name) = (parts.next(), parts.next()) else {
            continue;
        };
        let name = name.map(|n| n.trim_start_matches('*'));
        if is_hash(hash) && (name.is_none() || name == Some(file_name)) {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_checksum;

    #[test]
    fn parses_checksum_files() {
        let h = "a".repeat(64);
        let list = format!("{} other.zip\n{h}  blender.zip\n", "b".repeat(64));
        assert_eq!(parse_checksum(&list, "blender.zip"), Some(h.clone()));
        assert_eq!(parse_checksum(&h, "blender.zip"), Some(h));
        assert_eq!(parse_checksum("nothing", "x"), None);
    }
}
