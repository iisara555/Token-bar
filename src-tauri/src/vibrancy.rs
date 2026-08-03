//! Native window backdrop.
//!
//! Two layers make the glass: this one asks DWM for a real Mica/Acrylic
//! backdrop, and the CSS in `tokens.css` paints the highlight ring, tint and
//! noise on top. Either can stand alone — if DWM refuses, the CSS layer still
//! renders, and if the user has switched transparency effects off in Windows we
//! deliberately skip both and go opaque rather than fighting their preference.

use serde::Serialize;
use tauri::{Runtime, WebviewWindow};

use crate::config::GlassPref;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GlassMode {
    /// DWM is painting a real backdrop behind the window.
    Native,
    /// No native backdrop; CSS blur only. Still translucent.
    Css,
    /// Fully opaque. Either the user asked for it, or Windows transparency
    /// effects are off.
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

    // Mica first: on Windows 11 it keeps rounded corners and the window shadow
    // behaving. Acrylic is the Windows 10 fallback.
    #[cfg(target_os = "windows")]
    {
        if window_vibrancy::apply_mica(window, Some(dark)).is_ok() {
            return GlassMode::Native;
        }
        if window_vibrancy::apply_acrylic(window, None).is_ok() {
            return GlassMode::Native;
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = (window, dark);

    GlassMode::Css
}

/// Keep the toolbar's OS window genuinely transparent. Mica paints the whole
/// rectangular HWND behind the rounded CSS pill, which leaves square corners.
pub fn apply_bar<R: Runtime>(window: &WebviewWindow<R>, pref: GlassPref) -> GlassMode {
    #[cfg(target_os = "windows")]
    {
        let _ = window_vibrancy::clear_mica(window);
        let _ = window_vibrancy::clear_acrylic(window);
        let _ = window_vibrancy::clear_blur(window);
    }

    #[cfg(not(target_os = "windows"))]
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

#[cfg(not(target_os = "windows"))]
pub fn transparency_enabled() -> bool {
    true
}
