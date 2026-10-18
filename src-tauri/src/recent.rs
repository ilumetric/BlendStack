//! Recent files from Blender's own `recent-files.txt` lists, and .blend thumbnails.

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
    pub name: String,
    pub folder: String,
    pub modified: i64,
    pub size: u64,
    /// Blender series whose list mentioned the file, e.g. `5.1`.
    pub series: String,
}

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
            let key = if cfg!(windows) {
                line.to_lowercase()
            } else {
                line.to_string()
            };
            if !seen.insert(key) {
                continue;
            }
            let path = Path::new(line);
            let Ok(meta) = std::fs::metadata(path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            files.push(RecentFile {
                path: line.to_string(),
                name: path
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
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
                series: series.clone(),
            });
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.modified));
    files.truncate(limit);
    files
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
