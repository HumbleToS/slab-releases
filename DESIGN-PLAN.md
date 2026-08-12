# Slab — Design Plan

*The screen was always a slab. Now it acts like one of yours.*

## What this is

A native Windows app that turns the HYTE Y70 Touch case screen into a fully custom, fully touch-driven dashboard. It replaces the v1 prototype (static HTML + PowerShell media server + bat launcher) with a single installable Rust app. Built first for the reference setup, designed so any HYTE Y70 Touch owner can install it, theme it, and arrange their own widgets.

## Why native (lessons from v1)

The prototype worked but every moving part fought Windows:

- Smart App Control blocked the .bat and .ps1 (mark-of-the-web on downloaded scripts)
- Browser kiosk launch depended on which browser was installed and PATH quirks (no Chrome → Edge fallback)
- Monitor targeting via `--window-position` required the user to know their display layout
- Media info required a localhost PowerShell sidecar polling GSMTC, and PS 5.1 WinRT interop is fragile (returned `sessions: 0` on the target machine)
- Media control was limited to synthetic media-key presses — no shuffle, repeat, seek, or album art

A signed native exe with direct WinRT bindings eliminates all five.

## Product pillars

1. **Touch-first.** The panel is a touchscreen and the app must be fully operable by finger alone — no mouse, no keyboard, ever.
2. **Fully customizable.** Widgets, their order, their settings, and the entire visual theme live in user config. The shipped defaults are one possible dashboard, not the dashboard.
3. **Zero-friction install.** Installer → working screen. No scripts, no display coordinates, no browser flags.

## Architecture

**Shell:** Tauri v2. The v1 HTML/CSS frontend ports nearly unchanged into the webview. Rust backend owns everything system-facing.

**Backend crates:**
- `windows` — GSMTC (`Windows.Media.Control`) for now-playing metadata, playback status, album-art thumbnail stream, and session control (play/pause/next/prev/shuffle/repeat). Event-driven via `MediaPropertiesChanged` / `PlaybackInfoChanged` instead of polling.
- `tauri-plugin-autostart` — launch on login, no shell:startup shortcut.
- `serde` + `toml` — user config file.
- `notify` — watch config.toml and hot-reload on save, so customizing never requires a restart.
- `reqwest` — Open-Meteo fetch (no API key), cached, 10-minute refresh.

**Frontend:** vanilla HTML/CSS/JS inside Tauri's webview. No framework — the UI is a widget column, a framework would be ceremony. All colors, fonts, and dimensions route through CSS custom properties that the backend populates from the theme config at load and on hot-reload.

**IPC:** Tauri commands + events. Backend pushes media/weather/config state to the webview; webview sends control intents back. No localhost HTTP server anywhere.

## Monitor targeting

On startup, enumerate displays and auto-detect the HYTE panel by its shape: an extreme strip (≥3:1, short side ≤1200). Field reality (2026-08-11): the reference Y70 Touch reports native 682x2560, not the 1100x3840 this plan originally assumed — detection is therefore shape-based rather than exact-resolution, and the webview zoom normalizes whatever mode the panel runs to the 1100-wide design layout. Place a borderless, always-on-bottom fullscreen window there. If not found, show a touch-friendly display picker and remember the choice in config. This removes the single biggest setup landmine from v1.

## Customization model

Everything user-facing lives in `config.toml`. Two halves:

**Theme — every visual token is configurable:**

```toml
[theme]
accent        = "#e4553b"
panel_color   = "#0a0a0c"
panel_opacity = 0.42
blur          = 22
radius        = 26
font          = "Inter"        # any installed font
text_scale    = 1.0
background    = 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960'   # file, or folder to scan; gradient fallback
shade         = 0.45           # darkness of the legibility overlay
```

**Widgets — order in file = order on screen, each with its own params:**

```toml
[[widget]]
kind = "clock"
format = "12h"                 # or "24h"
show_date = true

[[widget]]
kind = "media"
show_art = true                # M2

[[widget]]
kind = "weather"
lat = 32.7026
lon = -103.136
label = "Hobbs, NM"
unit = "fahrenheit"

[[widget]]
kind = "shortcut"
label = "Discord"
uri = "discord://-/"
icon = "discord"               # built-in icon set, or path to an svg

[[widget]]
kind = "stats"
show_cpu = true                # live meters, sampled every 2s
show_ram = true
show_disk = true
disk = "C:"                    # which drive the disk meter watches

[[widget]]
kind = "text"
text = "arcade"                # static label
size = "medium"                # small / medium / large
align = "left"                 # or "center"
```

