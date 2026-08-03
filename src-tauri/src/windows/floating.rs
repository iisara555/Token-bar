//! The always-on-top pill.
//!
//! Position is stored **per monitor name**, not as a bare x/y. A single saved
//! coordinate is wrong the moment a laptop is undocked: the bar reappears in
//! dead space on a display that no longer exists.

use tauri::{Manager, Monitor, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

use crate::state::AppState;

/// Distance from a screen edge at which the bar clings to it.
const SNAP_LOGICAL: f64 = 24.0;
/// Breathing room kept between the bar and the edge it snapped to.
const MARGIN_LOGICAL: f64 = 16.0;

/// Fallback for the strip the OS keeps at the top of a display.
///
/// The real figure is measured from the monitor's work area; this is only the
/// floor applied on macOS, where a menu bar always exists even if the work area
/// were to come back reporting otherwise.
#[cfg(target_os = "macos")]
const TOP_INSET_LOGICAL: f64 = 26.0;
/// Windows reserves nothing at the top by default; a taskbar docked there shows
/// up in the work area and is handled from there.
#[cfg(not(target_os = "macos"))]
const TOP_INSET_LOGICAL: f64 = 0.0;

/// A menu bar taller than this only occurs on a display with a camera housing.
/// A standard Mac menu bar is 24pt; a notched one is around 37pt.
const NOTCHED_MENU_BAR_MIN_LOGICAL: f64 = 32.0;

/// Share of the display width to treat as occupied by the camera housing.
///
/// AppKit publishes the exact bounds through `NSScreen.auxiliaryTopLeftArea`,
/// but reaching that means taking on an Objective-C dependency for one
/// rectangle. Every notched Mac so far keeps the housing inside the middle ~13%
/// of the display — 200pt of 1512 on the 14", 185 of 1710 on the 15" Air — so
/// this rounds up to 14%. Rounding up costs a little usable width at the top of
/// the screen; rounding down would park the bar behind the housing, which is
/// the failure the setting exists to avoid.
const NOTCH_WIDTH_FRACTION: f64 = 0.14;

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
    /// The highest the bar is allowed to go. Zero when it may enter the menu bar.
    top_inset: i32,
    /// What the OS actually reserves at the top, whatever the bar is allowed to
    /// do. Used to tell whether the bar is currently sitting in that strip.
    reserved_top: i32,
    /// Absolute x range the camera housing occupies, when there is one and the
    /// bar is allowed up there.
    notch: Option<(i32, i32)>,
}

impl Metrics {
    fn for_scale(scale: f64) -> Self {
        // A monitor reporting a nonsensical scale would otherwise collapse every
        // threshold to zero and the bar would never snap at all.
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let top_inset = (TOP_INSET_LOGICAL * scale).round() as i32;
        Self {
            snap: (SNAP_LOGICAL * scale).round() as i32,
            margin: (MARGIN_LOGICAL * scale).round() as i32,
            top_inset,
            reserved_top: top_inset,
            notch: None,
        }
    }

    /// Measure the display rather than assuming it.
    ///
    /// The work area is the OS stating what it has already taken — the menu bar
    /// on macOS, a docked taskbar on Windows. Reading it is both more accurate
    /// than a constant and the only way to notice a menu bar tall enough to mean
    /// a camera housing is present.
    fn for_monitor(m: &Monitor, allow_in_notch: bool) -> Self {
        let mut metrics = Self::for_scale(m.scale_factor());
        let scale = if m.scale_factor().is_finite() && m.scale_factor() > 0.0 {
            m.scale_factor()
        } else {
            1.0
        };

        let measured = (m.work_area().position.y - m.position().y).max(0);
        // macOS always has a menu bar, so the constant stays as a floor there in
        // case the work area ever comes back flush with the display.
        metrics.reserved_top = if cfg!(target_os = "macos") {
            measured.max((TOP_INSET_LOGICAL * scale).round() as i32)
        } else {
            measured
        };

        if allow_in_notch {
            metrics.top_inset = 0;
            let notched =
                metrics.reserved_top as f64 / scale >= NOTCHED_MENU_BAR_MIN_LOGICAL;
            if notched {
                let width = m.size().width as f64;
                let half = width * NOTCH_WIDTH_FRACTION / 2.0;
                let centre = m.position().x as f64 + width / 2.0;
                metrics.notch = Some(((centre - half).round() as i32, (centre + half).round() as i32));
            }
        } else {
            metrics.top_inset = metrics.reserved_top;
        }

        metrics
    }
}

/// Whether the bar is currently allowed into the menu bar strip.
///
/// The setting is macOS-only: on Windows the equivalent strip is a taskbar the
/// user docked there on purpose, and covering it is not a feature.
fn allow_in_notch<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    cfg!(target_os = "macos")
        && window
            .app_handle()
            .state::<AppState>()
            .config
            .get()
            .bar
            .allow_in_notch
}

fn monitor_key<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Option<(String, PhysicalPosition<i32>, PhysicalSize<u32>, Metrics)> {
    let allow = allow_in_notch(window);
    let m = window.current_monitor().ok().flatten()?;
    let name = m.name().cloned().unwrap_or_else(|| "primary".to_string());
    Some((
        name,
        *m.position(),
        *m.size(),
        Metrics::for_monitor(&m, allow),
    ))
}

