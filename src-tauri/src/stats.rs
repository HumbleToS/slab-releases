//! System stats sampling for the `stats` widget: CPU load, RAM, and drive
//! usage via `sysinfo`, pushed as `stats-update` events every two seconds.
//!
//! Sampling only runs while a stats widget is actually configured — an idle
//! dashboard without one costs nothing but the config peek.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use sysinfo::{Disks, System};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Widget;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// Last emitted updates, replayed when the webview (re)loads so the meters
/// don't sit empty until the next sample.
#[derive(Default)]
pub struct StatsState {
    pub last: Mutex<Vec<StatsUpdate>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsUpdate {
    /// Index of the widget in the config's widget list, so the frontend can
    /// route updates when several stats widgets exist.
    pub widget_index: usize,
    pub cpu_percent: f64,
    pub ram_percent: f64,
    pub ram_text: String,
    /// None when the widget hides the disk meter or the drive wasn't found.
    pub disk_percent: Option<f64>,
    pub disk_text: Option<String>,
}

/// Sample on a dedicated thread — sysinfo calls are blocking I/O.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut sys = System::new();
        loop {
            sample(&app, &mut sys);
            std::thread::sleep(SAMPLE_INTERVAL);
        }
    });
}

fn sample(app: &AppHandle, sys: &mut System) {
    let widgets = crate::lock(&app.state::<crate::AppState>().config)
        .widgets
        .clone();
    let stats_widgets: Vec<(usize, bool, String)> = widgets
        .iter()
        .enumerate()
        .filter_map(|(index, widget)| match widget {
            Widget::Stats {
                show_disk, disk, ..
            } => Some((index, *show_disk, disk.clone())),
            _ => None,
        })
        .collect();
    if stats_widgets.is_empty() {
        return;
    }

    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let cpu_percent = f64::from(sys.global_cpu_usage());
    let ram_percent = percent(sys.used_memory(), sys.total_memory());
    let ram_text = usage_text(sys.used_memory(), sys.total_memory());
    let disks = stats_widgets
        .iter()
        .any(|(_, show_disk, _)| *show_disk)
        .then(Disks::new_with_refreshed_list);

    let mut updates = Vec::new();
    for (widget_index, show_disk, want) in stats_widgets {
        let mut update = StatsUpdate {
            widget_index,
            cpu_percent,
            ram_percent,
            ram_text: ram_text.clone(),
            disk_percent: None,
            disk_text: None,
        };
        if show_disk {
            if let Some(disk) = disks.as_ref().and_then(|d| find_disk(d, &want)) {
                let total = disk.total_space();
                let used = total.saturating_sub(disk.available_space());
                update.disk_percent = Some(percent(used, total));
                update.disk_text = Some(usage_text(used, total));
            }
        }
        if let Err(e) = app.emit("stats-update", &update) {
            log::warn!("could not emit stats-update: {e}");
        }
        updates.push(update);
    }
    *crate::lock(&app.state::<StatsState>().last) = updates;
}

fn find_disk<'a>(disks: &'a Disks, want: &str) -> Option<&'a sysinfo::Disk> {
    disks
        .list()
        .iter()
        .find(|disk| mount_matches(&disk.mount_point().to_string_lossy(), want))
}

/// `disk = "C:"` in config must match the `C:\` mount point, any case,
/// trailing slash or not.
fn mount_matches(mount: &str, want: &str) -> bool {
    mount
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(want.trim_end_matches(['\\', '/']))
}

fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    used as f64 / total as f64 * 100.0
}

/// Bytes → `"12.3 / 31.9 GB"`. Binary gigabytes to match Task Manager; three
/// significant-ish digits so a 2 TB drive doesn't read "1863.0".
fn usage_text(used: u64, total: u64) -> String {
    format!("{} / {} GB", gb(used), gb(total))
}

fn gb(bytes: u64) -> String {
    let value = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn percent_handles_zero_total() {
        assert_eq!(percent(5, 0), 0.0);
        assert_eq!(percent(1, 4), 25.0);
        assert_eq!(percent(0, 4), 0.0);
    }

    #[test]
    fn usage_text_formats_gigabytes() {
        assert_eq!(
            usage_text(12 * GIB + GIB * 3 / 10, 32 * GIB),
            "12.3 / 32.0 GB"
        );
        assert_eq!(usage_text(500 * GIB, 1863 * GIB), "500 / 1863 GB");
        assert_eq!(usage_text(0, 0), "0.0 / 0.0 GB");
    }

    #[test]
    fn mount_matching_forgives_case_and_slashes() {
        assert!(mount_matches("C:\\", "C:"));
        assert!(mount_matches("c:", "C:\\"));
        assert!(mount_matches("D:/", "d:"));
        assert!(!mount_matches("C:\\", "D:"));
    }
}
