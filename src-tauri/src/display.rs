//! Monitor enumeration and placement of the dashboard window on the HYTE panel.

use tauri::{
    AppHandle, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

/// Physical resolution of the HYTE Y70 Touch panel (portrait).
const PANEL_WIDTH: u32 = 1100;
const PANEL_HEIGHT: u32 = 3840;

/// The Y70 panel might not be running exactly 1100x3840 — Windows can pick a
/// different mode. Its shape is unmistakable though: an extreme strip
/// (≥3:1) with a short side no bigger than the panel's 1100 and a long side
/// in screen territory. No desktop monitor or ultrawide matches this: 32:9
/// ultrawides have a 1440+ short side, 21:9 are under 3:1.
fn looks_like_panel(width: u32, height: u32) -> bool {
    let short = width.min(height);
    let long = width.max(height);
    short > 0 && long >= 3 * short && short <= 1200 && long >= 1900
}

/// Create the dashboard window: borderless always-on-bottom fullscreen on the
/// HYTE panel when present, otherwise a fixed-ratio miniature of the panel on
/// the primary monitor (the display picker UI is a later milestone).
///
/// In both cases the webview zoom is set so the page lays out at exactly
/// 1100x3840 CSS pixels — the frontend is designed against the panel's real
/// dimensions and the preview is that same layout, scaled.
pub fn create_dashboard_window(app: &AppHandle) -> tauri::Result<()> {
    let monitors = app.available_monitors()?;
    for monitor in &monitors {
        log::info!(
            "monitor {:?}: {}x{} at {},{} (scale {})",
            monitor.name(),
            monitor.size().width,
            monitor.size().height,
            monitor.position().x,
            monitor.position().y,
            monitor.scale_factor()
        );
    }
    let panel = monitors
        .into_iter()
        .find(|m| looks_like_panel(m.size().width, m.size().height));

    match panel {
        Some(monitor) => {
            log::info!(
                "HYTE panel detected: {}x{} at {},{}",
                monitor.size().width,
                monitor.size().height,
                monitor.position().x,
                monitor.position().y
            );
            // Deliberately NOT always-on-bottom: HWND_BOTTOM sinks the window
            // beneath Wallpaper Engine's wallpaper layer, burying the
            // dashboard entirely. The panel is a dedicated surface — normal
            // z-order with focus-steal suppression is the correct behavior.
            let window = base_builder(app)
                .decorations(false)
                .resizable(false)
                .focused(false)
                .visible(false)
                .skip_taskbar(true)
                .build()?;
            // Fill the monitor's WORK AREA: everything except a taskbar shown
            // on the panel. An always-on-bottom window can never paint over
            // the taskbar, so matching its edge is the correct fit — and when
            // the taskbar is hidden/auto-hidden there, the work area IS the
            // full panel and Slab covers edge to edge.
            let (position, size) = panel_work_area(&monitor);
            log::info!(
                "placing dashboard at {},{} {}x{}",
                position.x,
                position.y,
                size.width,
                size.height
            );
            window.set_position(position)?;
            window.set_size(size)?;
            // The layout is width-proportional (vw-based rem in CSS), so the
            // page adapts to whatever resolution the panel reports. Only DPI
            // scaling needs cancelling so CSS pixels equal panel pixels.
            if (monitor.scale_factor() - 1.0).abs() > f64::EPSILON {
                set_zoom(&window, 1.0 / monitor.scale_factor());
            }
            suppress_focus_steal(&window);
            window.show()?;
            // Crossing onto a monitor with a different DPI can shave the
            // window during show (WM_DPICHANGED); assert geometry again so
            // the work area is covered exactly.
            window.set_position(position)?;
            window.set_size(size)?;
            if let (Ok(actual_position), Ok(actual_size)) =
                (window.outer_position(), window.outer_size())
            {
                log::info!(
                    "dashboard now at {},{} {}x{}",
                    actual_position.x,
                    actual_position.y,
                    actual_size.width,
                    actual_size.height
                );
            }
        }
        None => {
            log::warn!(
                "no {PANEL_WIDTH}x{PANEL_HEIGHT} panel found; opening a scaled preview on the primary monitor"
            );
            let scale = preview_scale(app);
            // vw-based layout: the miniature scales itself to its width, no
            // webview zoom needed.
            base_builder(app)
                .title("Slab (preview)")
                .inner_size(PANEL_WIDTH as f64 * scale, PANEL_HEIGHT as f64 * scale)
                .resizable(false)
                .build()?;
        }
    }
    Ok(())
}

fn base_builder(app: &AppHandle) -> WebviewWindowBuilder<'_, tauri::Wry, AppHandle> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into())).title("Slab")
}

