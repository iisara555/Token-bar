//! System tray icon, badge and popover.
//!
//! The icon is drawn in code rather than shipped as three PNGs, so the badge
//! colour can track budget state exactly: neutral under the warning threshold,
//! amber at it, red once the budget is blown.

use tauri::{
    image::Image,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, Runtime,
};

use super::{BAR, POPOVER, SETTINGS};
use crate::state::AppState;

const ICON_PX: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    Normal,
    Warning,
    Over,
}

impl Badge {
    fn rgb(self) -> (u8, u8, u8) {
        match self {
            // Muted slate: the tray is not the place to shout when all is well.
            Badge::Normal => (0x8A, 0x8F, 0x98),
            Badge::Warning => (0xE8, 0xA3, 0x3D),
            Badge::Over => (0xE5, 0x53, 0x4B),
        }
    }
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show_bar", "Show bar", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Token Bar", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &settings, &sep, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(badge_icon(Badge::Normal))
        .tooltip("Token Bar")
        .menu(&menu)
        // Left click should open the popover, not the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), position);
            }
        })
        .build(app)?;

    Ok(())
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id().as_ref() {
        "show_bar" => {
            if let Some(w) = app.get_webview_window(BAR) {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "refresh" => {
            app.state::<AppState>().nudge_all();
        }
        "settings" => {
            if let Some(w) = app.get_webview_window(SETTINGS) {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "quit" => {
            // Give the appbar a chance to release the desktop work area before
            // the process goes away.
            super::appbar::release_all();
            app.exit(0);
        }
        _ => {}
    }
}

fn toggle_popover<R: Runtime>(app: &AppHandle<R>, at: PhysicalPosition<f64>) {
    let Some(win) = app.get_webview_window(POPOVER) else {
        return;
    };

    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return;
    }

    // Anchor above the tray, nudged left so the panel does not hang off the
    // right edge of the screen.
    if let Ok(size) = win.outer_size() {
        let x = (at.x as i32 - size.width as i32 / 2).max(8);
        let y = (at.y as i32 - size.height as i32 - 12).max(8);
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    let _ = win.show();
    let _ = win.set_focus();
}

/// Repaint the tray icon and tooltip for the current totals.
pub fn update<R: Runtime>(app: &AppHandle<R>, badge: Badge, tooltip: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_icon(Some(badge_icon(badge)));
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

/// A filled rounded square in the badge colour, with a transparent margin so it
/// reads as an icon rather than a solid block at 100% scaling.
fn badge_icon(badge: Badge) -> Image<'static> {
    let (r, g, b) = badge.rgb();
    let n = ICON_PX as i32;
    let inset = 4;
    let radius = 7;
    let mut rgba = vec![0u8; (ICON_PX * ICON_PX * 4) as usize];

    for y in 0..n {
        for x in 0..n {
            let idx = ((y * n + x) * 4) as usize;
            let a = rounded_coverage(x, y, inset, n - inset, radius);
            if a == 0 {
                continue;
            }
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = a;
        }
    }
    Image::new_owned(rgba, ICON_PX, ICON_PX)
}

/// Coverage of a rounded rectangle at one pixel: 255 inside, 0 outside, with a
/// single-pixel soft edge on the corners so they do not look chewed.
fn rounded_coverage(x: i32, y: i32, lo: i32, hi: i32, radius: i32) -> u8 {
    if x < lo || y < lo || x >= hi || y >= hi {
        return 0;
    }
    // Distance into the nearest corner's quarter-circle.
    let cx = if x < lo + radius {
        lo + radius
    } else if x >= hi - radius {
        hi - radius - 1
    } else {
        return 255;
    };
    let cy = if y < lo + radius {
        lo + radius
    } else if y >= hi - radius {
        hi - radius - 1
    } else {
        return 255;
    };

    let dx = (x - cx) as f32;
    let dy = (y - cy) as f32;
    let d = (dx * dx + dy * dy).sqrt();
    let r = radius as f32;
    if d <= r - 0.5 {
        255
    } else if d >= r + 0.5 {
        0
    } else {
        (255.0 * (r + 0.5 - d)) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_buffer_is_the_right_length() {
        let img = badge_icon(Badge::Over);
        assert_eq!(img.width(), ICON_PX);
        assert_eq!(img.height(), ICON_PX);
        assert_eq!(img.rgba().len(), (ICON_PX * ICON_PX * 4) as usize);
    }

    #[test]
    fn corners_are_transparent_and_centre_is_opaque() {
        assert_eq!(rounded_coverage(0, 0, 4, 28, 7), 0);
        assert_eq!(rounded_coverage(16, 16, 4, 28, 7), 255);
    }

    #[test]
    fn badge_colours_are_distinct() {
        assert_ne!(Badge::Normal.rgb(), Badge::Warning.rgb());
        assert_ne!(Badge::Warning.rgb(), Badge::Over.rgb());
    }
}
