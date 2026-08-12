//! config.toml load, defaults regeneration, and hot-reload watching.
//!
//! Contract (CLAUDE.md): unknown keys are ignored, unknown widget kinds are
//! skipped with a log line, a missing or corrupt file regenerates defaults,
//! and saves hot-reload without a restart. User config must never crash Slab.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

pub const CONFIG_FILE: &str = "config.toml";

/// Shipped default config, comments included — written to disk whenever the
/// user has no config or a corrupt one. Unit tests keep it parseable and in
/// sync with `Theme::default()`.
pub const DEFAULT_CONFIG: &str = r##"# Slab configuration. Edit and save — the dashboard updates live.

# Launch Slab when Windows starts.
autostart = true

[theme]
accent        = "#e4553b"
panel_color   = "#0a0a0c"
panel_opacity = 0.42
blur          = 22
radius        = 26
font          = "Inter"        # any installed font
text_scale    = 1.0
# Background video or image. Point this at a file, or at a folder to use the
# newest video/image inside it. The default is Wallpaper Engine's workshop
# folder, so Slab picks up the most recently downloaded wallpaper; if nothing
# is found a built-in gradient is shown instead.
background    = 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960'
shade         = 0.45           # darkness of the legibility overlay, 0..1

[[widget]]
kind = "clock"
format = "12h"                 # or "24h"
show_date = true

[[widget]]
kind = "media"

[[widget]]
kind = "weather"
lat = 32.7026
lon = -103.136
label = "Hobbs, NM"
unit = "fahrenheit"            # or "celsius"

[[widget]]
kind = "shortcut"
label = "Discord"
uri = "discord://-/"
icon = "discord"               # built-in icon name, or path to an svg

# More widget kinds, ready to uncomment. Live CPU / RAM / drive meters:
#
# [[widget]]
# kind = "stats"
# show_cpu  = true
# show_ram  = true
# show_disk = true
# disk = "C:"                    # which drive the disk meter watches
# disk_label = "storage"         # name shown on the disk row

# A static label — title a section of the column, or just say something:
#
# [[widget]]
# kind = "text"
# text = "arcade"
# size = "medium"                # small / medium / large
# align = "left"                 # or "center"
"##;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub accent: String,
    pub panel_color: String,
    pub panel_opacity: f64,
    pub blur: f64,
    pub radius: f64,
    pub font: String,
    pub text_scale: f64,
    pub background: String,
    pub shade: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent: "#e4553b".into(),
            panel_color: "#0a0a0c".into(),
            panel_opacity: 0.42,
            blur: 22.0,
            radius: 26.0,
            font: "Inter".into(),
            text_scale: 1.0,
            background: "C:/Program Files (x86)/Steam/steamapps/workshop/content/431960".into(),
            shade: 0.45,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TempUnit {
    #[default]
    Fahrenheit,
    Celsius,
}