Rules: unknown keys are ignored, unknown widget kinds are skipped with a log line, a missing/corrupt config regenerates defaults (the broken file is preserved as `config.toml.invalid`). Config changes hot-reload via the file watcher — edit, save, watch the screen update. A future in-app settings panel (M3) writes to this same file; the file is always the truth.

`background` accepts a file or a folder: a folder (the default is Wallpaper Engine's workshop content directory) is scanned two levels deep for its newest video/image, so a freshly downloaded wallpaper becomes the background on the next reload without editing config. Missing/unsupported → built-in gradient derived from the theme's panel and accent colors.

## Touch requirements (non-negotiable, all milestones)

- Every interaction reachable by touch alone — no hover-dependent UI, no right-click, no keyboard-only paths
- Touch targets ≥ 90px with visible pressed states on everything interactive
- Scrollable regions use native touch inertia
- No OS text-selection or long-press callout artifacts on the dashboard surface
- The always-on-bottom window must not steal focus from games/apps when touched

## Design language (shipped default theme)

The v2 look ships as the default `[theme]` values:

- Near-black glass panels (`rgba(10,10,12,.42)`, 22px blur) on hairline borders
- Inter at weights 200–500; huge ultra-light clock floating directly on the wallpaper, no card
- Single accent `#e4553b`, used only for the sun-position dot on the weather bar
- Generous spacing so the wallpaper reads through; the dashboard frames the video, never buries it

These are defaults, not law: users change any of it in `[theme]`. The law is structural — nothing in the frontend may hardcode a themable value.

Derived tokens (computed by theme.rs so the frontend never invents a color): `--fg-rgb` (white on dark panels, near-black on light ones, by panel luminance), `--hairline` (foreground at 8% for the hairline borders), `--shade-overlay` (black at the configured `shade`). The no-background gradient blends the panel color toward the accent.

## Milestones

**M1 — parity, themable.** Tauri shell, auto monitor targeting, four widgets from config with per-widget params, full `[theme]` support with hot-reload, GSMTC media widget (metadata + play/pause/next/prev), weather, video/image background, autostart, all touch requirements. Amendment (2026-08-10): a minimal on-dashboard customization panel ships in M1 — gear toggle, accent presets, opacity/shade/text-size sliders — writing single `[theme]` keys back to config.toml so the file stays the source of truth; the full settings panel remains M3 and grows from this seed. Also pulled forward from M3 (same day): the NSIS installer (per-user, no admin) and a signed auto-updater against github.com/HumbleToS/slab-releases, so new builds reach the reference machine without file-passing. Code signing remains an M3 {{CONFIRM}}.

Second amendment (2026-08-10) — the Wallpaper Engine model: Slab lives in the system tray (Open Slab / Quit); launching it by hand opens a customization window on the primary monitor with a background selector (browses the configured folder + the WE workshop directory, WE titles from project.json, first-frame previews) and the shared theme controls; autostart goes straight to tray. The dashboard window itself is not part of the desktop flow — no taskbar entry, always-on-bottom on the panel. Backgrounds are never auto-picked: a folder in `background` only tells the selector where to browse; gradient until the user chooses. The dashboard keeps its own touch quick-panel (gear). Exit test: fresh Windows 11 machine with Smart App Control on, installer to working dashboard with zero script warnings and zero manual configuration — then change the accent color in config.toml, save, and watch it update live without restart.

Third amendment (2026-08-11) — more widget kinds: the M2 "additional widget kinds" land in M1 at the maintainer's request. `stats` shows CPU / RAM / drive meters via sysinfo, sampled every two seconds on a dedicated thread that idles when no stats widget is configured; the meter bars draw the accent on the hairline track (v2 defaults — both already theme tokens, nothing new to configure). `text` is a static label rendered bare like the clock, with `size` (small/medium/large) and `align` (left/center) params. Both ship as commented-out examples in the default config rather than changing the shipped dashboard. Temps remain out (the LibreHardwareMonitor open question stands).

**M2 — beyond parity.** Album art, shuffle/repeat, seek bar, display picker UI, tray icon with quit/restart/open-config, further widget kinds beyond stats/text as demand appears.

**M3 — shareable.** MSI/NSIS installer via Tauri bundler, code signing decision ({{CONFIRM}} — cert costs money), in-app touch settings panel that writes config.toml, scene.pkg auto-extract so users skip RePKG, README for non-dev HYTE owners.

## Open questions

- Code signing cert vs. telling users to click through SmartScreen ({{CONFIRM}}, M3)
- Whether weather location should geolocate by IP as a default instead of shipping Hobbs coords
- Whether M2 system-stats widget needs LibreHardwareMonitor interop for temps (sysinfo can't read them) — decide when we get there