/// Slide the bar out from behind the camera housing.
///
/// Only applies while the bar is up in the menu bar strip: below it the housing
/// is not in the way. Picks whichever side of the housing the bar is already
/// closer to, and gives up rather than forcing a position when the bar is too
/// wide to fit beside it at all — a bar jammed half off-screen is worse than one
/// the user can see is overlapping.
fn avoid_notch(
    x: i32,
    y: i32,
    win: PhysicalSize<u32>,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    m: Metrics,
) -> i32 {
    let Some((notch_left, notch_right)) = m.notch else {
        return x;
    };
    // Clear of the strip entirely, so the housing cannot be in the way.
    if y >= origin.y + m.reserved_top {
        return x;
    }
    let right = x + win.width as i32;
    if right <= notch_left || x >= notch_right {
        return x;
    }

    let screen_left = origin.x;
    let screen_right = origin.x + size.width as i32;
    let to_left = notch_left - win.width as i32;
    let to_right = notch_right;

    let left_fits = to_left >= screen_left;
    let right_fits = to_right + win.width as i32 <= screen_right;

    match (left_fits, right_fits) {
        (true, true) => {
            // Whichever side it was already leaning towards.
            let centre = x + win.width as i32 / 2;
            if centre < (notch_left + notch_right) / 2 {
                to_left
            } else {
                to_right
            }
        }
        (true, false) => to_left,
        (false, true) => to_right,
        (false, false) => x,
    }
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

    let (x, y) = clamp_to_monitor(x, y, win, origin, size, m.top_inset);
    let x = avoid_notch(x, y, win, origin, size, m);
    let _ = window.set_position(PhysicalPosition::new(x, y));
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
    // A save is a full serialise-and-replace of config.json. Skip it when the
    // bar has not actually moved, so a press that turned out not to be a drag
    // costs nothing.
    if state.config.get().bar.positions.get(&name) == Some(&(pos.x, pos.y)) {
        return;
    }
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
    // the display — snapping onto the menu bar is not a place anyone dragged to,
    // unless they have said it is, in which case `top_inset` is already zero.
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
    let x = avoid_notch(x, y, win, origin, size, m);
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

/// Re-run the placement rules against wherever the bar currently sits.
///
/// Called when the policy itself changes: switching the menu bar setting off has
/// to bring a bar that is up there back down, not leave it stranded over the
/// clock with no way to notice.
pub fn reapply_placement<R: Runtime>(window: &WebviewWindow<R>) {
    let Some((_, origin, size, m)) = monitor_key(window) else {
        return;
    };
    let Ok(pos) = window.outer_position() else {
        return;
    };
    let win = window.outer_size().unwrap_or(PhysicalSize::new(560, 64));

    let (x, y) = clamp_to_monitor(pos.x, pos.y, win, origin, size, m.top_inset);
    let x = avoid_notch(x, y, win, origin, size, m);
    if (x, y) != (pos.x, pos.y) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
        save_position(window);
    }
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

    /// A 14" MacBook Pro at 2x: 3024x1964 physical, housing across the middle.
    fn notched_metrics() -> Metrics {
        let mut m = Metrics::for_scale(2.0);
        m.reserved_top = 74; // 37pt menu bar at 2x
        m.top_inset = 0; // allowed into the strip
        let half = 3024.0 * NOTCH_WIDTH_FRACTION / 2.0;
        m.notch = Some(((1512.0 - half) as i32, (1512.0 + half) as i32));
        m
    }

    #[test]
    fn a_bar_in_the_menu_bar_is_pushed_off_the_camera_housing() {
        let m = notched_metrics();
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(3024u32, 1964u32);
        let win = PhysicalSize::new(560u32, 64u32);
        let (notch_left, notch_right) = m.notch.unwrap();

        // Dead centre, inside the strip: has to move clear of the housing.
        let x = avoid_notch(1232, 8, win, origin, screen, m);
        assert!(
            x + win.width as i32 <= notch_left || x >= notch_right,
            "bar still overlaps the housing at x={x}"
        );
    }

    #[test]
    fn the_bar_leaves_the_housing_by_the_nearer_side() {
        let m = notched_metrics();
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(3024u32, 1964u32);
        let win = PhysicalSize::new(560u32, 64u32);
        let (notch_left, notch_right) = m.notch.unwrap();

        // Leaning left of the housing's centre -> goes left.
        assert_eq!(
            avoid_notch(notch_left - 100, 8, win, origin, screen, m),
            notch_left - win.width as i32
        );
        // Leaning right -> goes right.
        assert_eq!(
            avoid_notch(notch_right - 60, 8, win, origin, screen, m),
            notch_right
        );
    }

    #[test]
    fn below_the_menu_bar_the_housing_is_irrelevant() {
        let m = notched_metrics();
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(3024u32, 1964u32);
        let win = PhysicalSize::new(560u32, 64u32);
        // Same x, but clear of the strip: leave it exactly where it was put.
        assert_eq!(avoid_notch(1232, 200, win, origin, screen, m), 1232);
    }

    #[test]
    fn a_display_without_a_housing_never_moves_the_bar() {
        let mut m = Metrics::for_scale(2.0);
        m.reserved_top = 48; // ordinary 24pt menu bar at 2x
        m.top_inset = 0;
        m.notch = None;
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(2560u32, 1600u32);
        let win = PhysicalSize::new(560u32, 64u32);
        assert_eq!(avoid_notch(1000, 8, win, origin, screen, m), 1000);
    }

    #[test]
    fn a_bar_too_wide_to_clear_the_housing_is_left_alone() {
        let m = notched_metrics();
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(3024u32, 1964u32);
        // Wider than either side of the housing: nowhere to put it, so don't
        // shove it half off the display.
        let win = PhysicalSize::new(2900u32, 64u32);
        assert_eq!(avoid_notch(60, 8, win, origin, screen, m), 60);
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
