//! Which Blender opens .blend files.
//!
//! Uses Blender's own registration (`blender --register`), which writes a
//! per-user file type named after the series (`blender.5.2`). Besides the
//! .blend association it carries the AppUserModelId Blender gives its
//! windows, which is where Windows takes the taskbar icon from: without a
//! registration (for example after another launcher unregistered Blender)
//! the taskbar shows a blank icon.
//!
//! When the registered build disappears (updated or deleted), the
//! registration is moved to another installed build of the same series, so
//! the icon and the association keep working. Nothing is registered unless
//! the user did it once.

use crate::error::{msg, Result};
use crate::model::{InstalledBuild, Version};
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub async fn register_blend_files(state: State<'_, AppState>, id: String) -> Result<()> {
    let build = state
        .find_installed(&id)
        .ok_or_else(|| msg("build not found"))?;
    let exe = PathBuf::from(&build.executable);
    tauri::async_runtime::spawn_blocking(move || register(&exe))
        .await
        .map_err(|e| msg(e.to_string()))?
}

/// Folder of the Blender build that currently opens .blend files, if any.
#[tauri::command]
pub fn blend_association() -> Option<String> {
    association().map(|(_, dir)| dir.to_string_lossy().into_owned())
}

/// Runs Blender's own registration for the build at `exe`.
pub fn register(exe: &Path) -> Result<()> {
    let mut cmd = Command::new(exe);
    cmd.arg("--register");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let status = cmd
        .status()
        .map_err(|e| msg(format!("could not run Blender: {e}")))?;
    if !status.success() {
        return Err(msg("Blender could not register .blend files"));
    }
    #[cfg(windows)]
    notify_shell();
    Ok(())
}

/// Moves a Blender registration whose build no longer exists to an installed
/// build of the same series. Returns true when it re-registered.
pub fn repair(installed: &[InstalledBuild]) -> bool {
    let Some((prog_id, dir)) = association() else {
        return false;
    };
    // Only Blender's own registrations (`blender.X.Y`), and only broken ones.
    let Some(series) = prog_id.strip_prefix("blender.") else {
        return false;
    };
    if dir
        .join(if cfg!(windows) {
            "blender.exe"
        } else {
            "blender"
        })
        .is_file()
    {
        return false;
    }
    let Some(target) = installed
        .iter()
        .filter(|b| {
            let v = Version::parse(&b.version);
            format!("{}.{}", v.major, v.minor) == series
        })
        // Prefer stable builds, then the newest.
        .max_by_key(|b| {
            (
                b.release_cycle == "stable",
                Version::parse(&b.version),
                b.date,
            )
        })
    else {
        return false;
    };
    match register(Path::new(&target.executable)) {
        Ok(()) => {
            log::info!("moved .blend registration {prog_id} to {}", target.path);
            true
        }
        Err(e) => {
            log::warn!("could not move .blend registration: {e}");
            false
        }
    }
}

/// Runs [`repair`] off the UI thread and tells the UI when it changed anything.
pub fn repair_in_background(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let installed = app.state::<AppState>().installed();
        if repair(&installed) {
            let _ = app.emit("association:changed", ());
        }
    });
}

/// The registered file type for .blend and the folder of its Blender.
fn association() -> Option<(String, PathBuf)> {
    #[cfg(windows)]
    {
        let prog_id = registry::read_string(r"Software\Classes\.blend")?;
        let command =
            registry::read_string(&format!(r"Software\Classes\{prog_id}\shell\open\command"))?;
        // `"C:\…\blender-launcher.exe" "%1"`
        let exe = command
            .trim_start()
            .strip_prefix('"')?
            .split('"')
            .next()?
            .to_string();
        let dir = PathBuf::from(exe).parent()?.to_path_buf();
        Some((prog_id, dir))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Tells Explorer that file associations changed so icons refresh at once.
#[cfg(windows)]
fn notify_shell() {
    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
}

#[cfg(windows)]
mod registry {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    /// Default value of a key under HKEY_CURRENT_USER.
    pub fn read_string(key: &str) -> Option<String> {
        let key: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                std::ptr::null(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1);
        let value = String::from_utf16_lossy(&buf[..len]);
        (!value.trim().is_empty()).then_some(value)
    }
}