/// The customization window ("open Slab" experience, Wallpaper Engine style):
/// a normal decorated window on the primary monitor with the background
/// selector and theme controls. Hidden on autostart; lives in the tray.
pub fn create_settings_window(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Slab")
        .inner_size(1180.0, 800.0)
        .min_inner_size(760.0, 520.0)
        .visible(visible)
        .build()?;
    Ok(())
}

/// Preview scale: the whole 3840-tall panel fits in ~85% of the primary
/// monitor's logical height, keeping the exact panel aspect ratio.
fn preview_scale(app: &AppHandle) -> f64 {
    let logical_height = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.size().height as f64 / m.scale_factor())
        .unwrap_or(1080.0);
    (logical_height * 0.85) / PANEL_HEIGHT as f64
}

/// The monitor's work area (bounds minus any taskbar), physical pixels.
/// Falls back to the full bounds if the query fails.
fn panel_work_area(monitor: &tauri::Monitor) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };

    let center = POINT {
        x: monitor.position().x + (monitor.size().width / 2) as i32,
        y: monitor.position().y + (monitor.size().height / 2) as i32,
    };
    unsafe {
        let handle = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(handle, &mut info).as_bool() {
            let work = info.rcWork;
            return (
                PhysicalPosition::new(work.left, work.top),
                PhysicalSize::new(
                    (work.right - work.left).max(0) as u32,
                    (work.bottom - work.top).max(0) as u32,
                ),
            );
        }
    }
    log::warn!("could not query the panel's work area; using full bounds");
    (
        PhysicalPosition::new(monitor.position().x, monitor.position().y),
        PhysicalSize::new(monitor.size().width, monitor.size().height),
    )
}

fn set_zoom(window: &WebviewWindow, zoom: f64) {
    if let Err(e) = window.set_zoom(zoom) {
        log::warn!("could not set webview zoom {zoom:.3}: {e}");
    }
}

/// The dashboard must never take foreground activation from the user's app
/// when touched. `WS_EX_NOACTIVATE` makes Windows deliver input without
/// activating the window; tap targets keep working.
fn suppress_focus_steal(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };

    match window.hwnd() {
        Ok(hwnd) => unsafe {
            let hwnd = HWND(hwnd.0);
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE.0 as isize);
        },
        Err(e) => log::warn!("could not set WS_EX_NOACTIVATE: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::looks_like_panel;

    #[test]
    fn y70_modes_match_either_orientation() {
        assert!(looks_like_panel(1100, 3840));
        assert!(looks_like_panel(3840, 1100));
        assert!(looks_like_panel(1080, 3840));
        assert!(looks_like_panel(550, 1920));
    }

    #[test]
    fn ordinary_and_ultrawide_monitors_do_not_match() {
        assert!(!looks_like_panel(1920, 1080));
        assert!(!looks_like_panel(2560, 1440));
        assert!(!looks_like_panel(3840, 2160));
        assert!(!looks_like_panel(5120, 1440)); // 32:9 — short side too big
        assert!(!looks_like_panel(3440, 1440)); // 21:9 — under 3:1
        assert!(!looks_like_panel(2560, 1080)); // 21:9 — under 3:1
        assert!(!looks_like_panel(0, 0));
    }
}
