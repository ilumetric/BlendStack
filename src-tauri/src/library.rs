//! Builds installed on disk: scanning, manifests and removal.

use crate::error::{msg, Result};
use crate::model::{Channel, InstalledBuild, Version};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub const MANIFEST: &str = ".blendstack.json";

static DAILY_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"blender-(\d+\.\d+\.\d+)-([a-z]+)\+(.+?)\.([0-9a-f]{12})-").unwrap()
});
static VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+\.\d+(?:\.\d+)?[a-z]?)").unwrap());
static SERIES_DIR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\d+$").unwrap());

/// The binary that starts Blender inside a build folder.
pub fn executable(dir: &Path) -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &["blender.exe"]
    } else if cfg!(target_os = "macos") {
        &["Blender.app/Contents/MacOS/Blender"]
    } else {
        &["blender"]
    };
    candidates.iter().map(|c| dir.join(c)).find(|p| p.is_file())
}

pub fn scan(library: &Path, external: &[String]) -> Vec<InstalledBuild> {
    let mut out = Vec::new();
    for channel in Channel::REMOTE {
        let root = library.join(channel.dir_name());
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !path.is_dir() {
                continue;
            }
            let id = format!("{}/{}", channel.dir_name(), name);
            if let Some(build) = load(&path, id, channel, false) {
                out.push(build);
            }
        }
    }
    for p in external {
        let path = PathBuf::from(p);
        if let Some(build) = load(&path, format!("external:{p}"), Channel::Custom, true) {
            out.push(build);
        }
    }
    out
}

fn load(dir: &Path, id: String, channel: Channel, external: bool) -> Option<InstalledBuild> {
    let exe = executable(dir)?;
    let mut build = std::fs::read(dir.join(MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice::<InstalledBuild>(&b).ok())
        .unwrap_or_else(|| {
            let mut b = infer(dir, channel);
            apply_blinfo(dir, &mut b);
            b
        });
    build.id = id;
    build.path = dir.to_string_lossy().into_owned();
    build.executable = exe.to_string_lossy().into_owned();
    build.external = external;
    if external {
        build.channel = Channel::Custom;
    }
    if build.installed_at == 0 {
        build.installed_at = std::fs::metadata(dir)
            .and_then(|m| m.created().or_else(|_| m.modified()))
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
    }
    Some(build)
}

/// Best-effort metadata for folders without a manifest (copied in by hand or imported).
fn infer(dir: &Path, channel: Channel) -> InstalledBuild {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut b = InstalledBuild {
        id: String::new(),
        path: String::new(),
        executable: String::new(),
        channel,
        version: String::new(),
        release_cycle: "stable".into(),
        branch: String::new(),
        patch: None,
        hash: None,
        date: None,
        installed_at: 0,
        source_id: None,
        archive_size: None,
        lts: false,
        external: false,
    };
    if let Some(c) = DAILY_NAME_RE.captures(&name) {
        b.version = c[1].to_string();
        b.release_cycle = c[2].to_string();
        b.branch = c[3].to_string();
        b.hash = Some(c[4].to_string());
        if let Some(pr) = b.branch.split('-').find(|p| p.starts_with("PR")) {
            b.patch = Some(pr.to_string());
        }
    } else if let Some(c) = VERSION_RE.captures(&name) {
        b.version = c[1].to_string();
    } else if let Some(series) = series_dir(dir) {
        b.version = series;
    }
    b.lts = Version::parse(&b.version).is_lts();
    b
}

/// Reads the `.blinfo` file Blender Launcher writes next to its builds, so an
/// existing Blender Launcher library shows correct versions and dates.
fn apply_blinfo(dir: &Path, b: &mut InstalledBuild) {
    #[derive(serde::Deserialize)]
    struct File {
        blinfo: Vec<Info>,
    }
    #[derive(serde::Deserialize)]
    struct Info {
        #[serde(default)]
        branch: String,
        #[serde(default)]
        subversion: String,
        #[serde(default)]
        build_hash: Option<String>,
        #[serde(default)]
        commit_time: Option<String>,
    }
    let Some(info) = std::fs::read(dir.join(".blinfo"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<File>(&bytes).ok())
        .and_then(|f| f.blinfo.into_iter().next())
    else {
        return;
    };
    if !info.subversion.is_empty() {
        b.version = info.subversion;
    }
    match info.branch.as_str() {
        "stable" | "lts" | "" => b.release_cycle = "stable".into(),
        other if b.branch.is_empty() => b.branch = other.to_string(),
        _ => {}
    }
    if info.branch == "lts" {
        b.lts = true;
    }
    if b.hash.is_none() {
        b.hash = info.build_hash.filter(|h| !h.is_empty());
    }
    b.date = info
        .commit_time
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
        .map(|d| d.timestamp())
        .or(b.date);
    b.lts = b.lts || Version::parse(&b.version).is_lts();
}

/// Blender keeps its data in a sibling folder named after the series, e.g. `5.1`.
fn series_dir(dir: &Path) -> Option<String> {
    let search = if cfg!(target_os = "macos") {
        dir.join("Blender.app/Contents/Resources")
    } else {
        dir.to_path_buf()
    };
    std::fs::read_dir(search)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| SERIES_DIR_RE.is_match(n))
        .max_by_key(|n| Version::parse(n))
}

pub fn write_manifest(dir: &Path, build: &InstalledBuild) -> Result<()> {
    let mut stored = build.clone();
    stored.path.clear();
    stored.executable.clear();
    std::fs::write(dir.join(MANIFEST), serde_json::to_vec_pretty(&stored)?)?;
    Ok(())
}

/// Removes a managed build. Imported builds are never deleted from disk.
pub fn remove(library: &Path, build: &InstalledBuild) -> Result<()> {
    if build.external {
        return Ok(());
    }
    let dir = PathBuf::from(&build.path);
    let canonical_lib = library.canonicalize()?;
    let canonical_dir = dir.canonicalize()?;
    if !canonical_dir.starts_with(&canonical_lib) || canonical_dir == canonical_lib {
        return Err(msg("refusing to delete a folder outside the library"));
    }
    // Rename first so a half-deleted build never shows up as installed.
    let trash = dir.with_file_name(format!(
        ".trash-{}-{}",
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        crate::model::now()
    ));
    let target = match std::fs::rename(&dir, &trash) {
        Ok(()) => trash,
        Err(e) if is_in_use(&e) => {
            return Err(msg("this build is in use — close Blender and try again"));
        }
        Err(_) => dir,
    };
    std::fs::remove_dir_all(&target)?;
    Ok(())
}

fn is_in_use(e: &std::io::Error) -> bool {
    // ERROR_SHARING_VIOLATION / ERROR_ACCESS_DENIED on Windows.
    matches!(e.raw_os_error(), Some(32) | Some(5)) && cfg!(target_os = "windows")
}

/// Moves every managed build into another library folder. Returns how many moved.
pub fn move_all(from: &Path, to: &Path) -> Result<usize> {
    let mut moved = 0;
    for channel in Channel::REMOTE {
        let Ok(entries) = std::fs::read_dir(from.join(channel.dir_name())) else {
            continue;
        };
        let dest_root = to.join(channel.dir_name());
        std::fs::create_dir_all(&dest_root)?;
        for e in entries.flatten() {
            let name = e.file_name();
            if name.to_string_lossy().starts_with('.') || !e.path().is_dir() {
                continue;
            }
            let dest = dest_root.join(&name);
            if dest.exists() {
                continue;
            }
            if std::fs::rename(e.path(), &dest).is_err() {
                // Different volume: copy, then remove the original.
                copy_dir(&e.path(), &dest)?;
                std::fs::remove_dir_all(e.path())?;
            }
            moved += 1;
        }
    }
    Ok(moved)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), &target)?;
        }
    }
    Ok(())
}

