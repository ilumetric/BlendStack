//! The Chat tab: Blender Chat (chat.blender.org) in a WebView of its own.
//!
//! The site forbids being framed, so it cannot live in an iframe. Instead a
//! second WebView is placed over the tab's content area; the UI reports the
//! area's position. It is created on first use and kept alive (hidden) so the
//! session and scroll position survive switching tabs. It shares the app's
//! WebView profile, so the login is remembered — next to the app when
//! self-contained. The page gets no access to the app's commands.
//!
//! The commands are async on purpose: on Windows a WebView created from a
//! synchronous command (which runs on the main thread) deadlocks.

use crate::error::{msg, Result};
use crate::state::AppState;
use tauri::webview::{NewWindowResponse, WebviewBuilder};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, State, WebviewUrl};
use tauri_plugin_opener::OpenerExt;

pub const URL: &str = "https://chat.blender.org/";
const LABEL: &str = "chat";

#[tauri::command]
pub async fn chat_show(
    app: AppHandle,
    state: State<'_, AppState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<()> {
    let position = LogicalPosition::new(x, y);
    let size = LogicalSize::new(width.max(1.0), height.max(1.0));
    if let Some(chat) = app.get_webview(LABEL) {
        chat.set_position(position)?;
        chat.set_size(size)?;
        chat.show()?;
        return Ok(());
    }

    let window = app
        .get_window("main")
        .ok_or_else(|| msg("main window not found"))?;
    let opener = app.clone();
    let mut builder = WebviewBuilder::new(LABEL, WebviewUrl::External(URL.parse().unwrap()))
        // Links that would open a new window go to the default browser.
        .on_new_window(move |url, _features| {
            let _ = opener.opener().open_url(url.as_str(), None::<&str>);
            NewWindowResponse::Deny
        });
    if state.paths.root.is_some() {
        builder = builder.data_directory(state.paths.webview_dir());
    }
    window.add_child(builder, position, size)?;
    Ok(())
}

#[tauri::command]
pub async fn chat_hide(app: AppHandle) -> Result<()> {
    if let Some(chat) = app.get_webview(LABEL) {
        chat.hide()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_reload(app: AppHandle) -> Result<()> {
    if let Some(chat) = app.get_webview(LABEL) {
        chat.navigate(URL.parse().unwrap())?;
    }
    Ok(())
}
