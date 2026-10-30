//! .blend files, and the icon of Blender's taskbar button on Windows.
//!
//! Blend Stack can open .blend files itself (Settings → Launching Blender):
//! it registers its own file type and picks the build for each file it is
//! given, see [`crate::launch::build_for_file`]. Windows only lets the user
//! make it the default: it asks the next time a .blend file is opened, or
//! the user picks it in Settings → Default apps.
//!
//! Blender gives its windows the AppUserModelId `blender.X.Y`. Windows takes
//! the taskbar icon from the shortcuts it keeps for that id (pinned and
//! "implicit" taskbar shortcuts), and Blender's own registration
//! (`blender --register`) adds a file type of the same name. Once the build
//! they point to is updated or deleted, the button shows a blank icon. So
//! whenever Blend Stack starts Blender, and after the library changes, the
//! broken ones are pointed at an installed build of the same series. Nothing
//! is created that was not there before.
//!
//! On Linux the build menu runs Blender's own `--register` instead.

use crate::error::{msg, Result};
use crate::model::InstalledBuild;
use crate::state::AppState;
use serde::Serialize;
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

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlendHandler {
    /// Blend Stack is registered for .blend files.
    pub registered: bool,
    /// Windows opens .blend files with Blend Stack (the user confirmed it).
    pub active: bool,
}

#[tauri::command]
pub fn blend_handler() -> BlendHandler {
    #[cfg(windows)]
    {
        win::handler()
    }
    #[cfg(not(windows))]
    {
        BlendHandler::default()
    }
}

/// Registers Blend Stack for .blend files, or gives them back to the app
/// they had before.
#[tauri::command]
pub async fn set_blend_handler(app: AppHandle, enabled: bool) -> Result<BlendHandler> {
    #[cfg(windows)]
    {
        let installed = app.state::<AppState>().installed();
        tauri::async_runtime::spawn_blocking(move || {
            if enabled {
                win::enable(&installed)
            } else {
                win::disable()
            }
        })
        .await
        .map_err(|e| msg(e.to_string()))??;
        Ok(win::handler())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, enabled);
        Err(msg("not supported on this system"))
    }
}

/// Windows' Default apps page for Blend Stack, where .blend files can be given to it.
#[tauri::command]
pub fn open_default_apps(app: AppHandle) -> Result<()> {
    #[cfg(windows)]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(win::DEFAULT_APPS_URL, None::<&str>)
            .map_err(|e| msg(e.to_string()))
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(msg("not supported on this system"))
    }
}

/// Blender series that saved a .blend file, read from its header.
#[tauri::command]
pub fn file_series(path: String) -> Option<String> {
    crate::recent::blend_series(Path::new(&path))
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
    win::notify_shell();
    Ok(())
}

/// Before `build` starts: points the taskbar shortcuts and Blender's file
/// type for its series at it when their build is gone.
pub fn on_launch(build: &InstalledBuild) {
    #[cfg(windows)]
    win::on_launch(build);
    #[cfg(not(windows))]
    let _ = build;
}

