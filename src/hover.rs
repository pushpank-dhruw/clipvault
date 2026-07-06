//! Notch hover watcher: opens the shelf when the pointer enters a top-center
//! hot zone (the "notch") and closes it again when the pointer leaves.
//!
//! Wayland clients cannot read the global cursor position, so this polls
//! Hyprland's command socket (`.socket.sock`) for `cursorpos`. On other
//! compositors the watcher never starts and the shelf stays hotkey-only.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

/// Vertical gap in logical pixels between the reserved top area (status bar)
/// and the shelf. Must match the offset used when positioning the shelf.
pub const SHELF_TOP_GAP: f32 = 8.0;

/// Shelf is not shown.
pub const SHELF_HIDDEN: u8 = 0;
/// Shelf was opened via the toggle command; the watcher leaves it alone.
pub const SHELF_TOGGLED: u8 = 1;
/// Shelf was opened by notch hover; the watcher may auto-close it.
pub const SHELF_HOVERED: u8 = 2;

/// Signals shared between the GUI event loop and the hover watcher thread.
#[derive(Clone, Default)]
pub struct HoverSignals {
    /// Watcher asks the GUI to show the shelf.
    pub show: Arc<AtomicBool>,
    /// Watcher asks the GUI to hide a hover-opened shelf.
    pub hide: Arc<AtomicBool>,
    /// GUI publishes the current shelf state (one of the `SHELF_*` constants).
    pub shelf_state: Arc<AtomicU8>,
}

impl HoverSignals {
    /// Fresh signal set with the shelf considered hidden.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Geometry and timing of the notch hot zone.
pub struct HoverZone {
    /// Hot zone width in logical pixels, centered at the top of the monitor.
    pub width: f32,
    /// Minimum hot zone height from the top edge. A reserved top area
    /// (status bar) extends it so pointing at the bar center also triggers.
    pub height: f32,
    /// Shelf width, used to compute the keep-open region.
    pub shelf_width: f32,
    /// Shelf height, used to compute the keep-open region.
    pub shelf_height: f32,
    /// How long the pointer must dwell in the hot zone before the shelf opens,
    /// so merely sweeping the pointer across the top edge does not trigger it.
    pub dwell_ms: u64,
    /// Grace period before a hover-opened shelf closes once the pointer left.
    pub close_delay_ms: u64,
    /// Cursor poll interval in milliseconds.
    pub poll_ms: u64,
}

/// Start the watcher thread. Returns `false` when Hyprland's IPC socket is
/// not available (non-Hyprland session); hover-to-open is disabled then.
pub fn spawn(zone: HoverZone, signals: HoverSignals, egui_ctx: egui::Context) -> bool {
    let Some(socket) = socket_path() else {
        return false;
    };
    std::thread::spawn(move || watch(&socket, &zone, &signals, &egui_ctx));
    true
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn contains(&self, px: f64, py: f64) -> bool {
        let (px, py) = (px as f32, py as f32);
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug)]
struct MonGeom {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    reserved_top: f32,
    focused: bool,
}

/// Caches the monitor layout so the watcher does not query `j/monitors` on
/// every poll (monitors change rarely; the cursor does not).
struct MonCache {
    mons: Vec<MonGeom>,
    fetched: Option<Instant>,
}

impl MonCache {
    fn new() -> Self {
        Self {
            mons: Vec::new(),
            fetched: None,
        }
    }