/// Cleans up leftovers of interrupted installs and deletions.
pub fn sweep(library: &Path) {
    for channel in Channel::REMOTE {
        let Ok(entries) = std::fs::read_dir(library.join(channel.dir_name())) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with(".trash-") || name.starts_with(".staging-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_daily_folder() {
        let b = infer(
            Path::new("blender-5.3.0-beta+v53.a1b2c3d4e5f6-windows.amd64-release"),
            Channel::Daily,
        );
        assert_eq!(b.version, "5.3.0");
        assert_eq!(b.release_cycle, "beta");
        assert_eq!(b.branch, "v53");
        assert_eq!(b.hash.as_deref(), Some("a1b2c3d4e5f6"));
    }

    #[test]
    fn infers_pr_folder() {
        let b = infer(
            Path::new("blender-4.5.12-candidate+main-PR161547.edd0646913eb-windows.amd64-release"),
            Channel::Experimental,
        );
        assert_eq!(b.patch.as_deref(), Some("PR161547"));
    }

    #[test]
    fn reads_blender_launcher_metadata() {
        let dir = std::env::temp_dir().join(format!("bs-blinfo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".blinfo"),
            r#"{"file_version": "1.5", "blinfo": [{"branch": "lts", "subversion": "5.2.0", "build_hash": "fbe6228777e7", "commit_time": "2026-07-14T12:24:00+05:00", "custom_name": "", "is_favorite": true}]}"#,
        )
        .unwrap();
        let mut b = infer(&dir, Channel::Stable);
        apply_blinfo(&dir, &mut b);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(b.version, "5.2.0");
        assert!(b.lts);
        assert_eq!(b.release_cycle, "stable");
        assert_eq!(b.hash.as_deref(), Some("fbe6228777e7"));
        assert!(b.date.is_some());
    }

    #[test]
    fn infers_stable_folder() {
        let b = infer(Path::new("blender-4.2.3-windows-x64"), Channel::Stable);
        assert_eq!(b.version, "4.2.3");
        assert!(b.lts);
    }
}
