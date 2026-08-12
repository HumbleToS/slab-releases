//! `[theme]` resolution into the CSS custom-property token set.
//!
//! Every themable value the frontend uses is produced here; the frontend
//! applies the tokens verbatim and never hardcodes its own. Invalid user
//! values fall back to the shipped defaults with a log line — never a crash.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::config::{Config, Theme, Widget};

/// Everything the webview needs to render: sent as the `config-update` event
/// payload at load and on every hot-reload.
#[derive(Debug, Clone, Serialize)]
pub struct UiState {
    pub tokens: Vec<(String, String)>,
    pub background: Background,
    pub widgets: Vec<Widget>,
    /// Where config.toml lives, so the customization window can offer
    /// "open config folder".
    pub config_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Background {
    pub kind: BackgroundKind,
    /// Absolute path for the webview to load via the asset protocol.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundKind {
    Video,
    Image,
    None,
}

pub fn ui_state(app: &AppHandle, config: &Config) -> UiState {
    let background = resolve_background(&config.theme.background);
    if let Some(path) = &background.path {
        // The webview itself has no filesystem reach; grant exactly this file.
        if let Err(e) = app.asset_protocol_scope().allow_file(path) {
            log::warn!("could not expose background {path}: {e}");
        }
    }
    for widget in &config.widgets {
        // Shortcut icons given as image paths (rather than built-in names)
        // also need asset-protocol access.
        if let Widget::Shortcut { icon, .. } = widget {
            if Path::new(icon).extension().is_some() && Path::new(icon).is_file() {
                if let Err(e) = app.asset_protocol_scope().allow_file(icon) {
                    log::warn!("could not expose icon {icon}: {e}");
                }
            }
        }
    }
    UiState {
        tokens: resolve_tokens(&config.theme),
        background,
        widgets: config.widgets.clone(),
        config_dir: app
            .path()
            .app_config_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

pub fn resolve_tokens(theme: &Theme) -> Vec<(String, String)> {
    let defaults = Theme::default();
    let panel_rgb = rgb_triplet(&theme.panel_color, &defaults.panel_color, "panel_color");
    // Derived tokens: the frontend must never invent a color, so foreground
    // and hairline are computed here from the panel's lightness.
    let fg_rgb = if triplet_luminance(&panel_rgb) < 0.5 {
        "255 255 255"
    } else {
        "16 16 18"
    };
    let shade = clamp01(theme.shade, "shade");
    vec![
        (
            "--accent".into(),
            css_color(&theme.accent, &defaults.accent, "accent"),
        ),
        ("--panel-rgb".into(), panel_rgb),
        (
            "--panel-opacity".into(),
            fmt_num(clamp01(theme.panel_opacity, "panel_opacity")),
        ),
        ("--blur".into(), px(theme.blur, "blur")),
        ("--radius".into(), px(theme.radius, "radius")),
        ("--font".into(), font_name(&theme.font, &defaults.font)),
        ("--text-scale".into(), fmt_num(text_scale(theme.text_scale))),
        ("--shade".into(), fmt_num(shade)),
        ("--fg-rgb".into(), fg_rgb.into()),
        ("--hairline".into(), format!("rgb({fg_rgb} / 0.08)")),
        (
            "--shade-overlay".into(),
            format!("rgb(0 0 0 / {})", fmt_num(shade)),
        ),
    ]
}

/// Relative lightness (0..1) of an `"r g b"` triplet string.
fn triplet_luminance(triplet: &str) -> f64 {
    let parts: Vec<f64> = triplet
        .split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    match parts.as_slice() {
        [r, g, b] => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0,
        _ => 0.0,
    }
}

fn is_hex_color(value: &str) -> bool {
    value.starts_with('#')
        && matches!(value.len(), 4 | 7 | 9)
        && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Pass a valid `#rgb`/`#rrggbb`/`#rrggbbaa` through; anything else falls back.
fn css_color(value: &str, fallback: &str, name: &str) -> String {
    let value = value.trim();
    if is_hex_color(value) {
        value.to_ascii_lowercase()
    } else {
        log::warn!("theme.{name} {value:?} is not a hex color; using {fallback}");
        fallback.into()
    }
}

/// `#rrggbb` → `"r g b"` so the frontend can combine it with the opacity token
/// via `rgb(var(--panel-rgb) / var(--panel-opacity))`.
fn rgb_triplet(value: &str, fallback: &str, name: &str) -> String {
    let hex = css_color(value, fallback, name);
    let digits = &hex[1..];
    let (r, g, b) = match digits.len() {
        3 => (
            u8::from_str_radix(&digits[0..1].repeat(2), 16),
            u8::from_str_radix(&digits[1..2].repeat(2), 16),
            u8::from_str_radix(&digits[2..3].repeat(2), 16),
        ),
        _ => (
            u8::from_str_radix(&digits[0..2], 16),
            u8::from_str_radix(&digits[2..4], 16),
            u8::from_str_radix(&digits[4..6], 16),
        ),
    };
    match (r, g, b) {
        (Ok(r), Ok(g), Ok(b)) => format!("{r} {g} {b}"),
        _ => rgb_triplet(fallback, fallback, name),
    }
}

fn clamp01(value: f64, name: &str) -> f64 {
    if !(0.0..=1.0).contains(&value) || value.is_nan() {
        log::warn!("theme.{name} {value} outside 0..1; clamping");
    }
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

fn px(value: f64, name: &str) -> String {
    let value = if value.is_finite() && value >= 0.0 {
        value
    } else {
        log::warn!("theme.{name} {value} is not a length; using 0");
        0.0
    };
    format!("{}px", fmt_num(value))
}

fn text_scale(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        log::warn!("theme.text_scale {value} is not positive; using 1");
        1.0
    }
}

/// Font family name, quoted for CSS; quotes and backslashes stripped so a
/// config value can never escape the declaration.
fn font_name(value: &str, fallback: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\' | ';' | '{' | '}'))
        .collect();
    let cleaned = cleaned.trim();
    let name = if cleaned.is_empty() {
        fallback
    } else {
        cleaned
    };
    format!("\"{name}\"")
}

fn fmt_num(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

const VIDEO_EXTENSIONS: [&str; 5] = ["mp4", "webm", "mkv", "mov", "avi"];
const IMAGE_EXTENSIONS: [&str; 7] = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "avif"];

/// A file path is used directly; a directory only marks where the selector
/// browses (never auto-picked); anything else means the gradient fallback.
pub fn resolve_background(raw: &str) -> Background {
    let raw = raw.trim();
    if raw.is_empty() {
        return Background {
            kind: BackgroundKind::None,
            path: None,
        };
    }
    let path = Path::new(raw);
    let picked = match fs::metadata(path) {
        Ok(meta) if meta.is_file() => Some(path.to_path_buf()),
        Ok(meta) if meta.is_dir() => {
            // Never guess a wallpaper: a folder only tells the selector where
            // to browse. Gradient until the user picks a file.
            log::info!("background {raw} is a folder; gradient until one is chosen in Slab");
            None
        }
        _ => {
            log::info!("background {raw} not found; using gradient");
            None
        }
    };
    match picked {
        Some(file) => match media_kind(&file) {
            Some(kind) => Background {
                kind,
                path: Some(file.to_string_lossy().into_owned()),
            },
            None => {
                log::warn!(
                    "background {} has an unsupported extension; using gradient",
                    file.display()
                );
                Background {
                    kind: BackgroundKind::None,
                    path: None,
                }
            }
        },
        None => Background {
            kind: BackgroundKind::None,
            path: None,
        },
    }
}

pub(crate) fn media_kind(path: &Path) -> Option<BackgroundKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some(BackgroundKind::Video)
    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some(BackgroundKind::Image)
    } else {
        None
    }
}

/// Where Wallpaper Engine keeps downloaded wallpapers; always offered to the
/// background selector alongside the configured folder.
const WALLPAPER_ENGINE_WORKSHOP: &str =
    "C:/Program Files (x86)/Steam/steamapps/workshop/content/431960";

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundOption {
    pub path: String,
    pub name: String,
    pub kind: BackgroundKind,
}

/// Videos and images the background selector can offer: the configured
/// background folder (or the chosen file's folder) plus the Wallpaper Engine
/// workshop directory. Newest first, capped to keep the grid sane.
pub fn list_backgrounds(app: &AppHandle) -> Vec<BackgroundOption> {
    let configured = {
        let state = app.state::<crate::AppState>();
        let config = crate::lock(&state.config);
        config.theme.background.clone()
    };
    let mut roots: Vec<PathBuf> = Vec::new();
    let configured_path = Path::new(configured.trim());
    if configured_path.is_dir() {
        roots.push(configured_path.to_path_buf());
    } else if let Some(parent) = configured_path.parent().filter(|p| p.is_dir()) {
        roots.push(parent.to_path_buf());
    }
    let workshop = PathBuf::from(WALLPAPER_ENGINE_WORKSHOP);
    if workshop.is_dir() && !roots.contains(&workshop) {
        roots.push(workshop);
    }

    let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for root in &roots {
        // Selector previews load through the asset protocol.
        if let Err(e) = app.asset_protocol_scope().allow_directory(root, true) {
            log::warn!("could not expose {}: {e}", root.display());
        }
        collect_media(root, 2, &mut found);
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.dedup_by(|a, b| a.1 == b.1);
    found
        .into_iter()
        .take(80)
        .filter_map(|(_, path)| {
            let kind = media_kind(&path)?;
            Some(BackgroundOption {
                name: display_name(&path),
                path: path.to_string_lossy().into_owned(),
                kind,
            })
        })
        .collect()
}

fn collect_media(dir: &Path, depth: u32, out: &mut Vec<(std::time::SystemTime, PathBuf)>) {
    // Hard stop for enormous libraries — the selector caps at 80 anyway.
    const SCAN_LIMIT: usize = 500;
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= SCAN_LIMIT {
            return;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if depth > 0 {
                collect_media(&path, depth - 1, out);
            }
        } else if media_kind(&path).is_some() {
            let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            out.push((modified, path));
        }
    }
}

/// Wallpaper Engine folders carry a project.json with the wallpaper's title;
/// prefer it over numeric workshop-id file stems.
fn display_name(path: &Path) -> String {
    if let Some(project) = path.parent().map(|p| p.join("project.json")) {
        if let Ok(text) = fs::read_to_string(&project) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(title) = json.get("title").and_then(|t| t.as_str()) {
                    return title.to_string();
                }
            }
        }
    }
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(tokens: &[(String, String)], name: &str) -> String {
        tokens
            .iter()
            .find(|(k, _)| k == name)
            .unwrap_or_else(|| panic!("token {name} missing"))
            .1
            .clone()
    }

