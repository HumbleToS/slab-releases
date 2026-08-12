//! Open-Meteo fetch, cache, and refresh; weather-code → label mapping.
//!
//! The webview never talks to the network — this module fetches for every
//! weather widget in config, caches the latest result, refreshes every ten
//! minutes, and pushes `weather-update` events.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{TempUnit, Widget};

const REFRESH_INTERVAL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize)]
pub struct WeatherUpdate {
    /// Index of the widget in the config's widget list, so the frontend can
    /// route updates when several weather widgets exist.
    pub widget_index: usize,
    pub label: String,
    pub unit: TempUnit,
    pub temperature: f64,
    pub high: f64,
    pub low: f64,
    pub code: u8,
    pub condition: String,
    pub is_day: bool,
    pub sunrise: String,
    pub sunset: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    current: ApiCurrent,
    daily: ApiDaily,
}

#[derive(Deserialize)]
struct ApiCurrent {
    temperature_2m: f64,
    weather_code: u8,
    is_day: u8,
}

#[derive(Deserialize)]
struct ApiDaily {
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
    sunrise: Vec<String>,
    sunset: Vec<String>,
}

/// Run the refresh loop: an immediate fetch, then every ten minutes, plus an
/// out-of-band fetch whenever the config changes (coords may have moved).
pub fn start(app: AppHandle, mut refresh: tokio::sync::mpsc::UnboundedReceiver<()>) {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let mut tick = tokio::time::interval(REFRESH_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                message = refresh.recv() => {
                    if message.is_none() {
                        return;
                    }
                    tick.reset();
                }
            }
            refresh_all(&app, &client).await;
        }
    });
}

async fn refresh_all(app: &AppHandle, client: &reqwest::Client) {
    let state = app.state::<crate::AppState>();
    let widgets = crate::lock(&state.config).widgets.clone();
    let mut updates = Vec::new();
    for (index, widget) in widgets.iter().enumerate() {
        let Widget::Weather {
            lat,
            lon,
            label,
            unit,
        } = widget
        else {
            continue;
        };
        match fetch(client, *lat, *lon, *unit).await {
            Ok(response) => {
                let update = to_update(index, label.clone(), *unit, &response);
                if let Err(e) = app.emit("weather-update", &update) {
                    log::warn!("could not emit weather-update: {e}");
                }
                updates.push(update);
            }
            Err(e) => log::warn!("weather fetch for {label:?} failed: {e}"),
        }
    }
    // Only successful fetches replace the cache; a failed refresh keeps
    // showing the last known state instead of blanking the widget.
    if !updates.is_empty() {
        *crate::lock(&state.weather) = updates;
    }
}

async fn fetch(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
    unit: TempUnit,
) -> Result<ApiResponse, reqwest::Error> {
    client
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("current", "temperature_2m,weather_code,is_day".into()),
            (
                "daily",
                "temperature_2m_max,temperature_2m_min,sunrise,sunset".into(),
            ),
            ("forecast_days", "1".into()),
            ("timezone", "auto".into()),
            ("temperature_unit", unit.api_name().into()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

fn to_update(
    widget_index: usize,
    label: String,
    unit: TempUnit,
    response: &ApiResponse,
) -> WeatherUpdate {
    WeatherUpdate {
        widget_index,
        label,
        unit,
        temperature: response.current.temperature_2m,
        high: response
            .daily
            .temperature_2m_max
            .first()
            .copied()
            .unwrap_or(f64::NAN),
        low: response
            .daily
            .temperature_2m_min
            .first()
            .copied()
            .unwrap_or(f64::NAN),
        code: response.current.weather_code,
        condition: condition_label(response.current.weather_code).into(),
        is_day: response.current.is_day == 1,
        sunrise: response.daily.sunrise.first().cloned().unwrap_or_default(),
        sunset: response.daily.sunset.first().cloned().unwrap_or_default(),
    }
}

/// WMO weather interpretation codes → display labels.
pub fn condition_label(code: u8) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly Clear",
        2 => "Partly Cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing Drizzle",
        61 => "Light Rain",
        63 => "Rain",
        65 => "Heavy Rain",
        66 | 67 => "Freezing Rain",
        71 => "Light Snow",
        73 => "Snow",
        75 => "Heavy Snow",
        77 => "Snow Grains",
        80 | 81 => "Showers",
        82 => "Heavy Showers",
        85 | 86 => "Snow Showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm + Hail",
        _ => "Weather",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_map_to_labels() {
        assert_eq!(condition_label(0), "Clear");
        assert_eq!(condition_label(2), "Partly Cloudy");
        assert_eq!(condition_label(45), "Fog");
        assert_eq!(condition_label(55), "Drizzle");
        assert_eq!(condition_label(63), "Rain");
        assert_eq!(condition_label(75), "Heavy Snow");
        assert_eq!(condition_label(82), "Heavy Showers");
        assert_eq!(condition_label(95), "Thunderstorm");
        assert_eq!(condition_label(99), "Thunderstorm + Hail");
    }

    #[test]
    fn unknown_codes_have_a_neutral_label() {
        assert_eq!(condition_label(42), "Weather");
        assert_eq!(condition_label(255), "Weather");
    }

    #[test]
    fn api_response_shape_parses() {
        let json = r#"{
            "current": {"temperature_2m": 91.4, "weather_code": 1, "is_day": 1},
            "daily": {
                "temperature_2m_max": [95.7],
                "temperature_2m_min": [70.2],
                "sunrise": ["2026-08-10T06:41"],
                "sunset": ["2026-08-10T20:02"]
            }
        }"#;
        let response: ApiResponse = serde_json::from_str(json).unwrap();
        let update = to_update(2, "Hobbs, NM".into(), TempUnit::Fahrenheit, &response);
        assert_eq!(update.widget_index, 2);
        assert_eq!(update.temperature, 91.4);
        assert_eq!(update.high, 95.7);
        assert_eq!(update.condition, "Mostly Clear");
        assert!(update.is_day);
        assert_eq!(update.sunset, "2026-08-10T20:02");
    }
}