/// Points everything registered for Blender or Blend Stack whose program is
/// gone at an installed build. Returns true when it changed anything.
pub fn repair(installed: &[InstalledBuild]) -> bool {
    #[cfg(windows)]
    {
        win::repair(installed)
    }
    #[cfg(not(windows))]
    {
        let _ = installed;
        false
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

#[cfg(windows)]
mod win {
    use super::{reg, shortcuts, BlendHandler};
    use crate::error::{msg, Result};
    use crate::launch::best_in_series;
    use crate::model::{series, Channel, InstalledBuild, Version};
    use std::path::{Path, PathBuf};

    /// Blend Stack's file type for .blend files.
    const PROG_ID: &str = "BlendStack.blend";
    /// Value under [`PROG_ID`] with the file type .blend had before, restored when turned off.
    const PREVIOUS: &str = "BlendStack.Previous";
    /// Where Windows keeps the app picked in "Open with → Always".
    const FILE_EXTS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.blend";
    const USER_CHOICES: [&str; 2] = ["UserChoice", "UserChoiceLatest"];
    /// Settings → Default apps lists Blend Stack under this name.
    const APP_NAME: &str = "Blend Stack";
    const CAPABILITIES: &str = r"Software\BlendStack\Capabilities";
    const REGISTERED_APPS: &str = r"Software\RegisteredApplications";
    pub const DEFAULT_APPS_URL: &str = "ms-settings:defaultapps?registeredAppUser=Blend%20Stack";
    /// Blender's Explorer thumbnail handler, registered by `blender --register`.
    const THUMBNAILER: &str = r"CLSID\{D45F043D-F17F-4e8a-8435-70971D9FA46D}\InProcServer32";

    fn class(key: &str) -> String {
        format!(r"Software\Classes\{key}")
    }

    fn own_exe() -> Option<PathBuf> {
        std::env::current_exe().ok()
    }

    pub fn handler() -> BlendHandler {
        let registered =
            reg::read(&class(&format!(r"{PROG_ID}\shell\open\command")), None).is_some();
        BlendHandler {
            registered,
            active: registered && opens_blend_files(),
        }
    }

    /// Whether Windows runs this executable for .blend files, as Explorer resolves it.
    fn opens_blend_files() -> bool {
        use windows_sys::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_NONE, ASSOCSTR_EXECUTABLE};
        let Some(exe) = own_exe() else {
            return false;
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let (ext, verb) = (reg::wide(".blend"), reg::wide("open"));
        let hr = unsafe {
            AssocQueryStringW(
                ASSOCF_NONE,
                ASSOCSTR_EXECUTABLE,
                ext.as_ptr(),
                verb.as_ptr(),
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        hr == 0 && same_file(Path::new(&String::from_utf16_lossy(&buf[..end])), &exe)
    }

    /// `Applications\Blend Stack.exe`: what "Open with" stores for the exe itself.
    fn is_app_key(prog_id: &str, exe: &Path) -> bool {
        exe.file_name().is_some_and(|name| {
            prog_id.eq_ignore_ascii_case(&format!(r"Applications\{}", name.to_string_lossy()))
        })
    }

    pub fn enable(installed: &[InstalledBuild]) -> Result<()> {
        let exe = own_exe().ok_or_else(|| msg("cannot find the Blend Stack executable"))?;
        let ext = class(".blend");
        let key = class(PROG_ID);
        let previous = reg::read(&ext, None).filter(|p| p != PROG_ID);
        reg::write(&key, None, "Blender File")?;
        if let Some(p) = &previous {
            reg::write(&key, Some(PREVIOUS), p)?;
        }
        reg::write(
            &format!(r"{key}\DefaultIcon"),
            None,
            &file_icon(installed, &exe),
        )?;
        reg::write(
            &format!(r"{key}\shell\open"),
            Some("FriendlyAppName"),
            "Blend Stack",
        )?;
        reg::write(
            &format!(r"{key}\shell\open\command"),
            None,
            &open_command(&exe),
        )?;
        reg::write_empty(&format!(r"{ext}\OpenWithProgids"), PROG_ID)?;
        reg::write(&ext, None, PROG_ID)?;
        // For Settings → Default apps.
        reg::write(CAPABILITIES, Some("ApplicationName"), APP_NAME)?;
        reg::write(
            CAPABILITIES,
            Some("ApplicationDescription"),
            "Opens .blend files in the Blender version they were saved with.",
        )?;
        reg::write(
            &format!(r"{CAPABILITIES}\FileAssociations"),
            Some(".blend"),
            PROG_ID,
        )?;
        reg::write(REGISTERED_APPS, Some(APP_NAME), CAPABILITIES)?;
        notify_shell();
        Ok(())
    }

    pub fn disable() -> Result<()> {
        let key = class(PROG_ID);
        let previous = reg::read(&key, Some(PREVIOUS)).filter(|p| {
            reg::read(&class(&format!(r"{p}\shell\open\command")), None)
                .is_some_and(|c| program_exists(&c))
        });
        reg::delete_tree(&key);
        reg::delete_value(REGISTERED_APPS, APP_NAME);
        reg::delete_tree(r"Software\BlendStack");
        let ext = class(".blend");
        reg::delete_value(&format!(r"{ext}\OpenWithProgids"), PROG_ID);
        if reg::read(&ext, None).as_deref() == Some(PROG_ID) {
            reg::write(&ext, None, previous.as_deref().unwrap_or(""))?;
        }
        // The choice made in "Open with" would point at a file type that is gone.
        let exe = own_exe();
        for k in USER_CHOICES {
            let k = format!(r"{FILE_EXTS}\{k}");
            if reg::read(&k, Some("ProgId"))
                .is_some_and(|c| c == PROG_ID || exe.as_deref().is_some_and(|e| is_app_key(&c, e)))
            {
                reg::delete_key(&k);
            }
        }
        notify_shell();
        Ok(())
    }

    pub fn on_launch(build: &InstalledBuild) {
        let Some(launcher) = launcher(build) else {
            return;
        };
        if fix_app_ids(&[(app_id(&build.version), launcher)]) {
            log::info!(
                "pointed Blender {} taskbar entries at {}",
                series(&build.version),
                build.path
            );
            notify_shell();
        }
    }

    pub fn repair(installed: &[InstalledBuild]) -> bool {
        let mut changed = false;

        // Blend Stack's own file type, after the app folder or the build its icon comes from moved.
        let key = class(PROG_ID);
        let command_key = format!(r"{key}\shell\open\command");
        if let (Some(command), Some(exe)) = (reg::read(&command_key, None), own_exe()) {
            if !program_exists(&command) {
                changed |= reg::write(&command_key, None, &open_command(&exe)).is_ok();
            }
            let icon_key = format!(r"{key}\DefaultIcon");
            let icon = reg::read(&icon_key, None).unwrap_or_default();
            let wanted = file_icon(installed, &exe);
            let own_icon = program(&icon).is_some_and(|p| same_file(&p, &exe));
            if icon != wanted && (!program_exists(&icon) || own_icon) {
                changed |= reg::write(&icon_key, None, &wanted).is_ok();
            }
        }

        // Blender's own file types and taskbar shortcuts, one per series.
        let mut targets: Vec<(String, PathBuf)> = Vec::new();
        for b in installed {
            let id = app_id(&b.version);
            if targets.iter().any(|(t, _)| *t == id) {
                continue;
            }
            let best = best_in_series(installed, &series(&b.version), None);
            if let Some(launcher) = best.and_then(launcher) {
                targets.push((id, launcher));
            }
        }
        changed |= fix_app_ids(&targets);

        // Explorer thumbnails for .blend files come from a Blender build too.
        let thumbnailer = class(THUMBNAILER);
        if let Some(dll) = reg::read(&thumbnailer, None) {
            if !Path::new(&dll).is_file() {
                // Newer handlers read files from older versions too.
                let newest = installed
                    .iter()
                    .map(|b| (Version::parse(&b.version), thumbnailer_dll(b)))
                    .filter(|(_, dll)| dll.is_file())
                    .max_by(|a, b| a.0.cmp(&b.0));
                if let Some((_, dll)) = newest {
                    changed |= reg::write(&thumbnailer, None, &dll.to_string_lossy()).is_ok();
                }
            }
        }

        if changed {
            log::info!("repaired .blend registrations");
            notify_shell();
        }
        changed
    }

    /// Points Blender's file type and the taskbar shortcuts of each app id
    /// whose program is gone at the given launcher.
    fn fix_app_ids(targets: &[(String, PathBuf)]) -> bool {
        let mut changed = false;
        for (id, launcher) in targets {
            let key = class(id);
            let command_key = format!(r"{key}\shell\open\command");
            if reg::read(&command_key, None).is_some_and(|c| !program_exists(&c)) {
                let quoted = format!("\"{}\"", launcher.display());
                let command = reg::write(&command_key, None, &format!("{quoted} \"%1\""));
                let icon = reg::write(
                    &format!(r"{key}\DefaultIcon"),
                    None,
                    &format!("{quoted}, 1"),
                );
                changed |= command.is_ok() && icon.is_ok();
            }
        }
        shortcuts::retarget(targets) || changed
    }

    /// Blender's AppUserModelId for a version: `blender.5.2`.
    fn app_id(version: &str) -> String {
        format!("blender.{}", series(version))
    }

    /// `blender-launcher.exe` starts Blender without a console; Blender registers it everywhere.
    fn launcher(build: &InstalledBuild) -> Option<PathBuf> {
        let launcher = Path::new(&build.executable).with_file_name("blender-launcher.exe");
        launcher.is_file().then_some(launcher)
    }

    fn thumbnailer_dll(build: &InstalledBuild) -> PathBuf {
        Path::new(&build.executable).with_file_name("BlendThumb.dll")
    }

    fn open_command(exe: &Path) -> String {
        format!("\"{}\" \"%1\"", exe.display())
    }

    /// Blender's file icon, from a stable build when there is one, else Blend Stack's own.
    fn file_icon(installed: &[InstalledBuild], exe: &Path) -> String {
        installed
            .iter()
            .filter_map(|b| {
                let stable = b.channel == Channel::Stable;
                Some(((stable, Version::parse(&b.version)), launcher(b)?))
            })
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, launcher)| format!("\"{}\", 1", launcher.display()))
            .unwrap_or_else(|| format!("\"{}\", 0", exe.display()))
    }

    /// The program in a shell command or icon location:
    /// `"C:\…\app.exe" "%1"` or `"C:\…\app.exe", 1`.
    fn program(value: &str) -> Option<PathBuf> {
        let value = value.trim();
        let path = match value.strip_prefix('"') {
            Some(rest) => rest.split('"').next()?,
            None => value.split(',').next()?.trim(),
        };
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    fn program_exists(value: &str) -> bool {
        program(value).is_some_and(|p| p.is_file())
    }

    fn same_file(a: &Path, b: &Path) -> bool {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }

    /// Tells Explorer that file associations changed so icons refresh at once.
    pub fn notify_shell() {
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

    #[cfg(test)]
    mod tests {
        use super::program;
        use std::path::PathBuf;

        #[test]
        fn reads_program_from_commands_and_icons() {
            let exe = Some(PathBuf::from(r"C:\Blender 5.2\blender-launcher.exe"));
            assert_eq!(
                program(r#""C:\Blender 5.2\blender-launcher.exe" "%1""#),
                exe
            );
            assert_eq!(program(r#""C:\Blender 5.2\blender-launcher.exe", 1"#), exe);
            assert_eq!(program(r"C:\Blender 5.2\blender-launcher.exe,1"), exe);
            assert_eq!(program("  "), None);
        }
    }
}

/// Values under HKEY_CURRENT_USER.
#[cfg(windows)]
mod reg {
    use crate::error::{msg, Result};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW,
        RegGetValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_NONE,
        REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE, RRF_RT_REG_SZ,
    };

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn name_ptr(name: &Option<Vec<u16>>) -> *const u16 {
        name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr())
    }

    /// A string value (`None`: the key's default value); empty values read as missing.
    pub fn read(key: &str, value: Option<&str>) -> Option<String> {
        let key = wide(key);
        let value = value.map(wide);
        let get = |buf: *mut u16, size: &mut u32| unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name_ptr(&value),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.cast(),
                size,
            )
        };
        let mut size = 0u32;
        if get(std::ptr::null_mut(), &mut size) != 0 {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2).max(1)];
        if get(buf.as_mut_ptr(), &mut size) != 0 {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1).min(buf.len());
        let text = String::from_utf16_lossy(&buf[..len]);
        (!text.trim().is_empty()).then_some(text)
    }

    fn set(key: &str, value: Option<&str>, kind: REG_VALUE_TYPE, data: &[u16]) -> Result<()> {
        let key_w = wide(key);
        let value = value.map(wide);
        let mut hkey: HKEY = std::ptr::null_mut();
        let mut status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key_w.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                std::ptr::null(),
                &mut hkey,
                std::ptr::null_mut(),
            )
        };
        if status == 0 {
            status = unsafe {
                RegSetValueExW(
                    hkey,
                    name_ptr(&value),
                    0,
                    kind,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
            unsafe { RegCloseKey(hkey) };
        }
        if status != 0 {
            return Err(msg(format!(
                "could not write to the registry ({key}): error {status}"
            )));
        }
        Ok(())
    }

    pub fn write(key: &str, value: Option<&str>, data: &str) -> Result<()> {
        set(key, value, REG_SZ, &wide(data))
    }

    /// A value without data, as in `OpenWithProgids`.
    pub fn write_empty(key: &str, value: &str) -> Result<()> {
        set(key, Some(value), REG_NONE, &[])
    }

    pub fn delete_value(key: &str, value: &str) {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(key).as_ptr(), wide(value).as_ptr()) };
    }

    /// A key without subkeys; also works where its values may not be changed (UserChoice).
    pub fn delete_key(key: &str) {
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(key).as_ptr()) };
    }

    pub fn delete_tree(key: &str) {
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(key).as_ptr()) };
    }
}

