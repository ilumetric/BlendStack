mod chat;
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
#[cfg(windows)]
mod toast;
mod tray;
mod updater;
mod watcher;
mod window;

use settings::{read_json, AppPaths, Settings};
use state::AppState;
use tauri::{Emitter, Manager, RunEvent, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{Target, TargetKind};

const HIDDEN_FLAG: &str = "--minimized";
/// Must match `identifier` in tauri.conf.json.
const IDENTIFIER: &str = "io.github.ilumetric.blendstack";

/// `.blend` files among command-line arguments (file association / "Open with").
fn blend_files<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| a.to_ascii_lowercase().ends_with(".blend") && std::path::Path::new(a).is_file())
        .collect()
}

/// Process start, for startup timing in the log.
pub static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub fn since_start() -> u128 {
    STARTED.get().map(|t| t.elapsed().as_millis()).unwrap_or(0)
}

pub fn run() {
    STARTED.get_or_init(std::time::Instant::now);
    let launched_hidden = std::env::args().any(|a| a == HIDDEN_FLAG);
    // Resolved before anything starts so logs and the WebView profile land
    // in the app's own folder too.
    let paths = AppPaths::resolve(IDENTIFIER);
    if let Err(e) = paths.create_dirs() {
        eprintln!("cannot create data folders: {e}");
    }
    updater::cleanup_previous();

    let app = tauri::Builder::default()
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
                .clear_targets()
                .target(Target::new(TargetKind::Folder {
                    path: paths.log_dir(),
                    file_name: Some("blendstack".into()),
                }))
                .target(Target::new(TargetKind::Stdout))
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
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
            let settings: Settings = read_json(&paths.settings_file());
            let library_dir = paths.library(&settings);

            let hidden = launched_hidden && settings.start_minimized;
            let state = AppState::new(paths.clone(), settings.clone(), hidden)?;
            state
                .pending_files
                .lock()
                .unwrap()
                .extend(blend_files(std::env::args()));
            app.manage(state);

            // The window is created here rather than from the config so its
            // WebView profile can live next to the app data, and so it opens
            // with its final size, theme and material right away.
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .ok_or_else(|| error::msg("main window missing from tauri.conf.json"))?;
            let config = window::prepare(app.handle(), config, &settings, hidden);
            // Look hints for public/boot.js, so the first frame is already right.
            let hints = serde_json::json!({
                "os": std::env::consts::OS,
                "translucent": window::translucent(&settings),
                "theme": settings.theme,
            });
            let mut builder = WebviewWindowBuilder::from_config(app.handle(), &config)?
                .initialization_script(format!("window.__BLENDSTACK__ = {hints};"));
            if paths.root.is_some() {
                builder = builder.data_directory(paths.webview_dir());
            }
            builder.build()?;

            std::thread::spawn(move || library::sweep(&library_dir));
            tray::create(app.handle())?;
            #[cfg(windows)]
            {
                let handle = app.handle().clone();
                toast::init(move || tray::show_main(&handle));
            }
            watcher::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                window::save_geometry(window.app_handle());
                let state = window.state::<AppState>();
                if state.settings().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                window::remember_geometry(window.app_handle());
            }
            _ => {}
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
            chat::chat_show,
            chat::chat_hide,
            chat::chat_reload,
            updater::check_app_update,
            updater::install_app_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while starting Blend Stack");

    app.run(|handle, event| {
        if let RunEvent::ExitRequested { .. } = event {
            window::save_geometry(handle);
            #[cfg(windows)]
            toast::clear();
        }
    });
}
