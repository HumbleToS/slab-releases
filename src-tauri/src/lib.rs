mod commands;
mod config;
mod display;
mod media;
mod stats;
mod theme;
mod weather;

use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;

pub(crate) struct AppState {
    pub config: Mutex<config::Config>,
    pub weather: Mutex<Vec<weather::WeatherUpdate>>,
    weather_refresh: tokio::sync::mpsc::UnboundedSender<()>,
}

/// Lock that shrugs off poisoning: a panicked background thread must never
/// take the rest of the app down with it.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Swap in a freshly loaded config, sync side effects, and push state.
pub(crate) fn apply_config(app: &AppHandle, new_config: config::Config) {
    sync_autostart(app, new_config.autostart);
    {
        let state = app.state::<AppState>();
        *lock(&state.config) = new_config;
    }
    push_ui_state(app);
    let state = app.state::<AppState>();
    if state.weather_refresh.send(()).is_err() {
        log::warn!("weather refresh channel closed");
    }
}

/// Emit the current resolved theme + widget list as `config-update`.
pub(crate) fn push_ui_state(app: &AppHandle) {
    let config = lock(&app.state::<AppState>().config).clone();
    let payload = theme::ui_state(app, &config);
    if let Err(e) = app.emit("config-update", &payload) {
        log::warn!("could not emit config-update: {e}");
    }
}

/// Re-emit the cached weather and media state — used when the webview
/// (re)loads so it doesn't wait out the next natural update.
pub(crate) fn replay_caches(app: &AppHandle) {
    let state = app.state::<AppState>();
    for update in lock(&state.weather).iter() {
        if let Err(e) = app.emit("weather-update", update) {
            log::warn!("could not replay weather-update: {e}");
        }
    }
    for update in lock(&app.state::<stats::StatsState>().last).iter() {
        if let Err(e) = app.emit("stats-update", update) {
            log::warn!("could not replay stats-update: {e}");
        }
    }
    let last = lock(&app.state::<media::MediaState>().last).clone();
    if let Err(e) = app.emit("media-update", &last) {
        log::warn!("could not replay media-update: {e}");
    }
}

/// Align the OS launch-on-login registration with the config flag.
fn sync_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if let Ok(current) = autolaunch.is_enabled() {
        if current == enabled {
            return;
        }
    }
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    match result {
        Ok(()) => log::info!("autostart {}", if enabled { "enabled" } else { "disabled" }),
        Err(e) => log::warn!("could not update autostart: {e}"),
    }
}

/// Slab lives in the tray like Wallpaper Engine: Open brings the
/// customization window back, Quit is the only way the panel goes away.
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let open = MenuItem::with_id(app, "open", "Open Slab", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Slab", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("slab").menu(&menu).tooltip("Slab");
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| match event.id().as_ref() {
        "open" => show_settings(app),
        "quit" => app.exit(0),
        _ => {}
    })
    .build(app)?;
    Ok(())
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Check the release feed now and every six hours; install silently and
/// restart when a newer version exists. Zero-friction pillar: the case screen
/// keeps itself current, nobody ships files around. Approved network call
/// (maintainer-approved 2026-08-10) alongside Open-Meteo.
fn start_updater(app: &AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match app.updater() {
                Ok(updater) => match updater.check().await {
                    Ok(Some(update)) => {
                        log::info!("update {} available; downloading", update.version);
                        match update.download_and_install(|_, _| {}, || {}).await {
                            Ok(()) => {
                                log::info!("update installed; restarting");
                                app.restart();
                            }
                            Err(e) => log::warn!("update install failed: {e}"),
                        }
                    }
                    Ok(None) => log::debug!("no update available"),
                    Err(e) => log::warn!("update check failed: {e}"),
                },
                Err(e) => log::warn!("updater unavailable: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(6 * 60 * 60)).await;
        }
    });
}

/// Every panic — any thread — lands in the log with its location before the
/// process reacts to it. Field diagnosis depends on this: the dashboard runs
/// unattended on a case screen with no console.
fn install_panic_logger() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::error!(
            "panic on thread {:?}: {info}",
            thread.name().unwrap_or("unnamed")
        );
        default_hook(info);
    }));
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|window, event| {
            // The customization window closes to the tray; the panel keeps
            // running until Quit.
            if window.label() == "settings" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::frontend_ready,
            commands::frontend_log,
            commands::list_backgrounds,
            commands::media_play_pause,
            commands::media_next,
            commands::media_prev,
            commands::open_shortcut,
            commands::set_theme,
            commands::widget_add,
            commands::widget_remove,
            commands::widget_move,
        ])
        .setup(|app| {
            install_panic_logger();
            let config_dir = app.path().app_config_dir()?;
            let config = config::load_or_create(&config_dir);
            sync_autostart(app.handle(), config.autostart);
            let (refresh_tx, refresh_rx) = tokio::sync::mpsc::unbounded_channel();
            app.manage(AppState {
                config: Mutex::new(config),
                weather: Mutex::new(Vec::new()),
                weather_refresh: refresh_tx,
            });
            app.manage(media::MediaState::default());
            app.manage(stats::StatsState::default());
            display::create_dashboard_window(app.handle())?;
            // Wallpaper Engine model: launched by hand → the customization
            // window greets you; launched at login → straight to the tray.
            let autostarted = std::env::args().any(|arg| arg == "--autostart");
            display::create_settings_window(app.handle(), !autostarted)?;
            setup_tray(app)?;
            config::watch(app.handle().clone(), config_dir);
            weather::start(app.handle().clone(), refresh_rx);
            media::start(app.handle().clone());
            stats::start(app.handle().clone());
            start_updater(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Slab");
}