/// Shortcuts Windows keeps for the taskbar (pinned, Start, and the implicit
/// ones behind a running app's button), retargeted the way Blender's own
/// registration does it.
#[cfg(windows)]
mod shortcuts {
    use std::path::{Path, PathBuf};
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PropVariantToStringAlloc,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READWRITE,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{
        FOLDERID_ImplicitAppShortcuts, IShellLinkW, SHGetKnownFolderPath, ShellLink,
        KF_FLAG_DEFAULT,
    };

    /// Points the shortcuts of each app id whose program is gone at its launcher.
    pub fn retarget(targets: &[(String, PathBuf)]) -> bool {
        if targets.is_empty() {
            return false;
        }
        let mut files = Vec::new();
        if let Some(root) = pinned_root() {
            collect(&root, 3, &mut files);
        }
        if files.is_empty() {
            return false;
        }
        unsafe {
            let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let mut changed = false;
            for file in &files {
                match fix(file, targets) {
                    Ok(true) => changed = true,
                    Ok(false) => {}
                    Err(e) => log::debug!("skipped shortcut {}: {e}", file.display()),
                }
            }
            if com.is_ok() {
                CoUninitialize();
            }
            changed
        }
    }

    /// `…\Quick Launch\User Pinned`, which holds TaskBar, StartMenu and ImplicitAppShortcuts.
    fn pinned_root() -> Option<PathBuf> {
        let path =
            unsafe { SHGetKnownFolderPath(&FOLDERID_ImplicitAppShortcuts, KF_FLAG_DEFAULT, None) }
                .ok()?;
        let dir = unsafe { path.to_string() }.ok();
        unsafe { CoTaskMemFree(Some(path.0 as _)) };
        Some(PathBuf::from(dir?).parent()?.to_path_buf())
    }

