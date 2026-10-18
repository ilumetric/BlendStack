//! Native window styling: theme and translucent backdrop (Mica / vibrancy).

use crate::settings::Settings;
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