    /// Monitor under the point, refreshing the cache when older than 2s.
    fn monitor_at(&mut self, socket: &Path, cx: f64, cy: f64) -> Option<MonGeom> {
        let stale = self
            .fetched
            .is_none_or(|t| t.elapsed() > Duration::from_secs(2));
        if stale && let Some(mons) = query_monitors(socket) {
            self.mons = mons;
            self.fetched = Some(Instant::now());
        }
        let mut focused = None;
        for m in &self.mons {
            let bounds = Rect {
                x: m.x,
                y: m.y,
                w: m.w,
                h: m.h,
            };
            if bounds.contains(cx, cy) {
                return Some(*m);
            }
            if m.focused {
                focused = Some(*m);
            }
        }
        focused
    }
}

fn watch(socket: &Path, zone: &HoverZone, signals: &HoverSignals, ctx: &egui::Context) {
    let poll = Duration::from_millis(zone.poll_ms.clamp(30, 1000));
    let close_delay = Duration::from_millis(zone.close_delay_ms);
    let dwell = Duration::from_millis(zone.dwell_ms);

    // Re-arm gate: after a trigger the pointer must leave the hot zone before
    // it can fire again. Tracked in every state (not just while hidden) so a
    // shelf dismissed with the pointer resting in the notch does not instantly
    // reopen.
    let mut armed = true;
    let mut keep_open: Option<Rect> = None;
    let mut last_inside = Instant::now();
    let mut dwell_since: Option<Instant> = None;
    let mut cache = MonCache::new();

    loop {
        std::thread::sleep(poll);
        let Some((cx, cy)) = cursor_pos(socket) else {
            continue;
        };

        let mon = cache.monitor_at(socket, cx, cy);
        let in_zone = mon.is_some_and(|m| hot_zone(&m, zone).contains(cx, cy));

        match signals.shelf_state.load(Ordering::SeqCst) {
            SHELF_HIDDEN => {
                if in_zone && armed {
                    // Require the pointer to dwell before opening.
                    let since = *dwell_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= dwell
                        && let Some(m) = mon
                    {
                        armed = false;
                        dwell_since = None;
                        last_inside = Instant::now();
                        keep_open = Some(keep_open_rect(&m, zone));
                        signals.show.store(true, Ordering::SeqCst);
                        ctx.request_repaint();
                    }
                } else {
                    dwell_since = None;
                    if !in_zone {
                        armed = true;
                    }
                }
            }
            SHELF_HOVERED => {
                if keep_open.is_some_and(|r| r.contains(cx, cy)) {
                    last_inside = Instant::now();
                } else if last_inside.elapsed() >= close_delay {
                    signals.hide.store(true, Ordering::SeqCst);
                    ctx.request_repaint();
                }
                armed = !in_zone;
            }
            _ => {
                // SHELF_TOGGLED: keep disarmed while the pointer sits in the
                // notch so dismissing the toggled shelf does not hover-reopen.
                armed = !in_zone;
                dwell_since = None;
            }
        }
    }
}

/// Top-center strip that triggers the shelf.
fn hot_zone(mon: &MonGeom, zone: &HoverZone) -> Rect {
    let w = zone.width.min(mon.w);
    Rect {
        x: mon.x + (mon.w - w) / 2.0,
        y: mon.y,
        w,
        h: zone.height.max(mon.reserved_top),
    }
}

/// Region the pointer may roam without closing a hover-opened shelf: the hot
/// zone, the shelf itself, and the gap between them, inflated for forgiveness.
fn keep_open_rect(mon: &MonGeom, zone: &HoverZone) -> Rect {
    const MARGIN: f32 = 24.0;
    let hot = hot_zone(mon, zone);
    let shelf_w = zone.shelf_width.min(mon.w);
    let shelf_x = mon.x + (mon.w - shelf_w) / 2.0;
    let bottom = mon.y + mon.reserved_top + SHELF_TOP_GAP + zone.shelf_height;
    let left = shelf_x.min(hot.x) - MARGIN;
    let right = (shelf_x + shelf_w).max(hot.x + hot.w) + MARGIN;
    Rect {
        x: left,
        y: mon.y,
        w: right - left,
        h: bottom - mon.y + MARGIN,
    }
}

fn socket_path() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let path = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    path.exists().then_some(path)
}

/// One-shot request over Hyprland's command socket (one connection per
/// query, mirroring hyprctl, but without spawning a process).
fn request(socket: &Path, cmd: &str) -> Option<String> {
    let timeout = Some(Duration::from_millis(500));
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(timeout).ok()?;
    stream.set_write_timeout(timeout).ok()?;
    stream.write_all(cmd.as_bytes()).ok()?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    Some(buf)
}

/// Global cursor position in logical (layout) coordinates.
fn cursor_pos(socket: &Path) -> Option<(f64, f64)> {
    let raw = request(socket, "j/cursorpos")?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some((v["x"].as_f64()?, v["y"].as_f64()?))
}

/// Logical geometry of every monitor.
fn query_monitors(socket: &Path) -> Option<Vec<MonGeom>> {
    let raw = request(socket, "j/monitors")?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(v.as_array()?.iter().filter_map(mon_geom).collect())
}

fn mon_geom(m: &serde_json::Value) -> Option<MonGeom> {
    let scale = m["scale"].as_f64().filter(|s| *s > 0.0).unwrap_or(1.0);
    let mut w = m["width"].as_f64()? / scale;
    let mut h = m["height"].as_f64()? / scale;
    // Odd transforms are 90/270 degree rotations; mode size is pre-transform.
    if m["transform"].as_i64().unwrap_or(0) % 2 == 1 {
        std::mem::swap(&mut w, &mut h);
    }
    let reserved_top = m["reserved"]
        .as_array()
        .and_then(|r| r.get(1))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    Some(MonGeom {
        x: m["x"].as_f64()? as f32,
        y: m["y"].as_f64()? as f32,
        w: w as f32,
        h: h as f32,
        reserved_top: reserved_top as f32,
        focused: m["focused"].as_bool() == Some(true),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: MonGeom = MonGeom {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
        reserved_top: 26.0,
        focused: true,
    };

    const ZONE: HoverZone = HoverZone {
        width: 300.0,
        height: 8.0,
        shelf_width: 800.0,
        shelf_height: 140.0,
        dwell_ms: 120,
        close_delay_ms: 400,
        poll_ms: 100,
    };

    #[test]
    fn should_trigger_on_top_center() {
        assert!(hot_zone(&MON, &ZONE).contains(960.0, 10.0));
    }

    #[test]
    fn should_not_trigger_off_center() {
        assert!(!hot_zone(&MON, &ZONE).contains(200.0, 10.0));
    }

    #[test]
    fn should_not_trigger_below_reserved_strip() {
        assert!(!hot_zone(&MON, &ZONE).contains(960.0, 40.0));
    }

    #[test]
    fn should_keep_open_inside_shelf() {
        assert!(keep_open_rect(&MON, &ZONE).contains(960.0, 100.0));
    }

    #[test]
    fn should_close_below_shelf() {
        assert!(!keep_open_rect(&MON, &ZONE).contains(960.0, 300.0));
    }

    #[test]
    fn should_close_beside_shelf() {
        assert!(!keep_open_rect(&MON, &ZONE).contains(200.0, 100.0));
    }
}
