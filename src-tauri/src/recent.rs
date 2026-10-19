//! Files for the Library: recent files from Blender's own `recent-files.txt`
//! lists, recovery files Blender leaves in its temp folder (autosaves and the
//! last session), backups next to recent files (`.blend1`, `.blend2`, …),
//! and the thumbnails embedded in .blend files.

use crate::error::{msg, Result};
use base64::Engine;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    /// Display name: the project name, without autosave or backup suffixes.
    pub name: String,
    pub folder: String,
    pub modified: i64,
    pub size: u64,
    /// Blender series that saved the file, e.g. `5.1`.
    pub series: String,
    /// `recent`, `autosave`, `session` (last session) or `backup`.
    pub kind: String,
    /// Backup number for `.blendN` files.
    pub backup: Option<u32>,
}

/// Blender's internal files in its temp folder; never real projects.
const INTERNAL_FILES: &[&str] = &["quit.blend", "copybuffer.blend"];
/// Blender keeps at most 32 save versions.
const MAX_BACKUPS: u32 = 32;

/// Blender's per-user configuration root.
fn config_root() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(if cfg!(target_os = "windows") {
        base.join("Blender Foundation").join("Blender")
    } else if cfg!(target_os = "macos") {
        base.join("Blender")
    } else {
        base.join("blender")
    })
}