impl TempUnit {
    /// The `temperature_unit` value Open-Meteo expects.
    pub fn api_name(self) -> &'static str {
        match self {
            TempUnit::Fahrenheit => "fahrenheit",
            TempUnit::Celsius => "celsius",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClockFormat {
    #[default]
    #[serde(rename = "12h")]
    Twelve,
    #[serde(rename = "24h")]
    TwentyFour,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Widget {
    Clock {
        #[serde(default)]
        format: ClockFormat,
        #[serde(default = "default_true")]
        show_date: bool,
    },
    Media {},
    Weather {
        lat: f64,
        lon: f64,
        #[serde(default)]
        label: String,
        #[serde(default)]
        unit: TempUnit,
    },
    Shortcut {
        label: String,
        uri: String,
        #[serde(default)]
        icon: String,
    },
    Stats {
        #[serde(default = "default_true")]
        show_cpu: bool,
        #[serde(default = "default_true")]
        show_ram: bool,
        #[serde(default)]
        show_disk: bool,
        #[serde(default = "default_disk")]
        disk: String,
        #[serde(default = "default_disk_label")]
        disk_label: String,
    },
    Text {
        text: String,
        #[serde(default)]
        size: TextSize,
        #[serde(default)]
        align: TextAlign,
    },
}

fn default_true() -> bool {
    true
}

fn default_disk() -> String {
    "C:".into()
}

fn default_disk_label() -> String {
    "storage".into()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Config {
    pub autostart: bool,
    pub theme: Theme,
    pub widgets: Vec<Widget>,
}

/// First parse pass: widgets stay raw TOML values so one bad entry is skipped
/// instead of failing the whole file.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawConfig {
    autostart: Option<bool>,
    theme: Theme,
    #[serde(rename = "widget")]
    widgets: Vec<toml::Value>,
}

pub fn parse(text: &str) -> Result<Config, toml::de::Error> {
    let raw: RawConfig = toml::from_str(text)?;
    Ok(Config {
        autostart: raw.autostart.unwrap_or(true),
        theme: raw.theme,
        widgets: raw.widgets.iter().filter_map(parse_widget).collect(),
    })
}

fn parse_widget(value: &toml::Value) -> Option<Widget> {
    let Some(kind) = value.get("kind").and_then(|k| k.as_str()).map(String::from) else {
        log::warn!("[[widget]] entry without a `kind` — skipped");
        return None;
    };
    match value.clone().try_into::<Widget>() {
        Ok(widget) => Some(widget),
        Err(e) => {
            log::warn!("widget kind {kind:?} skipped: {e}");
            None
        }
    }
}

pub fn default_config() -> Config {
    parse(DEFAULT_CONFIG).expect("default config template must parse")
}

/// Load `config.toml` from `dir`, regenerating defaults when the file is
/// missing or unparseable. A corrupt file is preserved as `config.toml.invalid`
/// before being replaced.
pub fn load_or_create(dir: &Path) -> Config {
    let path = dir.join(CONFIG_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => match parse(&text) {
            Ok(config) => config,
            Err(e) => {
                log::warn!("{} is invalid ({e}); regenerating defaults", path.display());
                let backup = path.with_extension("toml.invalid");
                if let Err(e) = fs::rename(&path, &backup) {
                    log::warn!("could not back up invalid config: {e}");
                } else {
                    log::warn!("invalid config kept at {}", backup.display());
                }
                write_default(&path);
                default_config()
            }
        },
        Err(_) => {
            log::info!("no config at {}; writing defaults", path.display());
            write_default(&path);
            default_config()
        }
    }
}

fn write_default(path: &Path) {
    if let Some(dir) = path.parent() {
        if let Err(e) = fs::create_dir_all(dir) {
            log::error!("could not create config dir {}: {e}", dir.display());
            return;
        }
    }
    if let Err(e) = fs::write(path, DEFAULT_CONFIG) {
        log::error!("could not write default config to {}: {e}", path.display());
    }
}

const THEME_STRING_KEYS: [&str; 4] = ["accent", "panel_color", "font", "background"];
const THEME_NUMBER_KEYS: [&str; 5] = ["panel_opacity", "blur", "radius", "text_scale", "shade"];

/// Write one `[theme]` key into config.toml in place, preserving the rest of
/// the file (comments included). The in-app settings panel goes through here:
/// the file stays the single source of truth and the watcher applies the
/// change exactly like a hand edit.
pub fn set_theme_value(dir: &Path, key: &str, value: &serde_json::Value) -> Result<(), String> {
    let entry: toml_edit::Value = if THEME_STRING_KEYS.contains(&key) {
        value
            .as_str()
            .ok_or_else(|| format!("theme.{key} needs a string"))?
            .into()
    } else if THEME_NUMBER_KEYS.contains(&key) {
        value
            .as_f64()
            .ok_or_else(|| format!("theme.{key} needs a number"))?
            .into()
    } else {
        return Err(format!("unknown theme key {key:?}"));
    };

    let path = dir.join(CONFIG_FILE);
    let text = fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_CONFIG.to_string());
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .or_else(|_| DEFAULT_CONFIG.parse())
        .map_err(|e| format!("config unparseable: {e}"))?;
    doc["theme"][key] = toml_edit::value(entry);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, doc.to_string()).map_err(|e| e.to_string())
}

/// Starter [[widget]] entries for the customization window's "add widget"
/// buttons. Every template must parse into a valid Widget (unit-tested);
/// required params get editable placeholder values.
const WIDGET_TEMPLATES: [(&str, &str); 6] = [
    ("clock", "kind = \"clock\"\nformat = \"12h\"\nshow_date = true\n"),
    ("media", "kind = \"media\"\n"),
    (
        "weather",
        "kind = \"weather\"\nlat = 32.7026\nlon = -103.136\nlabel = \"Hobbs, NM\"\nunit = \"fahrenheit\"\n",
    ),
    (
        "shortcut",
        "kind = \"shortcut\"\nlabel = \"New shortcut\"\nuri = \"https://github.com\"\nicon = \"globe\"\n",
    ),
    (
        "stats",
        "kind = \"stats\"\nshow_cpu = true\nshow_ram = true\nshow_disk = true\ndisk = \"C:\"\n",
    ),
    ("text", "kind = \"text\"\ntext = \"label\"\nsize = \"small\"\nalign = \"left\"\n"),
];

/// Load config.toml for in-place editing with the same fallback rules as
/// `set_theme_value`: missing file starts from the default template.
fn load_doc(path: &Path) -> Result<toml_edit::DocumentMut, String> {
    let text = fs::read_to_string(path).unwrap_or_else(|_| DEFAULT_CONFIG.to_string());
    text.parse()
        .or_else(|_| DEFAULT_CONFIG.parse())
        .map_err(|e| format!("config unparseable: {e}"))
}

fn save_doc(path: &Path, doc: &toml_edit::DocumentMut) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, doc.to_string()).map_err(|e| e.to_string())
}

