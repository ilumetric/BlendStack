mod commands;
mod download;
mod error;
mod extract;
mod launch;
mod library;
mod model;
mod notifications;
mod recent;
mod settings;
mod sources;
mod state;
mod tray;
mod watcher;
mod window;

use settings::{read_json, AppPaths, Settings};
use state::AppState;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_window_state::StateFlags;

const HIDDEN_FLAG: &str = "--minimized";

/// `.blend` files among command-line arguments (file association / "Open with").
fn blend_files<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| a.to_ascii_lowercase().ends_with(".blend") && std::path::Path::new(a).is_file())
        .collect()
}

pub fn run() {
    let launched_hidden = std::env::args().any(|a| a == HIDDEN_FLAG);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let files = blend_files(argv);
            if !files.is_empty() {
                let state = app.state::<AppState>();
                state.pending_files.lock().unwrap().extend(files);
                let _ = app.emit("files:opened", ());
            }
            tray::show_main(app);
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .build(),
        )
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    StateFlags::all() & !StateFlags::VISIBLE & !StateFlags::DECORATIONS,
                )
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_FLAG]),
        ))
        .setup(move |app| {
            let resolver = app.path();
            let paths = AppPaths {
                config_dir: resolver.app_config_dir()?,
                cache_dir: resolver.app_cache_dir()?,
                data_dir: resolver.app_local_data_dir()?,
            };
            for dir in [&paths.config_dir, &paths.cache_dir, &paths.data_dir] {
                std::fs::create_dir_all(dir)?;
            }
            let settings: Settings = read_json(&paths.settings_file());
            let library_dir = paths.library(&settings);
            let state = AppState::new(
                paths,
                settings.clone(),
                launched_hidden && settings.start_minimized,
            )?;
            state
                .pending_files
                .lock()
                .unwrap()
                .extend(blend_files(std::env::args()));
            app.manage(state);

            std::thread::spawn(move || library::sweep(&library_dir));
            window::apply_style(app.handle(), &settings);
            tray::create(app.handle())?;
            watcher::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.settings().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::app_ready,
            commands::get_system_info,
            commands::update_settings,
            commands::list_installed,
            commands::get_cached_builds,
            commands::get_builds,
            commands::get_cached_news,
            commands::get_news,
            commands::start_download,
            commands::cancel_download,
            commands::clear_downloads,
            commands::list_downloads,
            commands::launch_build,
            commands::delete_build,
            commands::import_build,
            commands::move_library,
            commands::reveal_path,
            commands::open_url,
            commands::get_recent_files,
            commands::get_thumbnail,
            commands::get_notifications,
            commands::mark_notifications_read,
            commands::clear_notifications,
            commands::check_now,
            commands::take_pending_files,
            commands::check_app_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Blend Stack");
}