    #[test]
    fn default_theme_resolves_to_design_plan_values() {
        let tokens = resolve_tokens(&Theme::default());
        assert_eq!(token(&tokens, "--accent"), "#e4553b");
        assert_eq!(token(&tokens, "--panel-rgb"), "10 10 12");
        assert_eq!(token(&tokens, "--panel-opacity"), "0.42");
        assert_eq!(token(&tokens, "--blur"), "22px");
        assert_eq!(token(&tokens, "--radius"), "26px");
        assert_eq!(token(&tokens, "--font"), "\"Inter\"");
        assert_eq!(token(&tokens, "--text-scale"), "1");
        assert_eq!(token(&tokens, "--shade"), "0.45");
        assert_eq!(token(&tokens, "--fg-rgb"), "255 255 255");
        assert_eq!(token(&tokens, "--hairline"), "rgb(255 255 255 / 0.08)");
        assert_eq!(token(&tokens, "--shade-overlay"), "rgb(0 0 0 / 0.45)");
    }

    #[test]
    fn light_panels_get_dark_foreground() {
        let theme = Theme {
            panel_color: "#f2f2f4".into(),
            ..Theme::default()
        };
        let tokens = resolve_tokens(&theme);
        assert_eq!(token(&tokens, "--fg-rgb"), "16 16 18");
    }

