//! System tray icon, badge and popover.
//!
//! The icon is drawn in code rather than shipped as three PNGs, so the badge
//! colour can track budget state exactly: neutral under the warning threshold,
//! amber at it, red once the budget is blown.
//!
//! macOS is the exception, and deliberately. A menu bar extra is a *template
//! image*: AppKit throws away its colour and re-tints the alpha channel to suit
//! the menu bar it is sitting in — black on a light bar, white on a dark one,
//! and inverted again while the menu is pulled down. A coloured icon opts out of
//! all of that and is the single most conspicuous way to look like a foreign
//! app in the one place every Mac user glances at constantly. So on macOS the
//! badge changes *shape* instead of colour: a ring at rest, a ring with a wedge
//! missing at warning, a filled disc once the budget is blown — readable in
//! monochrome, and readable to anyone who cannot separate amber from red.

use tauri::{
    image::Image,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime,
};

use super::{BAR, POPOVER, SETTINGS};
use crate::state::AppState;

/// Windows tray icons are supplied at 32px and downsampled by the shell.
#[cfg(not(target_os = "macos"))]
const ICON_PX: u32 = 32;

/// macOS menu bar extras are 18pt tall; 36px is that at 2x, which is what every
/// Mac since 2012 actually renders. Below this the ring's stroke lands on a
/// half-pixel and the icon reads as a smudge.
#[cfg(target_os = "macos")]
const ICON_PX: u32 = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    Normal,
    Warning,
    Over,
}

impl Badge {
    /// Only the colour-icon platforms need this; macOS badges by shape.
    #[cfg(not(target_os = "macos"))]
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

    let builder = TrayIconBuilder::with_id("main")
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
        });

    // Hand AppKit the alpha channel and let it pick the colour. Without this the
    // icon stays whatever colour it was drawn and turns invisible against a dark
    // menu bar.
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);

    builder.build(app)?;

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

    if let Ok(size) = win.outer_size() {
        let _ = win.set_position(anchor_to_tray(app, at, size));
    }
    let _ = win.show();
    let _ = win.set_focus();
}

/// Where a panel opened from the tray should sit.
///
/// The old rule was "always above the icon", which is right for a Windows
/// taskbar at the bottom and wrong everywhere else — on macOS the menu bar is at
/// the top of the screen, so subtracting the panel's height put the whole thing
/// off the top of the display, and a Windows taskbar docked to the top does the
/// same. Deciding from where the icon actually is covers every arrangement of
/// both without asking which OS this is.
fn anchor_to_tray<R: Runtime>(
    app: &AppHandle<R>,
    at: PhysicalPosition<f64>,
    size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    const GAP: i32 = 12;
    const EDGE: i32 = 8;

    let click_x = at.x as i32;
    let click_y = at.y as i32;

    // Screen bounds, so the panel can be kept on-screen. Without a monitor we
    // still produce a sane position, just without the clamp.
    let bounds = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| (*m.position(), *m.size()));

    let mut x = click_x - size.width as i32 / 2;
    // Above the icon by default; below it when the icon is in the top strip of
    // the screen, which is where a menu bar or a top-docked taskbar lives.
    let mut y = click_y - size.height as i32 - GAP;

    if let Some((origin, screen)) = bounds {
        let top_strip = origin.y + (screen.height as i32) / 5;
        if click_y <= top_strip {
            y = click_y + GAP;
        }

        let min_x = origin.x + EDGE;
        let max_x = origin.x + screen.width as i32 - size.width as i32 - EDGE;
        x = x.clamp(min_x, max_x.max(min_x));

        let min_y = origin.y + EDGE;
        let max_y = origin.y + screen.height as i32 - size.height as i32 - EDGE;
        y = y.clamp(min_y, max_y.max(min_y));
    } else {
        x = x.max(EDGE);
        y = y.max(EDGE);
    }

    PhysicalPosition::new(x, y)
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
#[cfg(not(target_os = "macos"))]
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

