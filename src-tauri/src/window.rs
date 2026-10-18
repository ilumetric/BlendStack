//! Native window styling: theme and translucent backdrop (Mica / vibrancy).

use crate::settings::{Settings, WindowGeometry};
use crate::state::AppState;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Theme, WebviewWindow};

/// Whether the OS can render a translucent backdrop behind the UI.
pub fn supports_translucency() -> bool {
    #[cfg(target_os = "windows")]
    {
        // Mica requires Windows 11 (build 22000+).
        windows_version::OsVersion::current().build >= 22000
    }
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

pub fn is_translucent(_app: &AppHandle, settings: &Settings) -> bool {
    settings.translucency && supports_translucency()
}

pub fn apply_style(app: &AppHandle, settings: &Settings) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let theme = match settings.theme.as_str() {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    };
    let _ = window.set_theme(theme);

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::{Effect, EffectState, EffectsBuilder};
        if is_translucent(app, settings) {
            let effect = if cfg!(target_os = "windows") {
                Effect::Mica
            } else {
                Effect::Sidebar
            };
            let effects = EffectsBuilder::new()
                .effect(effect)
                .state(EffectState::FollowsWindowActiveState)
                .build();
            if let Err(e) = window.set_effects(effects) {
                log::warn!("could not apply window effects: {e}");
            }
        } else {
            let _ = window.set_effects(None::<WindowEffectsConfig>);
        }
    }
}

/// Puts the window back where it was, if that spot is still on a screen.
pub fn restore_geometry(window: &WebviewWindow, g: WindowGeometry) {
    let visible = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| {
            let (p, s) = (m.position(), m.size());
            let (cx, cy) = (g.x + 80, g.y + 20);
            cx >= p.x && cy >= p.y && cx < p.x + s.width as i32 && cy < p.y + s.height as i32
        });
    if g.width >= 400 && g.height >= 300 {
        let _ = window.set_size(PhysicalSize::new(g.width, g.height));
    }
    if visible {
        let _ = window.set_position(PhysicalPosition::new(g.x, g.y));
    }
    if g.maximized {
        let _ = window.maximize();
    }
}

/// Tracks the window while it moves; cheap, nothing is written to disk.
pub fn remember_geometry(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_minimized().unwrap_or(true) || !window.is_visible().unwrap_or(false) {
        return;
    }
    let state = app.state::<AppState>();
    let mut current = state.geometry.lock().unwrap();
    let maximized = window.is_maximized().unwrap_or(false);
    if maximized {
        if let Some(g) = current.as_mut() {
            g.maximized = true;
        }
        return;
    }
    if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
        *current = Some(WindowGeometry {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            maximized: false,
        });
    }
}

pub fn save_geometry(app: &AppHandle) {
    remember_geometry(app);
    let state = app.state::<AppState>();
    let geometry = *state.geometry.lock().unwrap();
    if geometry.is_some() && geometry != state.settings().window {
        let _ = state.update_settings(|s| s.window = geometry);
    }
}