    #[test]
    fn invalid_colors_fall_back() {
        let theme = Theme {
            accent: "red; background: url(evil)".into(),
            panel_color: "#zzzzzz".into(),
            ..Theme::default()
        };
        let tokens = resolve_tokens(&theme);
        assert_eq!(token(&tokens, "--accent"), "#e4553b");
        assert_eq!(token(&tokens, "--panel-rgb"), "10 10 12");
    }

    #[test]
    fn short_hex_expands() {
        let theme = Theme {
            panel_color: "#fff".into(),
            ..Theme::default()
        };
        assert_eq!(token(&resolve_tokens(&theme), "--panel-rgb"), "255 255 255");
    }

    #[test]
    fn out_of_range_numbers_clamp() {
        let theme = Theme {
            panel_opacity: 7.0,
            shade: -3.0,
            blur: -10.0,
            text_scale: 0.0,
            ..Theme::default()
        };
        let tokens = resolve_tokens(&theme);
        assert_eq!(token(&tokens, "--panel-opacity"), "1");
        assert_eq!(token(&tokens, "--shade"), "0");
        assert_eq!(token(&tokens, "--blur"), "0px");
        assert_eq!(token(&tokens, "--text-scale"), "1");
    }

    #[test]
    fn font_cannot_escape_the_declaration() {
        let theme = Theme {
            font: "Inter\"; } body { color: red".into(),
            ..Theme::default()
        };
        let value = token(&resolve_tokens(&theme), "--font");
        assert!(!value.contains(';') && !value.contains('}'));
    }