fn widget_tables(doc: &mut toml_edit::DocumentMut) -> &mut toml_edit::ArrayOfTables {
    // Ensure the array exists so ops on an empty config still work.
    if doc
        .get("widget")
        .and_then(|w| w.as_array_of_tables())
        .is_none()
    {
        doc.insert(
            "widget",
            toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }
    doc["widget"]
        .as_array_of_tables_mut()
        .expect("just ensured")
}

/// The UI lists only the widgets `parse` accepted; the file may hold invalid
/// entries too (kept, per the config contract). Map a UI index onto the raw
/// [[widget]] array by counting entries the same way `parse` does.
fn file_index(tables: &toml_edit::ArrayOfTables, ui_index: usize) -> Result<usize, String> {
    let mut seen_valid = 0;
    for (raw_index, table) in tables.iter().enumerate() {
        let value: Option<toml::Value> = toml::from_str(&table.to_string()).ok();
        if value.as_ref().and_then(parse_widget).is_some() {
            if seen_valid == ui_index {
                return Ok(raw_index);
            }
            seen_valid += 1;
        }
    }
    Err(format!("widget index {ui_index} out of range"))
}

/// Append a starter widget of `kind`, writing through config.toml so the
/// watcher applies it like a hand edit.
/// Set one param of the widget at `ui_index`, in place, preserving comments.
/// The edit must leave the widget valid — a value that would make `parse`
/// skip the widget is rejected and the file is left untouched. This is what
/// lets the customization window offer full editing to non-technical users
/// without ever being able to corrupt the config.
pub fn set_widget_value(
    dir: &Path,
    ui_index: usize,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    if key == "kind" {
        return Err("kind is structural — remove the widget and add another".into());
    }
    let entry: toml_edit::Value = match value {
        serde_json::Value::String(s) => s.as_str().into(),
        serde_json::Value::Bool(b) => (*b).into(),
        serde_json::Value::Number(n) => n.as_f64().ok_or("bad number")?.into(),
        _ => return Err(format!("widget.{key} needs a string, number, or bool")),
    };
    let path = dir.join(CONFIG_FILE);
    let mut doc = load_doc(&path)?;
    let tables = widget_tables(&mut doc);
    let raw = file_index(tables, ui_index)?;
    let table = tables.get_mut(raw).ok_or("widget vanished")?;
    table[key] = toml_edit::value(entry);
    let still_valid = toml::from_str::<toml::Value>(&table.to_string())
        .ok()
        .as_ref()
        .and_then(parse_widget)
        .is_some();
    if !still_valid {
        // doc is discarded, the file keeps its previous contents
        return Err(format!("widget.{key}: that value isn't valid here"));
    }
    save_doc(&path, &doc)
}

pub fn add_widget(dir: &Path, kind: &str) -> Result<(), String> {
    let template = WIDGET_TEMPLATES
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, template)| *template)
        .ok_or_else(|| format!("unknown widget kind {kind:?}"))?;
    let path = dir.join(CONFIG_FILE);
    let mut doc = load_doc(&path)?;
    let entry: toml_edit::DocumentMut = template.parse().expect("templates are valid toml");
    widget_tables(&mut doc).push(entry.as_table().clone());
    save_doc(&path, &doc)
}

