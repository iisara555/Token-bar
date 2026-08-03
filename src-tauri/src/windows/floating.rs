//! The always-on-top pill.
//!
//! Position is stored **per monitor name**, not as a bare x/y. A single saved
//! coordinate is wrong the moment a laptop is undocked: the bar reappears in
//! dead space on a display that no longer exists.

use tauri::{Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

use crate::state::AppState;

/// Distance from a screen edge at which the bar clings to it.
const SNAP_LOGICAL: f64 = 24.0;
/// Breathing room kept between the bar and the edge it snapped to.
const MARGIN_LOGICAL: f64 = 16.0;

/// Height the OS reserves for its own bar along the top of every display.
///
/// On macOS this is the menu bar, which the window server will happily let a
/// borderless always-on-top window sit underneath — so nothing stops the bar
/// from being placed there, it just permanently covers the clock and the menu
/// bar extras, this app's own tray icon among them.
#[cfg(target_os = "macos")]
const TOP_INSET_LOGICAL: f64 = 26.0;
/// Windows reserves nothing at the top by default; a taskbar docked there is
/// handled by the snap logic instead, which can see where it actually is.
#[cfg(not(target_os = "macos"))]
const TOP_INSET_LOGICAL: f64 = 0.0;

/// The layout constants in device pixels for a particular display.
///
/// These were plain `i32` pixel constants, which meant a 24px snap threshold
/// became 24 *device* pixels — 12pt — on any Retina MacBook, so the bar had to
/// be dragged to within half the intended distance of an edge before it would
/// catch, and then sat half as far from that edge as it should. Every one of
/// them is a physical measurement of a thing a hand is aiming at, so every one
/// of them scales.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    snap: i32,
    margin: i32,
    top_inset: i32,
}

impl Metrics {
    fn for_scale(scale: f64) -> Self {
        // A monitor reporting a nonsensical scale would otherwise collapse every
        // threshold to zero and the bar would never snap at all.
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        Self {
            snap: (SNAP_LOGICAL * scale).round() as i32,
            margin: (MARGIN_LOGICAL * scale).round() as i32,
            top_inset: (TOP_INSET_LOGICAL * scale).round() as i32,
        }
    }
}

fn monitor_key<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Option<(String, PhysicalPosition<i32>, PhysicalSize<u32>, Metrics)> {
    let m = window.current_monitor().ok().flatten()?;
    let name = m.name().cloned().unwrap_or_else(|| "primary".to_string());
    Some((
        name,
        *m.position(),
        *m.size(),
        Metrics::for_scale(m.scale_factor()),
    ))
}

/// Put the bar back where the user left it on this monitor, or top-centre on
/// first run.
pub fn restore_position<R: Runtime>(window: &WebviewWindow<R>) {
    let Some((name, origin, size, m)) = monitor_key(window) else {
        return;
    };

    let saved = {
        let state = window.app_handle().state::<AppState>();
        state.config.get().bar.positions.get(&name).copied()
    };

    let win = window.outer_size().unwrap_or(PhysicalSize::new(560, 64));

    let (x, y) = match saved {
        Some((x, y)) => (x, y),
        // First run: top-centre, below whatever the OS owns up there. On a Mac
        // this is what keeps the bar from launching straight on top of the menu
        // bar extras — including this app's own.
        None => (
            origin.x + (size.width as i32 - win.width as i32) / 2,
            origin.y + m.top_inset + m.margin,
        ),
    };

    let clamped = clamp_to_monitor(x, y, win, origin, size, m.top_inset);
    let _ = window.set_position(PhysicalPosition::new(clamped.0, clamped.1));
}

/// Persist where the bar now sits, keyed by the monitor it sits on.
pub fn save_position<R: Runtime>(window: &WebviewWindow<R>) {
    let Some((name, _, _, _)) = monitor_key(window) else {
        return;
    };
    let Ok(pos) = window.outer_position() else {
        return;
    };

    let state = window.app_handle().state::<AppState>();
    let _ = state.config.update(|c| {
        c.bar.positions.insert(name, (pos.x, pos.y));
    });
}

