//! Which Blender opens .blend files.
//!
//! Uses Blender's own registration (`blender --register`), which writes a
//! per-user file type named after the series (`blender.5.2`). Besides the
//! .blend association it carries the AppUserModelId Blender gives its
//! windows, which is where Windows takes the taskbar icon from: without a
//! registration (for example after another launcher unregistered Blender)
//! the taskbar shows a blank icon.

use crate::error::{msg, Result};
use crate::state::AppState;
use std::path::PathBuf;
use std::process::Command;
use tauri::State;

#[tauri::command]
pub async fn register_blend_files(state: State<'_, AppState>, id: String) -> Result<()> {
    let build = state
        .find_installed(&id)
        .ok_or_else(|| msg("build not found"))?;
    let exe = PathBuf::from(&build.executable);
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let mut cmd = Command::new(&exe);
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
    })
    .await
    .map_err(|e| msg(e.to_string()))?
}

/// Folder of the Blender build that currently opens .blend files, if any.
#[tauri::command]
pub fn blend_association() -> Option<String> {
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
        PathBuf::from(exe)
            .parent()
            .map(|dir| dir.to_string_lossy().into_owned())
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