/// Remove the widget at `ui_index` (position among the *valid* widgets, as
/// shown in the customization window).
pub fn remove_widget(dir: &Path, ui_index: usize) -> Result<(), String> {
    let path = dir.join(CONFIG_FILE);
    let mut doc = load_doc(&path)?;
    let tables = widget_tables(&mut doc);
    let raw = file_index(tables, ui_index)?;
    tables.remove(raw);
    save_doc(&path, &doc)
}

/// Swap the widget at `ui_index` with its valid neighbor above (`up`) or
/// below. Order in the file is order on screen.
pub fn move_widget(dir: &Path, ui_index: usize, up: bool) -> Result<(), String> {
    let neighbor_ui = if up {
        ui_index.checked_sub(1).ok_or("already first")?
    } else {
        ui_index + 1
    };
    let path = dir.join(CONFIG_FILE);
    let mut doc = load_doc(&path)?;
    let tables = widget_tables(&mut doc);
    let a = file_index(tables, ui_index)?;
    let b = file_index(tables, neighbor_ui)?;
    let table_a = tables.get(a).cloned().ok_or("widget vanished")?;
    let table_b = tables.get(b).cloned().ok_or("widget vanished")?;
    // toml_edit tables remember their place in the document; swapping the
    // values alone renders in the original order. Swap, then pin each slot's
    // document position back so the rendered order actually flips.
    let position_a = table_a.position();
    let position_b = table_b.position();
    *tables.get_mut(a).expect("checked") = table_b;
    *tables.get_mut(b).expect("checked") = table_a;
    if let (Some(position_a), Some(position_b)) = (position_a, position_b) {
        tables.get_mut(a).expect("checked").set_position(position_a);
        tables.get_mut(b).expect("checked").set_position(position_b);
    }
    save_doc(&path, &doc)
}