    #[test]
    fn media_kind_by_extension() {
        assert_eq!(
            media_kind(Path::new("C:/x/wall.MP4")),
            Some(BackgroundKind::Video)
        );
        assert_eq!(
            media_kind(Path::new("C:/x/wall.jpeg")),
            Some(BackgroundKind::Image)
        );
        assert_eq!(media_kind(Path::new("C:/x/scene.pkg")), None);
        assert_eq!(media_kind(Path::new("C:/x/noext")), None);
    }

    #[test]
    fn empty_or_missing_background_is_gradient() {
        assert_eq!(resolve_background("").kind, BackgroundKind::None);
        assert_eq!(
            resolve_background("Z:/definitely/not/here.mp4").kind,
            BackgroundKind::None
        );
    }

    #[test]
    fn directory_background_is_gradient_until_chosen() {
        let dir = std::env::temp_dir().join(format!("slab-theme-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("scene.mp4"), b"video").unwrap();

        let background = resolve_background(dir.to_str().unwrap());
        assert_eq!(background.kind, BackgroundKind::None);
        assert!(background.path.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_media_finds_nested_files_and_display_name_prefers_titles() {
        let dir = std::env::temp_dir().join(format!("slab-collect-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let sub = dir.join("123456789");
        fs::create_dir_all(&sub).unwrap();
        fs::write(dir.join("plain.png"), b"png").unwrap();
        fs::write(sub.join("project.json"), br#"{"title": "Neon Rain"}"#).unwrap();
        fs::write(sub.join("scene.mp4"), b"video").unwrap();
        fs::write(sub.join("notes.txt"), b"skip me").unwrap();

        let mut found = Vec::new();
        collect_media(&dir, 2, &mut found);
        assert_eq!(found.len(), 2);
        assert_eq!(display_name(&sub.join("scene.mp4")), "Neon Rain");
        assert_eq!(display_name(&dir.join("plain.png")), "plain");
        let _ = fs::remove_dir_all(&dir);
    }
}
