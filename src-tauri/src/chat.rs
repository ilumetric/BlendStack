//! The Chat tab: Blender Chat (chat.blender.org) in a WebView of its own.
//!
//! The site forbids being framed, so it cannot live in an iframe. Instead a
//! second WebView is placed edge to edge over the tab's content area; the UI
//! reports the area's position. It is created on first use and kept alive
//! (hidden) so the session survives switching tabs, and it shares the app's
//! WebView profile, so the login is remembered. The page gets no access to
//! the app's commands.
//!
//! Signing in happens in the default browser: Blender Chat only offers
//! single sign-on with Blender ID, so when the page starts that redirect the
//! app opens it in the browser instead, with a loopback address as the
//! return URL. The homeserver sends a one-time login token there, which the
//! embedded page then uses to finish the sign-in — no password is typed
//! into the app.
//!
//! The commands are async on purpose: on Windows a WebView created from a
//! synchronous command (which runs on the main thread) deadlocks.

use crate::error::{msg, Result};
use crate::state::AppState;
use std::time::Duration;
use tauri::webview::{NewWindowResponse, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Url, WebviewUrl};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const URL: &str = "https://chat.blender.org/";
const HOMESERVER: &str = "https://matrix.blender.org";
const LABEL: &str = "chat";
/// How long to wait for the browser to come back with a login token.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

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
    let navigator = app.clone();
    let mut builder = WebviewBuilder::new(LABEL, WebviewUrl::External(URL.parse().unwrap()))
        // Links that would open a new window go to the default browser.
        .on_new_window(move |url, _features| {
            let _ = opener.opener().open_url(url.as_str(), None::<&str>);
            NewWindowResponse::Deny
        })
        // Single sign-on goes through the default browser instead.
        .on_navigation(move |url| {
            if !is_sso_redirect(url) {
                return true;
            }
            let (app, url) = (navigator.clone(), url.clone());
            tauri::async_runtime::spawn(async move {
                if let Err(e) = sign_in_in_browser(&app, url).await {
                    log::warn!("chat sign-in failed: {e}");
                }
            });
            false
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

fn is_sso_redirect(url: &Url) -> bool {
    url.as_str().starts_with(HOMESERVER) && url.path().contains("/login/sso/redirect")
}

async fn sign_in_in_browser(app: &AppHandle, sso: Url) -> Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let callback = format!("http://127.0.0.1:{}/sso", listener.local_addr()?.port());

    // Same request, but the homeserver returns the token to us.
    let mut return_to = URL.to_string();
    let mut external = sso.clone();
    {
        let pairs: Vec<(String, String)> = sso
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let mut query = external.query_pairs_mut();
        query.clear();
        for (k, v) in pairs {
            if k == "redirectUrl" {
                return_to = v;
                query.append_pair(&k, &callback);
            } else {
                query.append_pair(&k, &v);
            }
        }
    }
    log::info!("chat sign-in opened in the browser, waiting on {callback}");
    app.opener()
        .open_url(external.as_str(), None::<&str>)
        .map_err(|e| msg(e.to_string()))?;

    let token = tokio::time::timeout(SIGN_IN_TIMEOUT, wait_for_token(&listener))
        .await
        .map_err(|_| msg("timed out waiting for the browser sign-in"))??;

    // Finish in the embedded page exactly as if the redirect had happened there.
    let mut target = Url::parse(&return_to).unwrap_or_else(|_| URL.parse().unwrap());
    target.query_pairs_mut().append_pair("loginToken", &token);
    if let Some(chat) = app.get_webview(LABEL) {
        chat.eval(format!(
            "localStorage.setItem('mx_sso_hs_url', {hs}); location.replace({target});",
            hs = serde_json::to_string(HOMESERVER)?,
            target = serde_json::to_string(target.as_str())?,
        ))?;
    }
    crate::tray::show_main(app);
    let _ = app.emit("chat:signed-in", ());
    Ok(())
}

/// Minimal HTTP endpoint: waits for `/sso?loginToken=…` and answers with a page.
async fn wait_for_token(listener: &tokio::net::TcpListener) -> Result<String> {
    loop {
        let (mut socket, _) = listener.accept().await?;
        let mut buf = vec![0u8; 16 * 1024];
        let n = socket.read(&mut buf).await?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let token = Url::parse(&format!("http://127.0.0.1{path}"))
            .ok()
            .and_then(|u| {
                u.query_pairs()
                    .find(|(k, _)| k == "loginToken")
                    .map(|(_, v)| v.into_owned())
            });
        let (status, body) = match &token {
            Some(_) => ("200 OK", SIGNED_IN_PAGE),
            None => ("404 Not Found", ""),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;
        if let Some(token) = token {
            return Ok(token);
        }
    }
}

const SIGNED_IN_PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Blend Stack</title>
<style>
  body{margin:0;height:100vh;display:grid;place-items:center;background:#1c1c1e;color:#f5f5f7;
       font:15px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI Variable Text","Segoe UI",sans-serif}
  main{text-align:center}
  h1{font-size:22px;margin:0 0 8px;letter-spacing:-.02em}
  p{margin:0;color:#a1a1a6}
</style></head>
<body><main>
  <h1>Signed in to Blender Chat</h1>
  <p>Return to Blend Stack — you can close this tab.</p>
  <p>Вход выполнен — вернитесь в Blend Stack, эту вкладку можно закрыть.</p>
</main></body></html>"#;