/// Pull the bar flush to whichever edge it was dropped near, then save.
pub fn snap_to_edge<R: Runtime>(window: &WebviewWindow<R>) {
    let Some((_, origin, size, m)) = monitor_key(window) else {
        return;
    };
    let Ok(pos) = window.outer_position() else {
        return;
    };
    let win = window.outer_size().unwrap_or(PhysicalSize::new(560, 64));

    let left = origin.x;
    // The top edge the bar snaps to is the top of the *usable* screen, not of
    // the display — snapping onto the menu bar is not a place anyone dragged to.
    let top = origin.y + m.top_inset;
    let right = origin.x + size.width as i32;
    let bottom = origin.y + size.height as i32;

    let mut x = pos.x;
    let mut y = pos.y;

    if (x - left).abs() <= m.snap {
        x = left + m.margin;
    } else if (right - (x + win.width as i32)).abs() <= m.snap {
        x = right - win.width as i32 - m.margin;
    }

    if (y - top).abs() <= m.snap {
        y = top + m.margin;
    } else if (bottom - (y + win.height as i32)).abs() <= m.snap {
        y = bottom - win.height as i32 - m.margin;
    }

    let (x, y) = clamp_to_monitor(x, y, win, origin, size, m.top_inset);
    let _ = window.set_position(PhysicalPosition::new(x, y));
    save_position(window);
}

/// Never leave the bar somewhere it cannot be grabbed.
fn clamp_to_monitor(
    x: i32,
    y: i32,
    win: PhysicalSize<u32>,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    top_inset: i32,
) -> (i32, i32) {
    let min_y = origin.y + top_inset;
    let max_x = origin.x + size.width as i32 - win.width as i32;
    let max_y = origin.y + size.height as i32 - win.height as i32;
    (
        x.clamp(origin.x, max_x.max(origin.x)),
        y.clamp(min_y, max_y.max(min_y)),
    )
}

/// Click-through: the bar stays visible but stops swallowing mouse input.
pub fn set_click_through<R: Runtime>(window: &WebviewWindow<R>, on: bool) {
    let _ = window.set_ignore_cursor_events(on);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_the_bar_on_screen() {
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(1920u32, 1080u32);
        let win = PhysicalSize::new(560u32, 64u32);

        assert_eq!(clamp_to_monitor(-500, -500, win, origin, screen, 0), (0, 0));
        assert_eq!(
            clamp_to_monitor(9999, 9999, win, origin, screen, 0),
            (1920 - 560, 1080 - 64)
        );
        assert_eq!(clamp_to_monitor(100, 100, win, origin, screen, 0), (100, 100));
    }

    #[test]
    fn clamp_respects_a_secondary_monitor_offset() {
        // A display sitting to the left of the primary has a negative origin.
        let origin = PhysicalPosition::new(-1920, 0);
        let screen = PhysicalSize::new(1920u32, 1080u32);
        let win = PhysicalSize::new(560u32, 64u32);
        assert_eq!(clamp_to_monitor(-5000, 0, win, origin, screen, 0), (-1920, 0));
    }

    #[test]
    fn clamp_survives_a_window_wider_than_the_screen() {
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(400u32, 300u32);
        let win = PhysicalSize::new(560u32, 64u32);
        // max_x would be negative; the bar pins to the left edge rather than
        // panicking on an inverted clamp range.
        assert_eq!(clamp_to_monitor(50, 10, win, origin, screen, 0), (0, 10));
    }

    #[test]
    fn clamp_keeps_the_bar_clear_of_a_reserved_top_strip() {
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(1920u32, 1080u32);
        let win = PhysicalSize::new(560u32, 64u32);
        // 52 device pixels is a 26pt macOS menu bar on a 2x display.
        assert_eq!(clamp_to_monitor(100, 0, win, origin, screen, 52), (100, 52));
        assert_eq!(clamp_to_monitor(100, 300, win, origin, screen, 52), (100, 300));
    }

    #[test]
    fn a_reserved_strip_taller_than_the_screen_does_not_invert_the_clamp() {
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(400u32, 80u32);
        let win = PhysicalSize::new(560u32, 64u32);
        // max_y (16) is below min_y (52). The bar pins to the inset rather than
        // panicking on an inverted range, matching how max_x already behaves.
        assert_eq!(clamp_to_monitor(0, 0, win, origin, screen, 52), (0, 52));
    }

    #[test]
    fn metrics_track_the_display_scale() {
        // The thresholds are hand-aimed distances, so they are the same physical
        // size on a Retina display as on a 1x one — twice the device pixels.
        let one_x = Metrics::for_scale(1.0);
        let two_x = Metrics::for_scale(2.0);
        assert_eq!(one_x.snap, 24);
        assert_eq!(two_x.snap, 48);
        assert_eq!(two_x.margin, 32);
    }

    #[test]
    fn a_nonsense_scale_factor_falls_back_to_1x() {
        // Rounding 24 * 0.0 to zero would leave a snap threshold that can never
        // be met, so the bar would silently stop snapping at all.
        for bad in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            assert_eq!(Metrics::for_scale(bad).snap, 24, "scale {bad}");
        }
    }
}