fn normalized(path: &Path) -> String {
    let s = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// Files in the system temp folder are Blender's own (autosaves, last
/// session, copy buffer); they belong in Recovery, not in Recent.
fn in_temp_dir(path: &Path) -> bool {
    let temp = normalized(&std::env::temp_dir());
    let parent = path.parent().map(normalized).unwrap_or_default();
    parent.trim_end_matches(['\\', '/']) == temp.trim_end_matches(['\\', '/'])
}

fn entry(
    path: &Path,
    meta: &std::fs::Metadata,
    name: String,
    series: String,
    kind: &str,
) -> RecentFile {
    RecentFile {
        path: path.to_string_lossy().into_owned(),
        name,
        folder: path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        size: meta.len(),
        series,
        kind: kind.to_string(),
        backup: None,
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Recent files from every Blender version's history, newest first.
pub fn list(limit: usize) -> Vec<RecentFile> {
    let Some(root) = config_root() else {
        return Vec::new();
    };
    let Ok(series_dirs) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for dir in series_dirs.flatten() {
        let series = dir.file_name().to_string_lossy().into_owned();
        let Ok(text) = std::fs::read_to_string(dir.path().join("config").join("recent-files.txt"))
        else {
            continue;
        };
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let path = Path::new(line);
            if is_internal(path) || !seen.insert(normalized(path)) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(path) else {
                continue;
            };
            if meta.is_file() {
                files.push(entry(path, &meta, stem(path), series.clone(), "recent"));
            }
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.modified));
    files.truncate(limit);
    files
}

/// Autosaves (`Name_PID_autosave.blend`, `PID_autosave.blend`) and the last
/// session (`quit.blend`) from the temp folder, newest first.
pub fn recovery(limit: usize) -> Vec<RecentFile> {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for e in entries.flatten() {
        let file_name = e.file_name().to_string_lossy().into_owned();
        let lower = file_name.to_lowercase();
        let kind = if lower == "quit.blend" {
            "session"
        } else if lower.ends_with("_autosave.blend") {
            "autosave"
        } else {
            continue;
        };
        let Ok(meta) = e.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let path = e.path();
        // "Woodshed_27200_autosave.blend" → "Woodshed"; unsaved files have no name.
        let name = if kind == "autosave" {
            let base = &file_name[..file_name.len() - "_autosave.blend".len()];
            match base.rsplit_once('_') {
                Some((project, pid)) if pid.chars().all(|c| c.is_ascii_digit()) => {
                    project.to_string()
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };
        let series = blend_series(&path).unwrap_or_default();
        files.push(entry(&path, &meta, name, series, kind));
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.modified));
    files.truncate(limit);
    files
}

/// Backups Blender keeps next to saved files: `Name.blend1` is the previous
/// save, `Name.blend2` the one before, and so on.
pub fn backups(limit: usize) -> Vec<RecentFile> {
    let mut files = Vec::new();
    for recent in list(60) {
        let path = PathBuf::from(&recent.path);
        for n in 1..=MAX_BACKUPS {
            let mut backup = path.clone().into_os_string();
            backup.push(n.to_string());
            let backup = PathBuf::from(backup);
            let Ok(meta) = std::fs::metadata(&backup) else {
                break;
            };
            // The series of the saved file; reading each backup could pull it from a cloud drive.
            let mut file = entry(
                &backup,
                &meta,
                recent.name.clone(),
                recent.series.clone(),
                "backup",
            );
            file.backup = Some(n);
            files.push(file);
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.modified));
    files.truncate(limit);
    files
}

/// Blender series that wrote a .blend file, read from its header.
fn blend_series(path: &Path) -> Option<String> {
    let mut head = [0u8; 17];
    open_blend(path).ok()?.read_exact(&mut head).ok()?;
    if &head[..7] != b"BLENDER" {
        return None;
    }
    let digits = match head[7] {
        // Classic header: "BLENDER-v405".
        b'_' | b'-' => &head[9..12],
        // Blender 5 header: "BLENDER17-01v0502".
        b'0'..=b'9' => &head[13..17],
        _ => return None,
    };
    let version: u32 = std::str::from_utf8(digits).ok()?.parse().ok()?;
    Some(format!("{}.{}", version / 100, version % 100))
}

/// Blender's own files (last session, copy buffer, autosaves) — shown in
/// Recovery, never in Recent, even with a custom temp folder.
fn is_internal(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    in_temp_dir(path)
        || INTERNAL_FILES.contains(&name.as_str())
        || name.ends_with("_autosave.blend")
}

/// PNG data URL of the thumbnail embedded in a .blend file, cached on disk.
pub fn thumbnail(path: &Path, cache_dir: &Path) -> Result<Option<String>> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key =
        hex::encode(&Sha256::digest(format!("{}|{mtime}|{}", path.display(), meta.len()))[..12]);
    let thumbs = cache_dir.join("thumbs");
    let cached = thumbs.join(format!("{key}.png"));
    let empty_marker = thumbs.join(format!("{key}.none"));

    let png = if let Ok(bytes) = std::fs::read(&cached) {
        bytes
    } else if empty_marker.exists() {
        return Ok(None);
    } else {
        std::fs::create_dir_all(&thumbs)?;
        match read_blend_thumbnail(path)? {
            Some((w, h, rgba)) => {
                let png = encode_png(w, h, &rgba)?;
                let _ = std::fs::write(&cached, &png);
                png
            }
            None => {
                let _ = std::fs::write(&empty_marker, b"");
                return Ok(None);
            }
        }
    };
    Ok(Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )))
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| msg(e.to_string()))?;
        // Blender stores rows bottom-up.
        let row = (w * 4) as usize;
        let flipped: Vec<u8> = rgba.chunks_exact(row).rev().flatten().copied().collect();
        writer
            .write_image_data(&flipped)
            .map_err(|e| msg(e.to_string()))?;
    }
    Ok(out)
}

fn open_blend(path: &Path) -> Result<Box<dyn Read>> {
    let mut file = File::open(path)?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    let file = File::open(path)?;
    Ok(match magic {
        [0x28, 0xB5, 0x2F, 0xFD] => Box::new(zstd::stream::read::Decoder::new(file)?),
        [0x1F, 0x8B, ..] => Box::new(flate2::read::GzDecoder::new(BufReader::new(file))),
        _ => Box::new(BufReader::new(file)),
    })
}