/// A template gauge: the badge state read as fullness rather than as colour.
///
/// Empty ring → ring with a centre dot → solid disc. Every pixel is black; only
/// the alpha channel carries the shape, which is what makes it a template image
/// AppKit is willing to re-tint. Drawing it in colour here would have no effect
/// beyond wasting the bytes — `icon_as_template(true)` discards RGB entirely.
#[cfg(target_os = "macos")]
fn badge_icon(badge: Badge) -> Image<'static> {
    let n = ICON_PX as f32;
    let centre = n / 2.0;
    let outer = n * 0.40;
    let inner = outer - n * 0.11;
    let dot = n * 0.17;

    let mut rgba = vec![0u8; (ICON_PX * ICON_PX * 4) as usize];

    for y in 0..ICON_PX {
        for x in 0..ICON_PX {
            // Pixel centres, so the shape is symmetric about the icon's middle
            // rather than half a pixel off it.
            let dx = x as f32 + 0.5 - centre;
            let dy = y as f32 + 0.5 - centre;
            let d = (dx * dx + dy * dy).sqrt();

            let alpha = match badge {
                Badge::Normal => edge(outer - d).min(edge(d - inner)),
                Badge::Warning => edge(outer - d).min(edge(d - inner)).max(edge(dot - d)),
                Badge::Over => edge(outer - d),
            };
            if alpha == 0 {
                continue;
            }
            // RGB stays zero: alpha is the entire payload of a template image.
            rgba[((y * ICON_PX + x) * 4 + 3) as usize] = alpha;
        }
    }

    Image::new_owned(rgba, ICON_PX, ICON_PX)
}

/// One-pixel antialiased step over a signed distance: fully inside at +0.5,
/// fully outside at -0.5, linear across the pixel in between.
#[cfg(target_os = "macos")]
fn edge(signed_distance: f32) -> u8 {
    ((signed_distance + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Coverage of a rounded rectangle at one pixel: 255 inside, 0 outside, with a
/// single-pixel soft edge on the corners so they do not look chewed.
#[cfg(not(target_os = "macos"))]
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

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn corners_are_transparent_and_centre_is_opaque() {
        assert_eq!(rounded_coverage(0, 0, 4, 28, 7), 0);
        assert_eq!(rounded_coverage(16, 16, 4, 28, 7), 255);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn badge_colours_are_distinct() {
        assert_ne!(Badge::Normal.rgb(), Badge::Warning.rgb());
        assert_ne!(Badge::Warning.rgb(), Badge::Over.rgb());
    }

    /// The whole point of the macOS icon is that it survives having its colour
    /// thrown away, so the three states have to differ in the alpha channel
    /// alone. Comparing rendered buffers is the only check that actually proves
    /// that — a colour comparison would pass on an icon that is invisible in
    /// the menu bar.
    #[cfg(target_os = "macos")]
    #[test]
    fn badge_shapes_are_distinct_in_alpha_alone() {
        let alpha = |b: Badge| badge_icon(b).rgba().chunks(4).map(|p| p[3]).collect::<Vec<_>>();
        let (normal, warning, over) = (alpha(Badge::Normal), alpha(Badge::Warning), alpha(Badge::Over));

        assert_ne!(normal, warning);
        assert_ne!(warning, over);
        assert_ne!(normal, over);
    }

    /// A template image whose RGB is anything but zero is a sign the colour
    /// path leaked back in; AppKit would discard it, so it can only mislead.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_icon_carries_no_colour() {
        let img = badge_icon(Badge::Warning);
        assert!(img.rgba().chunks(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    }

    /// The centre is inside the ring's hole at rest and inside the disc when
    /// over budget — the reading the whole shape scheme rests on.
    #[cfg(target_os = "macos")]
    #[test]
    fn ring_is_hollow_and_over_is_filled() {
        let centre = |b: Badge| {
            let mid = ICON_PX / 2;
            badge_icon(b).rgba()[((mid * ICON_PX + mid) * 4 + 3) as usize]
        };
        assert_eq!(centre(Badge::Normal), 0);
        assert_eq!(centre(Badge::Over), 255);
        assert_eq!(centre(Badge::Warning), 255);
    }
}
