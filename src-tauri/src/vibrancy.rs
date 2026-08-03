//! Native window backdrop.
//!
//! Two layers make the glass: this one asks the compositor for a real backdrop
//! — Mica/Acrylic from DWM on Windows, an `NSVisualEffectView` from AppKit on
//! macOS — and the CSS in `tokens.css` paints the highlight ring, tint and noise
//! on top. Either can stand alone: if the compositor refuses, the CSS layer
//! still renders, and if the user has switched transparency effects off we
//! deliberately skip both and go opaque rather than fighting their preference.

use serde::Serialize;
use tauri::{Runtime, WebviewWindow};

use crate::config::GlassPref;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GlassMode {
    /// DWM is painting a real backdrop behind the window.
    Native,
    /// AppKit is painting an `NSVisualEffectView` behind the window.
    ///
    /// Kept distinct from `Native` because the two need different amounts of
    /// help from CSS: a macOS material arrives already tinted and already
    /// adapting to what is behind it, so anything CSS adds on top is additive
    /// where on Windows it is the whole tint.
    Vibrancy,
    /// No native backdrop; CSS blur only. Still translucent.
    Css,
    /// Fully opaque. Either the user asked for it, or the OS has transparency
    /// effects switched off.
    Solid,
}

/// Apply the best available backdrop and report what was actually achieved, so
/// the frontend can pick a matching token set instead of guessing.
pub fn apply<R: Runtime>(window: &WebviewWindow<R>, pref: GlassPref, dark: bool) -> GlassMode {
    let wanted = match pref {
        GlassPref::Off => return GlassMode::Solid,
        GlassPref::On => true,
        // Respect the OS accessibility/battery setting.
        GlassPref::Auto => transparency_enabled(),
    };

    if !wanted {
        return GlassMode::Solid;
    }

    // No native backdrop on Windows, for any window.
    //
    // DWM composites Mica and Acrylic *underneath* the webview, filling the
    // window's whole content rect. It has no idea the web content draws a
    // 28px-rounded card, so the material stays a hard rectangle behind it and
    // shows through at all four corners as a black square. CSS cannot reach
    // that layer — `clip-path` on the document only ever clips what the webview
    // itself paints — and DWM's own corner rounding is a fixed ~8px that would
    // still leave the difference visible.
    //
    // Nothing is really lost: `--glass-fill` is `rgba(0, 0, 0, 0.94)`, so the
    // material was contributing about six percent of a black surface. The bar
    // has always taken this path (see `apply_bar`); this makes the two secondary
    // windows agree with it instead of trading a rounded corner for a tint
    // nobody can see.
    #[cfg(target_os = "windows")]
    {
        let _ = dark;
        let _ = window_vibrancy::clear_mica(window);
        let _ = window_vibrancy::clear_acrylic(window);
        let _ = window_vibrancy::clear_blur(window);
    }

    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::NSVisualEffectMaterial;

        // `Popover` and `HudWindow` are the two materials AppKit intends for
        // exactly this — small utility panels floating over arbitrary content.
        // `Popover` is the lighter of the two and is what the system's own
        // menu-bar extras use, so it is what a Mac user's eye expects here.
        //
        // The radius has to match `--radius-card`, or AppKit rounds the
        // material to a different curve than CSS rounds the surface and leaves
        // a bright crescent in each corner.
        let material = if dark {
            NSVisualEffectMaterial::HudWindow
        } else {
            NSVisualEffectMaterial::Popover
        };
        if window_vibrancy::apply_vibrancy(window, material, None, Some(16.0)).is_ok() {
            return GlassMode::Vibrancy;
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = (window, dark);

    GlassMode::Css
}

/// Keep the toolbar's OS window genuinely transparent.
///
/// A native backdrop paints the whole rectangular window behind the rounded CSS
/// pill, which leaves square corners — true of Mica on Windows and of an
/// `NSVisualEffectView` on macOS alike, since both fill the window's content
/// rect rather than whatever shape the web content happens to draw.
pub fn apply_bar<R: Runtime>(window: &WebviewWindow<R>, pref: GlassPref) -> GlassMode {
    #[cfg(target_os = "windows")]
    {
        let _ = window_vibrancy::clear_mica(window);
        let _ = window_vibrancy::clear_acrylic(window);
        let _ = window_vibrancy::clear_blur(window);
    }

    #[cfg(target_os = "macos")]
    {
        let _ = window_vibrancy::clear_vibrancy(window);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = window;

    match pref {
        GlassPref::Off => GlassMode::Solid,
        GlassPref::On => GlassMode::Css,
        GlassPref::Auto if transparency_enabled() => GlassMode::Css,
        GlassPref::Auto => GlassMode::Solid,
    }
}

/// Settings → Personalization → Colors → Transparency effects.
///
/// Absent value means "on": that is the Windows default, and a fresh profile
/// has no such registry value at all.
#[cfg(target_os = "windows")]
pub fn transparency_enabled() -> bool {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    unsafe {
        let mut data: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut std::ffi::c_void),
            Some(&mut size),
        );
        if status.is_ok() {
            data != 0
        } else {
            true
        }
    }
}

/// System Settings → Accessibility → Display → Reduce transparency.
///
/// AppKit already honours this inside `NSVisualEffectView` — the material goes
/// opaque on its own — but the answer still has to come back here, because the
/// frontend picks a token set from the reported [`GlassMode`] and would
/// otherwise keep tinting for a translucent surface that is no longer
/// translucent, ending up with text on a muddied opaque panel.
///
/// Read through `defaults` rather than the Objective-C runtime: it is one
/// process, only on a glass re-evaluation (startup and settings changes), and
/// it avoids taking an `objc` dependency for a single boolean.
///
/// Absent value means "off", i.e. transparency stays on — that is the macOS
/// default and a fresh account has no such key at all.
#[cfg(target_os = "macos")]
pub fn transparency_enabled() -> bool {
    let output = std::process::Command::new("defaults")
        .args(["read", "com.apple.universalaccess", "reduceTransparency"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let value = String::from_utf8_lossy(&out.stdout);
            // Reduced transparency on ("1") means glass off.
            value.trim() != "1"
        }
        // No key, no `defaults`, or a sandbox that blocked the read. Defaulting
        // to "transparency is on" matches the OS default and is the recoverable
        // direction: the CSS `prefers-reduced-transparency` query is still
        // there as a second line of defence if this guessed wrong.
        _ => true,
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn transparency_enabled() -> bool {
    true
}
