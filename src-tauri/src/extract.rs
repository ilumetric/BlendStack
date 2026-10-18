//! Unpacking downloaded archives into the library.

use crate::error::{msg, Result};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub const CANCELLED: &str = "cancelled";

/// Extracts `archive` into a new folder under `dest_root` and returns that folder.
pub fn extract(
    archive: &Path,
    dest_root: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(f64) + Sync),
) -> Result<PathBuf> {
    let file_name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = archive_stem(&file_name);
    fs::create_dir_all(dest_root)?;
    let staging = dest_root.join(format!(".staging-{stem}"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    let result = (|| -> Result<PathBuf> {
        let lower = file_name.to_ascii_lowercase();
        if lower.ends_with(".zip") {
            extract_zip(archive, &staging, cancel, progress)?;
        } else if lower.ends_with(".dmg") {
            extract_dmg(archive, &staging)?;
            return finalize(&staging, &dest_root.join(&stem), false);
        } else if lower.ends_with(".tar.xz") || lower.ends_with(".tar.bz2") {
            extract_tar(archive, &staging, cancel)?;
        } else {
            return Err(msg(format!("unsupported archive: {file_name}")));
        }
        finalize(&staging, &dest_root.join(&stem), true)
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn archive_stem(name: &str) -> String {
    for ext in [".tar.xz", ".tar.bz2", ".zip", ".dmg"] {
        if let Some(s) = name.strip_suffix(ext) {
            return s.to_string();
        }
    }
    name.to_string()
}

/// Moves the extracted content into its final place. Archives that wrap
/// everything in one top-level folder keep that folder's name.
fn finalize(staging: &Path, fallback: &Path, unwrap_single: bool) -> Result<PathBuf> {
    let entries: Vec<_> = fs::read_dir(staging)?.flatten().collect();
    let (source, target) = if unwrap_single && entries.len() == 1 && entries[0].path().is_dir() {
        let inner = entries[0].path();
        let target = staging.parent().unwrap().join(entries[0].file_name());
        (inner, target)
    } else {
        (staging.to_path_buf(), fallback.to_path_buf())
    };
    if target.exists() {
        fs::remove_dir_all(&target)?;
    }
    fs::rename(&source, &target)?;
    if staging.exists() {
        let _ = fs::remove_dir_all(staging);
    }
    Ok(target)
}

fn extract_zip(
    archive: &Path,
    out: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(f64) + Sync),
) -> Result<()> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(archive)?))?;

    // Pass 1: directory tree and the list of files, without decompressing anything.
    let mut files: Vec<(usize, PathBuf, u64)> = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let path = out.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            files.push((i, path, entry.size()));
        }
    }
    drop(zip);

    // Pass 2: decompress in parallel; each worker owns its own archive handle.
    let total: u64 = files.iter().map(|f| f.2).sum::<u64>().max(1);
    let done = AtomicU64::new(0);
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    files.sort_by_key(|f| std::cmp::Reverse(f.2));
    let mut buckets: Vec<Vec<(usize, PathBuf, u64)>> = vec![Vec::new(); workers];
    for (n, f) in files.into_iter().enumerate() {
        buckets[n % workers].push(f);
    }

    std::thread::scope(|scope| -> Result<()> {
        let handles: Vec<_> = buckets
            .into_iter()
            .map(|bucket| {
                let done = &done;
                scope.spawn(move || -> Result<()> {
                    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(archive)?))?;
                    for (i, path, size) in bucket {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(msg(CANCELLED));
                        }
                        let mut entry = zip.by_index(i)?;
                        let mut writer = BufWriter::with_capacity(256 * 1024, File::create(&path)?);
                        std::io::copy(&mut entry, &mut writer)?;
                        writer.flush()?;
                        #[cfg(unix)]
                        if let Some(mode) = entry.unix_mode() {
                            use std::os::unix::fs::PermissionsExt;
                            fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
                        }
                        let d = done.fetch_add(size, Ordering::Relaxed) + size;
                        progress(d as f64 / total as f64);
                    }
                    Ok(())
                })
            })
            .collect();
        for h in handles {
            h.join().map_err(|_| msg("extraction worker crashed"))??;
        }
        Ok(())
    })
}

#[cfg(target_os = "linux")]
fn extract_tar(archive: &Path, out: &Path, cancel: &AtomicBool) -> Result<()> {
    let file = BufReader::new(File::open(archive)?);
    let name = archive.to_string_lossy().to_ascii_lowercase();
    let reader: Box<dyn std::io::Read> = if name.ends_with(".bz2") {
        Box::new(bzip2::read::BzDecoder::new(file))
    } else {
        Box::new(xz2::read::XzDecoder::new(file))
    };
    let mut tar = tar::Archive::new(reader);
    tar.set_preserve_permissions(true);
    for entry in tar.entries()? {
        if cancel.load(Ordering::Relaxed) {
            return Err(msg(CANCELLED));
        }
        entry?.unpack_in(out)?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn extract_tar(_archive: &Path, _out: &Path, _cancel: &AtomicBool) -> Result<()> {
    Err(msg("tar archives are only supported on Linux"))
}

#[cfg(target_os = "macos")]
fn extract_dmg(archive: &Path, out: &Path) -> Result<()> {
    use std::process::Command;
    let mount = out.join(".mount");
    fs::create_dir_all(&mount)?;
    let status = Command::new("hdiutil")
        .args([
            "attach",
            "-nobrowse",
            "-readonly",
            "-noautoopen",
            "-mountpoint",
        ])
        .arg(&mount)
        .arg(archive)
        .status()?;
    if !status.success() {
        return Err(msg("could not mount the disk image"));
    }
    let copy = (|| -> Result<()> {
        let app = fs::read_dir(&mount)?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or_else(|| msg("no application found in the disk image"))?;
        let ok = Command::new("ditto")
            .arg(&app)
            .arg(out.join("Blender.app"))
            .status()?
            .success();
        if !ok {
            return Err(msg("could not copy Blender.app"));
        }
        Ok(())
    })();
    let _ = Command::new("hdiutil")
        .args(["detach", "-force"])
        .arg(&mount)
        .status();
    let _ = fs::remove_dir_all(&mount);
    copy
}

#[cfg(not(target_os = "macos"))]
fn extract_dmg(_archive: &Path, _out: &Path) -> Result<()> {
    Err(msg("disk images are only supported on macOS"))
}
