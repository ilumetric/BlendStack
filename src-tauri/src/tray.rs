//! System tray icon with quick-launch entries.

use crate::model::{InstalledBuild, Version};
use crate::state::AppState;
use crate::watcher::cycle_label;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

const TRAY_ID: &str = "main";
const QUICK_LAUNCH: usize = 8;

pub fn display_name(b: &InstalledBuild, label: Option<&String>) -> String {
    if let Some(l) = label.filter(|l| !l.trim().is_empty()) {
        return l.clone();
    }
    let cycle = cycle_label(&b.release_cycle);
    let mut name = format!("Blender {}", b.version);
    if !cycle.is_empty() {
        name.push(' ');
        name.push_str(cycle);
    }
    if let Some(p) = &b.patch {
        name.push_str(&format!(" ({p})"));
    }
    name
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        #[cfg(windows)]
        if let Ok(hwnd) = w.hwnd() {
            bring_to_front(hwnd.0);
        }
    }
}

/// Windows refuses to activate a window when another app has the foreground
/// (a file opened from Explorer, the first window after a launch from the
/// shell), and `set_focus` then only flashes the taskbar button. Sharing input
/// with the foreground thread for a moment lifts that lock.
#[cfg(windows)]
fn bring_to_front(hwnd: *mut std::ffi::c_void) {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground == hwnd {
            return;
        }
        let theirs = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
        let ours = GetCurrentThreadId();
        let attached = theirs != 0 && theirs != ours && AttachThreadInput(ours, theirs, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(ours, theirs, 0);
        }
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let mut builds = state.installed();
    // Favorites first, then most recently launched, then newest.
    builds.sort_by(|a, b| {
        let fa = settings.favorites.contains(&a.id);
        let fb = settings.favorites.contains(&b.id);
        let la = settings.last_launched.get(&a.id).copied().unwrap_or(0);
        let lb = settings.last_launched.get(&b.id).copied().unwrap_or(0);
        fb.cmp(&fa)
            .then(lb.cmp(&la))
            .then(Version::parse(&b.version).cmp(&Version::parse(&a.version)))
    });

    let menu = Menu::new(app)?;
    let default = settings
        .default_build
        .as_ref()
        .and_then(|id| builds.iter().find(|b| &b.id == id))
        .or(builds.first());
    if let Some(d) = default {
        let label = format!("Launch {}", display_name(d, settings.labels.get(&d.id)));
        menu.append(&MenuItem::with_id(
            app,
            format!("launch:{}", d.id),
            label,
            true,
            None::<&str>,
        )?)?;
    }
    if builds.len() > 1 {
        let sub = Submenu::new(app, "Launch", true)?;
        for b in builds.iter().take(QUICK_LAUNCH) {
            let star = if settings.favorites.contains(&b.id) {
                "★ "
            } else {
                ""
            };
            let label = format!("{star}{}", display_name(b, settings.labels.get(&b.id)));
            sub.append(&MenuItem::with_id(
                app,
                format!("launch:{}", b.id),
                label,
                true,
                None::<&str>,
            )?)?;
        }
        menu.append(&sub)?;
    }
    if !builds.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "show",
        "Open Blend Stack",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "check",
        "Check for New Builds",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Blend Stack")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "show" => show_main(app),
                "quit" => app.exit(0),
                "check" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::watcher::check(&app).await;
                    });
                }
                _ => {
                    if let Some(build_id) = id.strip_prefix("launch:") {
                        if let Err(e) = crate::commands::launch_by_id(app, build_id, None) {
                            log::error!("tray launch failed: {e}");
                        }
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => log::warn!("tray menu refresh failed: {e}"),
    }
}