    fn collect(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                if depth > 0 {
                    collect(&path, depth - 1, out);
                }
            } else if path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("lnk"))
            {
                out.push(path);
            }
        }
    }

    fn until_nul(buf: &[u16]) -> String {
        String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
    }

    unsafe fn fix(file: &Path, targets: &[(String, PathBuf)]) -> windows::core::Result<bool> {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let persist: IPersistFile = link.cast()?;
        persist.Load(&HSTRING::from(file.as_os_str()), STGM_READWRITE)?;
        let store: IPropertyStore = link.cast()?;
        let mut value = store.GetValue(&PKEY_AppUserModel_ID)?;
        let id = PropVariantToStringAlloc(&value).ok().map(|p| {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as _));
            s
        });
        let _ = PropVariantClear(&mut value);
        let Some((_, launcher)) = targets.iter().find(|(t, _)| Some(t) == id.as_ref()) else {
            return Ok(false);
        };

        let mut path = [0u16; 1024];
        link.GetPath(&mut path, std::ptr::null_mut(), 0)?;
        let target = until_nul(&path);
        if !target.is_empty() && Path::new(&target).is_file() {
            return Ok(false);
        }
        let launcher = HSTRING::from(launcher.as_os_str());
        link.SetPath(&launcher)?;
        let mut icon = [0u16; 1024];
        let mut index = 0;
        if link.GetIconLocation(&mut icon, &mut index).is_ok() {
            let icon = until_nul(&icon);
            if !icon.is_empty() && !Path::new(&icon).is_file() {
                link.SetIconLocation(&launcher, 0)?;
            }
        }
        persist.Save(PCWSTR::null(), true)?;
        log::info!("pointed shortcut {} at {launcher}", file.display());
        Ok(true)
    }
}