/// Walks the first blocks of a .blend file looking for the `TEST` (thumbnail) block.
/// Supports the classic 12-byte header and the 17-byte header introduced in Blender 5.0.
fn read_blend_thumbnail(path: &Path) -> Result<Option<(u32, u32, Vec<u8>)>> {
    let mut r = open_blend(path)?;
    let mut head = [0u8; 12];
    r.read_exact(&mut head)?;
    if &head[..7] != b"BLENDER" {
        return Ok(None);
    }

    enum Layout {
        Legacy { ptr: usize, little: bool },
        Large,
    }
    let layout = match head[7] {
        b'_' => Layout::Legacy {
            ptr: 4,
            little: head[8] == b'v',
        },
        b'-' => Layout::Legacy {
            ptr: 8,
            little: head[8] == b'v',
        },
        b'0'..=b'9' => {
            // "BLENDER17-01v0502": header size digits, '-', format version, endianness, version.
            let size: usize = std::str::from_utf8(&head[7..9])
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if size < 12 || head[9] != b'-' {
                return Ok(None);
            }
            let mut rest = vec![0u8; size - 12];
            r.read_exact(&mut rest)?;
            if rest.first() != Some(&b'v') {
                return Ok(None);
            }
            Layout::Large
        }
        _ => return Ok(None),
    };

    let read_i32 = |b: &[u8], little: bool| {
        let a = [b[0], b[1], b[2], b[3]];
        if little {
            i32::from_le_bytes(a)
        } else {
            i32::from_be_bytes(a)
        }
    };

    for _ in 0..64 {
        let (code, len, little) = match layout {
            Layout::Legacy { ptr, little } => {
                let mut bh = vec![0u8; 16 + ptr];
                if r.read_exact(&mut bh).is_err() {
                    return Ok(None);
                }
                (
                    [bh[0], bh[1], bh[2], bh[3]],
                    read_i32(&bh[4..8], little) as i64,
                    little,
                )
            }
            Layout::Large => {
                let mut bh = [0u8; 32];
                if r.read_exact(&mut bh).is_err() {
                    return Ok(None);
                }
                let len = i64::from_le_bytes(bh[16..24].try_into().unwrap());
                ([bh[0], bh[1], bh[2], bh[3]], len, true)
            }
        };
        if len < 0 {
            return Ok(None);
        }
        match &code {
            b"TEST" => {
                let mut dims = [0u8; 8];
                r.read_exact(&mut dims)?;
                let w = read_i32(&dims[..4], little);
                let h = read_i32(&dims[4..], little);
                if !(1..=2048).contains(&w) || !(1..=2048).contains(&h) {
                    return Ok(None);
                }
                let mut rgba = vec![0u8; (w * h * 4) as usize];
                r.read_exact(&mut rgba)?;
                return Ok(Some((w as u32, h as u32, rgba)));
            }
            b"GLOB" | b"DNA1" | b"ENDB" => return Ok(None),
            _ => {
                std::io::copy(&mut (&mut r).take(len as u64), &mut std::io::sink())?;
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    /// Run with `BLEND_FILE=<path> cargo test -- --ignored` against a real file.
    #[test]
    #[ignore]
    fn reads_real_thumbnail() {
        let path = std::env::var("BLEND_FILE").expect("set BLEND_FILE");
        let thumb = super::read_blend_thumbnail(std::path::Path::new(&path)).unwrap();
        let (w, h, rgba) = thumb.expect("file has no thumbnail");
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        println!("thumbnail {w}x{h}");
    }
}

#[cfg(test)]
mod list_tests {
    /// `cargo test lists_real_files -- --ignored --nocapture` prints the
    /// three Library lists for the current user.
    #[test]
    #[ignore]
    fn lists_real_files() {
        for (name, files) in [
            ("recent", super::list(40)),
            ("recovery", super::recovery(60)),
            ("backups", super::backups(60)),
        ] {
            println!("== {name}: {}", files.len());
            for f in files.iter().take(6) {
                println!(
                    "  {:<22} {:<9} {:?} {:<4} {}",
                    f.name, f.kind, f.backup, f.series, f.path
                );
            }
        }
    }
}
