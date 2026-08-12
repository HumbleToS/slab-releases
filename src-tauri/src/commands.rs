//! Tauri IPC surface. Thin handlers only — logic lives in the sibling modules.

use tauri::{AppHandle, Manager};

use crate::media::{self, Control};

/// The settings panel writes one theme key; the config watcher then applies
/// it like any hand edit — no direct state mutation from IPC. Async so the
/// file I/O never runs on the main thread.
#[tauri::command]
pub async fn set_theme(
    app: AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
        crate::config::set_theme_value(&dir, &key, &value).inspect_err(|e| {
            log::warn!("set_theme {key}: {e}");
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Widget management from the customization window: add/remove/reorder write
/// [[widget]] tables through config.toml (like set_theme, the watcher then
/// applies the change). Param editing stays a config-file affair until M3.
#[tauri::command]
pub async fn widget_add(app: AppHandle, kind: String) -> Result<(), String> {
    widget_op(app, move |dir| crate::config::add_widget(dir, &kind)).await
}

#[tauri::command]
pub async fn widget_remove(app: AppHandle, index: usize) -> Result<(), String> {
    widget_op(app, move |dir| crate::config::remove_widget(dir, index)).await
}

#[tauri::command]
pub async fn widget_move(app: AppHandle, index: usize, up: bool) -> Result<(), String> {
    widget_op(app, move |dir| crate::config::move_widget(dir, index, up)).await
}

async fn widget_op<F>(app: AppHandle, op: F) -> Result<(), String>
where
    F: FnOnce(&std::path::Path) -> Result<(), String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
        op(&dir).inspect_err(|e| log::warn!("widget op: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The webview announces it has loaded and wants the full current state.
#[tauri::command]
pub fn frontend_ready(app: AppHandle) {
    crate::push_ui_state(&app);
    crate::replay_caches(&app);
}

/// Webview JS errors surface in Slab.log — the dashboard has no devtools.
#[tauri::command]
pub fn frontend_log(window: tauri::WebviewWindow, message: String) {
    log::error!("webview {:?}: {message}", window.label());
}

/// Backgrounds available to the customization window's selector. Async and
/// off the runtime threads: scanning a big Wallpaper Engine library is heavy
/// disk I/O, and a sync command would block the main thread — freezing every
/// window's rendering while it walks the folder tree.
#[tauri::command]
pub async fn list_backgrounds(app: AppHandle) -> Vec<crate::theme::BackgroundOption> {
    tauri::async_runtime::spawn_blocking(move || crate::theme::list_backgrounds(&app))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn media_play_pause(app: AppHandle) {
    media::control(&app, Control::PlayPause);
}

#[tauri::command]
pub fn media_next(app: AppHandle) {
    media::control(&app, Control::Next);
}

#[tauri::command]
pub fn media_prev(app: AppHandle) {
    media::control(&app, Control::Prev);
}

/// Launch a shortcut widget's URI (app protocol, file, or URL) through the
/// Windows shell, exactly as the user configured it.
#[tauri::command]
pub fn open_shortcut(uri: String) {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let uri = uri.trim().to_string();
    if uri.is_empty() {
        log::warn!("shortcut with empty uri ignored");
        return;
    }
    log::info!("opening shortcut {uri}");
    let result = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(uri.as_str()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // Per the ShellExecute contract, values <= 32 are error codes.
    if result.0 as usize <= 32 {
        log::warn!("shell could not open {uri} (code {})", result.0 as usize);
    }
}
