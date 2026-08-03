//! The always-on-top pill.
//!
//! Position is stored **per monitor name**, not as a bare x/y. A single saved
//! coordinate is wrong the moment a laptop is undocked: the bar reappears in
//! dead space on a display that no longer exists.

use tauri::{Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

use crate::state::AppState;

/// Distance from a screen edge at which the bar clings to it.
const SNAP_PX: i32 = 24;
/// Breathing room kept between the bar and the edge it snapped to.
const MARGIN: i32 = 16;

fn monitor_key<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Option<(String, PhysicalPosition<i32>, PhysicalSize<u32>)> {
    let m = window.current_monitor().ok().flatten()?;
    let name = m.name().cloned().unwrap_or_else(|| "primary".to_string());
    Some((name, *m.position(), *m.size()))
}

/// Put the bar back where the user left it on this monitor, or top-centre on
/// first run.
pub fn restore_position<R: Runtime>(window: &WebviewWindow<R>) {
    let Some((name, origin, size)) = monitor_key(window) else {
        return;
    };

    let saved = {
        let state = window.app_handle().state::<AppState>();
        state.config.get().bar.positions.get(&name).copied()
    };

    let win = window.outer_size().unwrap_or(PhysicalSize::new(560, 64));

    let (x, y) = match saved {
        Some((x, y)) => (x, y),
        None => (
            origin.x + (size.width as i32 - win.width as i32) / 2,
            origin.y + MARGIN,
        ),
    };

    let clamped = clamp_to_monitor(x, y, win, origin, size);
    let _ = window.set_position(PhysicalPosition::new(clamped.0, clamped.1));
}

/// Persist where the bar now sits, keyed by the monitor it sits on.
pub fn save_position<R: Runtime>(window: &WebviewWindow<R>) {
    let Some((name, _, _)) = monitor_key(window) else {
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
    let Some((_, origin, size)) = monitor_key(window) else {
        return;
    };
    let Ok(pos) = window.outer_position() else {
        return;
    };
    let win = window.outer_size().unwrap_or(PhysicalSize::new(560, 64));

    let left = origin.x;
    let top = origin.y;
    let right = origin.x + size.width as i32;
    let bottom = origin.y + size.height as i32;

    let mut x = pos.x;
    let mut y = pos.y;

    if (x - left).abs() <= SNAP_PX {
        x = left + MARGIN;
    } else if (right - (x + win.width as i32)).abs() <= SNAP_PX {
        x = right - win.width as i32 - MARGIN;
    }

    if (y - top).abs() <= SNAP_PX {
        y = top + MARGIN;
    } else if (bottom - (y + win.height as i32)).abs() <= SNAP_PX {
        y = bottom - win.height as i32 - MARGIN;
    }

    let (x, y) = clamp_to_monitor(x, y, win, origin, size);
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
) -> (i32, i32) {
    let max_x = origin.x + size.width as i32 - win.width as i32;
    let max_y = origin.y + size.height as i32 - win.height as i32;
    (
        x.clamp(origin.x, max_x.max(origin.x)),
        y.clamp(origin.y, max_y.max(origin.y)),
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

        assert_eq!(clamp_to_monitor(-500, -500, win, origin, screen), (0, 0));
        assert_eq!(
            clamp_to_monitor(9999, 9999, win, origin, screen),
            (1920 - 560, 1080 - 64)
        );
        assert_eq!(clamp_to_monitor(100, 100, win, origin, screen), (100, 100));
    }

    #[test]
    fn clamp_respects_a_secondary_monitor_offset() {
        // A display sitting to the left of the primary has a negative origin.
        let origin = PhysicalPosition::new(-1920, 0);
        let screen = PhysicalSize::new(1920u32, 1080u32);
        let win = PhysicalSize::new(560u32, 64u32);
        assert_eq!(clamp_to_monitor(-5000, 0, win, origin, screen), (-1920, 0));
    }

    #[test]
    fn clamp_survives_a_window_wider_than_the_screen() {
        let origin = PhysicalPosition::new(0, 0);
        let screen = PhysicalSize::new(400u32, 300u32);
        let win = PhysicalSize::new(560u32, 64u32);
        // max_x would be negative; the bar pins to the left edge rather than
        // panicking on an inverted clamp range.
        assert_eq!(clamp_to_monitor(50, 10, win, origin, screen), (0, 10));
    }
}