/// Watch `dir` for saves of config.toml and re-apply the config on change.
/// Runs for the lifetime of the app; watcher failures are logged, never fatal.
pub fn watch(app: AppHandle, dir: PathBuf) {
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                log::error!("config watcher unavailable: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
            log::error!("could not watch {}: {e}", dir.display());
            return;
        }
        log::info!("watching {} for changes", dir.join(CONFIG_FILE).display());
        while let Ok(event) = rx.recv() {
            let Ok(event) = event else { continue };
            let is_config = event.paths.iter().any(|p| {
                p.file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case(CONFIG_FILE))
            });
            if !is_config {
                continue;
            }
            // Editors save in bursts (truncate + write, or atomic replace);
            // settle, then drain the queue so one save applies once.
            std::thread::sleep(Duration::from_millis(150));
            while rx.try_recv().is_ok() {}
            log::info!("config change detected; reloading");
            let reload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::apply_config(&app, load_or_create(&dir));
            }));
            if reload.is_err() {
                // Details already logged by the panic hook; the watcher
                // itself must survive to serve the next save.
                log::error!("config reload panicked; keeping previous state");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_parses_with_all_widgets() {
        let config = parse(DEFAULT_CONFIG).expect("default template must parse");
        assert!(config.autostart);
        assert_eq!(config.theme, Theme::default());
        assert_eq!(config.widgets.len(), 4);
        assert!(matches!(
            config.widgets[0],
            Widget::Clock {
                format: ClockFormat::Twelve,
                show_date: true
            }
        ));
        assert!(matches!(config.widgets[1], Widget::Media {}));
        assert!(matches!(config.widgets[3], Widget::Shortcut { .. }));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let config = parse(
            "bogus = 1\n\
             [theme]\naccent = \"#ffffff\"\nnot_a_theme_key = 2\n\
             [[widget]]\nkind = \"clock\"\nmystery = true\n",
        )
        .expect("unknown keys must not fail the parse");
        assert_eq!(config.theme.accent, "#ffffff");
        assert_eq!(config.widgets.len(), 1);
    }

    #[test]
    fn unknown_widget_kind_is_skipped() {
        let config = parse(
            "[[widget]]\nkind = \"clock\"\n\
             [[widget]]\nkind = \"hologram\"\n\
             [[widget]]\nkind = \"media\"\n",
        )
        .unwrap();
        assert_eq!(config.widgets.len(), 2);
    }

    #[test]
    fn malformed_known_widget_is_skipped() {
        // weather without lat/lon must not crash or abort the rest
        let config = parse(
            "[[widget]]\nkind = \"weather\"\nlabel = \"nowhere\"\n\
             [[widget]]\nkind = \"clock\"\n",
        )
        .unwrap();
        assert_eq!(config.widgets.len(), 1);
        assert!(matches!(config.widgets[0], Widget::Clock { .. }));
    }

    #[test]
    fn widget_params_take_defaults() {
        let config = parse("[[widget]]\nkind = \"clock\"\n").unwrap();
        assert!(matches!(
            config.widgets[0],
            Widget::Clock {
                format: ClockFormat::Twelve,
                show_date: true
            }
        ));
    }

    #[test]
    fn stats_widget_takes_defaults() {
        let config = parse("[[widget]]\nkind = \"stats\"\n").unwrap();
        assert!(matches!(
            &config.widgets[0],
            Widget::Stats {
                show_cpu: true,
                show_ram: true,
                show_disk: false,
                disk,
                disk_label
            } if disk == "C:" && disk_label == "storage"
        ));
    }

    #[test]
    fn stats_widget_accepts_params() {
        let config = parse(
            "[[widget]]\nkind = \"stats\"\nshow_cpu = false\nshow_disk = true\ndisk = \"D:\"\ndisk_label = \"games\"\n",
        )
        .unwrap();
        assert!(matches!(
            &config.widgets[0],
            Widget::Stats {
                show_cpu: false,
                show_ram: true,
                show_disk: true,
                disk,
                disk_label
            } if disk == "D:" && disk_label == "games"
        ));
    }

    #[test]
    fn text_widget_parses_size_and_align() {
        let config =
            parse("[[widget]]\nkind = \"text\"\ntext = \"arcade\"\nsize = \"large\"\nalign = \"center\"\n")
                .unwrap();
        assert!(matches!(
            &config.widgets[0],
            Widget::Text {
                text,
                size: TextSize::Large,
                align: TextAlign::Center
            } if text == "arcade"
        ));
    }

    #[test]
    fn text_widget_without_text_is_skipped() {
        let config = parse(
            "[[widget]]\nkind = \"text\"\nsize = \"small\"\n\
             [[widget]]\nkind = \"text\"\ntext = \"ok\"\n",
        )
        .unwrap();
        assert_eq!(config.widgets.len(), 1);
        assert!(matches!(
            &config.widgets[0],
            Widget::Text {
                text,
                size: TextSize::Medium,
                align: TextAlign::Left
            } if text == "ok"
        ));
    }

    #[test]
    fn default_template_examples_stay_valid_when_uncommented() {
        // The commented widget examples in DEFAULT_CONFIG must parse if a user
        // uncomments them exactly as written. Prose comments (no `=`, no
        // `[[widget]]`) stay comments.
        let uncommented: String = DEFAULT_CONFIG
            .lines()
            .map(|line| {
                let stripped = line.strip_prefix("# ").unwrap_or(line);
                if stripped.starts_with("[[widget]]") || stripped.contains(" = ") {
                    stripped
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let config = parse(&uncommented).expect("uncommented examples must parse");
        assert_eq!(config.widgets.len(), 6);
        assert!(matches!(config.widgets[4], Widget::Stats { .. }));
        assert!(matches!(config.widgets[5], Widget::Text { .. }));
    }

    #[test]
    fn corrupt_input_is_an_error() {
        assert!(parse("this is not = = toml [").is_err());
    }

    #[test]
    fn empty_input_yields_defaults() {
        let config = parse("").unwrap();
        assert!(config.autostart);
        assert_eq!(config.theme, Theme::default());
        assert!(config.widgets.is_empty());
    }

    #[test]
    fn integer_coordinates_parse_as_floats() {
        let config = parse("[[widget]]\nkind = \"weather\"\nlat = 33\nlon = -103\n").unwrap();
        assert!(
            matches!(config.widgets[0], Widget::Weather { lat, lon, .. } if lat == 33.0 && lon == -103.0)
        );
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("slab-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_regenerates_defaults() {
        let dir = scratch_dir("missing");
        let config = load_or_create(&dir);
        assert_eq!(config, default_config());
        assert!(dir.join(CONFIG_FILE).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_regenerated() {
        let dir = scratch_dir("corrupt");
        fs::write(dir.join(CONFIG_FILE), "??? not toml [[[").unwrap();
        let config = load_or_create(&dir);
        assert_eq!(config, default_config());
        assert!(dir.join("config.toml.invalid").exists());
        assert_eq!(
            fs::read_to_string(dir.join(CONFIG_FILE)).unwrap(),
            DEFAULT_CONFIG
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_theme_value_edits_in_place_and_keeps_comments() {
        let dir = scratch_dir("set-theme");
        fs::write(dir.join(CONFIG_FILE), DEFAULT_CONFIG).unwrap();
        set_theme_value(&dir, "accent", &serde_json::json!("#123456")).unwrap();
        set_theme_value(&dir, "panel_opacity", &serde_json::json!(0.9)).unwrap();
        let text = fs::read_to_string(dir.join(CONFIG_FILE)).unwrap();
        assert!(text.contains("# Slab configuration"), "comments preserved");
        let config = parse(&text).unwrap();
        assert_eq!(config.theme.accent, "#123456");
        assert_eq!(config.theme.panel_opacity, 0.9);
        assert_eq!(config.widgets.len(), 4, "widgets untouched");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_theme_value_rejects_bad_keys_and_types() {
        let dir = scratch_dir("set-theme-bad");
        assert!(set_theme_value(&dir, "autostart", &serde_json::json!(false)).is_err());
        assert!(set_theme_value(&dir, "accent", &serde_json::json!(5)).is_err());
        assert!(set_theme_value(&dir, "shade", &serde_json::json!("dark")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_theme_value_creates_missing_config() {
        let dir = scratch_dir("set-theme-missing");
        set_theme_value(&dir, "accent", &serde_json::json!("#abcdef")).unwrap();
        let config = load_or_create(&dir);
        assert_eq!(config.theme.accent, "#abcdef");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn widget_templates_all_parse_valid() {
        for (kind, template) in WIDGET_TEMPLATES {
            let config = parse(&format!("[[widget]]\n{template}")).unwrap();
            assert_eq!(config.widgets.len(), 1, "template {kind:?} must be valid");
        }
        // and the template list covers every Widget variant
        assert_eq!(WIDGET_TEMPLATES.len(), 6);
    }

    #[test]
    fn add_widget_appends_and_hot_parses() {
        let dir = scratch_dir("add-widget");
        fs::write(dir.join(CONFIG_FILE), DEFAULT_CONFIG).unwrap();
        add_widget(&dir, "stats").unwrap();
        let text = fs::read_to_string(dir.join(CONFIG_FILE)).unwrap();
        assert!(text.contains("# Slab configuration"), "comments preserved");
        let config = parse(&text).unwrap();
        assert_eq!(config.widgets.len(), 5);
        assert!(matches!(config.widgets[4], Widget::Stats { .. }));
        assert!(add_widget(&dir, "hologram").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_widget_targets_valid_index_past_invalid_entries() {
        let dir = scratch_dir("remove-widget");
        // raw file: clock, INVALID, media — UI sees [clock, media]
        fs::write(
            dir.join(CONFIG_FILE),
            "[[widget]]\nkind = \"clock\"\n\
             [[widget]]\nkind = \"hologram\"\n\
             [[widget]]\nkind = \"media\"\n",
        )
        .unwrap();
        remove_widget(&dir, 1).unwrap(); // remove media, not the invalid entry
        let text = fs::read_to_string(dir.join(CONFIG_FILE)).unwrap();
        assert!(text.contains("clock"));
        assert!(text.contains("hologram"), "invalid entry left in place");
        assert!(!text.contains("media"));
        assert!(remove_widget(&dir, 5).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_widget_swaps_neighbors() {
        let dir = scratch_dir("move-widget");
        fs::write(dir.join(CONFIG_FILE), DEFAULT_CONFIG).unwrap();
        move_widget(&dir, 0, false).unwrap(); // clock down past media
        let config = parse(&fs::read_to_string(dir.join(CONFIG_FILE)).unwrap()).unwrap();
        assert!(matches!(config.widgets[0], Widget::Media {}));
        assert!(matches!(config.widgets[1], Widget::Clock { .. }));
        move_widget(&dir, 1, true).unwrap(); // and back up
        let config = parse(&fs::read_to_string(dir.join(CONFIG_FILE)).unwrap()).unwrap();
        assert!(matches!(config.widgets[0], Widget::Clock { .. }));
        assert!(move_widget(&dir, 0, true).is_err(), "first can't move up");
        assert!(move_widget(&dir, 3, false).is_err(), "last can't move down");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_widget_value_edits_params_in_place() {
        let dir = scratch_dir("set-widget");
        fs::write(dir.join(CONFIG_FILE), DEFAULT_CONFIG).unwrap();
        // shortcut is widget 3: retitle it and repoint it
        set_widget_value(&dir, 3, "label", &serde_json::json!("Steam")).unwrap();
        set_widget_value(&dir, 3, "uri", &serde_json::json!("steam://open/games")).unwrap();
        set_widget_value(&dir, 3, "icon", &serde_json::json!("steam")).unwrap();
        // clock format flips, weather moves
        set_widget_value(&dir, 0, "format", &serde_json::json!("24h")).unwrap();
        set_widget_value(&dir, 0, "show_date", &serde_json::json!(false)).unwrap();
        set_widget_value(&dir, 2, "lat", &serde_json::json!(35.1)).unwrap();
        let text = fs::read_to_string(dir.join(CONFIG_FILE)).unwrap();
        assert!(text.contains("# Slab configuration"), "comments preserved");
        let config = parse(&text).unwrap();
        assert!(matches!(
            &config.widgets[3],
            Widget::Shortcut { label, uri, icon }
                if label == "Steam" && uri == "steam://open/games" && icon == "steam"
        ));
        assert!(matches!(
            config.widgets[0],
            Widget::Clock {
                format: ClockFormat::TwentyFour,
                show_date: false
            }
        ));
        assert!(matches!(config.widgets[2], Widget::Weather { lat, .. } if lat == 35.1));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_widget_value_rejects_invalidating_edits() {
        let dir = scratch_dir("set-widget-bad");
        fs::write(dir.join(CONFIG_FILE), DEFAULT_CONFIG).unwrap();
        let before = fs::read_to_string(dir.join(CONFIG_FILE)).unwrap();
        // weather lat must stay a number; kind is untouchable; bad index errors
        assert!(set_widget_value(&dir, 2, "lat", &serde_json::json!("north")).is_err());
        assert!(set_widget_value(&dir, 0, "kind", &serde_json::json!("media")).is_err());
        assert!(set_widget_value(&dir, 9, "label", &serde_json::json!("x")).is_err());
        assert!(set_widget_value(&dir, 0, "format", &serde_json::json!("13h")).is_err());
        assert_eq!(
            fs::read_to_string(dir.join(CONFIG_FILE)).unwrap(),
            before,
            "rejected edits must not touch the file"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn widget_ops_on_missing_config_start_from_defaults() {
        let dir = scratch_dir("widget-ops-missing");
        add_widget(&dir, "text").unwrap();
        let config = load_or_create(&dir);
        assert_eq!(config.widgets.len(), 5, "defaults plus the added widget");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_file_round_trips() {
        let dir = scratch_dir("valid");
        fs::write(
            dir.join(CONFIG_FILE),
            "autostart = false\n[theme]\naccent = \"#123456\"\n",
        )
        .unwrap();
        let config = load_or_create(&dir);
        assert!(!config.autostart);
        assert_eq!(config.theme.accent, "#123456");
        let _ = fs::remove_dir_all(&dir);
    }
}
