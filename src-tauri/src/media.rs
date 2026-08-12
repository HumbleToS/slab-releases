//! Now-playing metadata and playback control via GSMTC (`Windows.Media.Control`).
//!
//! Event-driven only: session/properties/playback-info change events push
//! `media-update` — no polling loops, no synthetic keypresses. Control intents
//! call the session's TryTogglePlayPause / TrySkipNext / TrySkipPrevious.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

#[derive(Debug, Clone, Serialize)]
pub struct MediaUpdate {
    pub title: String,
    pub artist: String,
    pub app_id: String,
    pub playing: bool,
    pub empty: bool,
}

impl Default for MediaUpdate {
    fn default() -> Self {
        MediaUpdate {
            title: String::new(),
            artist: String::new(),
            app_id: String::new(),
            playing: false,
            empty: true,
        }
    }
}

#[derive(Default)]
pub struct MediaState {
    manager: Mutex<Option<SessionManager>>,
    /// Per-session change registrations (properties token, playback token),
    /// dropped and re-created whenever the session list changes.
    subscriptions: Mutex<Vec<(Session, i64, i64)>>,
    pub last: Mutex<MediaUpdate>,
}

pub enum Control {
    PlayPause,
    Next,
    Prev,
}

pub fn start(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("gsmtc".into())
        .spawn(move || match init(&app) {
            Ok(()) => {
                // Handlers fire on the WinRT threadpool; this thread only
                // keeps the manager's registrations alive.
                loop {
                    std::thread::park();
                }
            }
            Err(e) => {
                log::error!("media integration unavailable: {e}");
                push_update(&app, MediaUpdate::default());
            }
        });
    if let Err(e) = spawned {
        log::error!("could not spawn gsmtc thread: {e}");
    }
}

fn init(app: &AppHandle) -> windows::core::Result<()> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
    let manager = SessionManager::RequestAsync()?.get()?;
    let state = app.state::<MediaState>();
    crate::lock(&state.manager).replace(manager.clone());

    manager.SessionsChanged(&TypedEventHandler::new({
        let app = app.clone();
        move |_, _| {
            resubscribe(&app);
            Ok(())
        }
    }))?;
    manager.CurrentSessionChanged(&TypedEventHandler::new({
        let app = app.clone();
        move |_, _| {
            push_state(&app);
            Ok(())
        }
    }))?;

    resubscribe(app);
    Ok(())
}

/// Re-register property/playback listeners on every live session, then push
/// the current state.
fn resubscribe(app: &AppHandle) {
    let state = app.state::<MediaState>();
    let Some(manager) = crate::lock(&state.manager).clone() else {
        return;
    };

    {
        let mut subscriptions = crate::lock(&state.subscriptions);
        for (session, properties_token, playback_token) in subscriptions.drain(..) {
            let _ = session.RemoveMediaPropertiesChanged(properties_token);
            let _ = session.RemovePlaybackInfoChanged(playback_token);
        }
        if let Ok(sessions) = manager.GetSessions() {
            for session in sessions {
                let properties_token = session.MediaPropertiesChanged(&TypedEventHandler::new({
                    let app = app.clone();
                    move |_, _| {
                        push_state(&app);
                        Ok(())
                    }
                }));
                let playback_token = session.PlaybackInfoChanged(&TypedEventHandler::new({
                    let app = app.clone();
                    move |_, _| {
                        push_state(&app);
                        Ok(())
                    }
                }));
                match (properties_token, playback_token) {
                    (Ok(pt), Ok(bt)) => subscriptions.push((session, pt, bt)),
                    (pt, bt) => {
                        log::warn!("session subscription failed: {:?} {:?}", pt.err(), bt.err());
                    }
                }
            }
        }
    }
    push_state(app);
}

/// Read the preferred session and emit `media-update`.
fn push_state(app: &AppHandle) {
    let state = app.state::<MediaState>();
    let manager = crate::lock(&state.manager).clone();
    let update = manager
        .as_ref()
        .and_then(pick_session)
        .and_then(|session| match read_session(&session) {
            Ok(update) => Some(update),
            Err(e) => {
                log::warn!("could not read media session: {e}");
                None
            }
        })
        .unwrap_or_default();
    push_update(app, update);
}

fn push_update(app: &AppHandle, update: MediaUpdate) {
    let state = app.state::<MediaState>();
    *crate::lock(&state.last) = update.clone();
    if let Err(e) = app.emit("media-update", &update) {
        log::warn!("could not emit media-update: {e}");
    }
}

/// Prefer the actively playing session; fall back to the system's current one.
fn pick_session(manager: &SessionManager) -> Option<Session> {
    if let Ok(sessions) = manager.GetSessions() {
        for session in sessions {
            let playing = session
                .GetPlaybackInfo()
                .and_then(|info| info.PlaybackStatus())
                .map(|status| status == PlaybackStatus::Playing)
                .unwrap_or(false);
            if playing {
                return Some(session);
            }
        }
    }
    manager.GetCurrentSession().ok()
}

fn read_session(session: &Session) -> windows::core::Result<MediaUpdate> {
    let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == PlaybackStatus::Playing;
    let properties = session.TryGetMediaPropertiesAsync()?.get()?;
    Ok(MediaUpdate {
        title: properties
            .Title()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        artist: properties
            .Artist()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        app_id: session
            .SourceAppUserModelId()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        playing,
        empty: false,
    })
}

/// Fire a control intent at the preferred session. The returned async
/// operation is dropped deliberately — it completes on its own and the
/// resulting state change comes back through the change events.
pub fn control(app: &AppHandle, action: Control) {
    let state = app.state::<MediaState>();
    let manager = crate::lock(&state.manager).clone();
    let Some(session) = manager.as_ref().and_then(pick_session) else {
        log::info!("media control ignored: no active session");
        return;
    };
    let result = match action {
        Control::PlayPause => session.TryTogglePlayPauseAsync(),
        Control::Next => session.TrySkipNextAsync(),
        Control::Prev => session.TrySkipPreviousAsync(),
    };
    if let Err(e) = result {
        log::warn!("media control failed: {e}");
    }
}
