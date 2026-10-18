//! Native window styling: theme and translucent backdrop (Mica / vibrancy).

use crate::settings::{Settings, WindowGeometry};
use crate::state::AppState;
use tauri::utils::config::WindowConfig;
use tauri::{AppHandle, Manager, Theme};

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

pub fn translucent(settings: &Settings) -> bool {
    settings.translucency && supports_translucency()
}

pub fn apply_style(app: &AppHandle, settings: &Settings) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.set_theme(theme(settings));

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use tauri::utils::config::WindowEffectsConfig;
        let result = if translucent(settings) {
            window.set_effects(effects())
        } else {
            window.set_effects(None::<WindowEffectsConfig>)
        };
        if let Err(e) = result {
            log::warn!("could not apply window effects: {e}");
        }
    }
}

/// Window config for startup: last size and position, theme and material.
///
/// Creating the WebView takes a few hundred milliseconds. With a translucent
/// material the window is shown at once (the material is its first frame)
/// and the UI paints into it as soon as it is ready. Opaque windows stay
/// hidden until the UI has painted (`app_ready`), so nothing see-through
/// ever shows.
pub fn prepare(
    app: &AppHandle,
    mut config: WindowConfig,
    settings: &Settings,
    hidden: bool,
) -> WindowConfig {
    if let Some(g) = settings.window.filter(|g| on_screen(app, g)) {
        config.x = Some(g.x);
        config.y = Some(g.y);
        config.width = g.width.max(config.min_width.unwrap_or(0.0));
        config.height = g.height.max(config.min_height.unwrap_or(0.0));
        config.center = false;
        config.maximized = g.maximized;
    }
    config.theme = theme(settings);
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    if translucent(settings) {
        config.window_effects = Some(effects());
        config.visible = !hidden;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = hidden;
    config
}

fn on_screen(app: &AppHandle, g: &WindowGeometry) -> bool {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| {
            let scale = m.scale_factor();
            let (p, s) = (m.position(), m.size());
            let cx = ((g.x + 80.0) * scale) as i32;
            let cy = ((g.y + 20.0) * scale) as i32;
            cx >= p.x && cy >= p.y && cx < p.x + s.width as i32 && cy < p.y + s.height as i32
        })
}

fn theme(settings: &Settings) -> Option<Theme> {
    match settings.theme.as_str() {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn effects() -> tauri::utils::config::WindowEffectsConfig {
    use tauri::window::{Effect, EffectState, EffectsBuilder};
    let effect = if cfg!(target_os = "windows") {
        Effect::Mica
    } else {
        Effect::Sidebar
    };
    EffectsBuilder::new()
        .effect(effect)
        .state(EffectState::FollowsWindowActiveState)
        .build()
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
    let scale = window.scale_factor().unwrap_or(1.0);
    if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
        let (pos, size) = (pos.to_logical::<f64>(scale), size.to_logical::<f64>(scale));
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
