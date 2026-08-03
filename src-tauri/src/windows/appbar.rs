//! Taskbar-docked strip (phase 2 — not yet wired to any UI).
//!
//! Docking uses the Win32 appbar protocol: `ABM_NEW` to register, `ABM_QUERYPOS`
//! then `ABM_SETPOS` to reserve an edge, and handling of `ABN_POSCHANGED`,
//! `ABN_FULLSCREENAPP`, `WM_DPICHANGED` and `WM_DISPLAYCHANGE` to stay put.
//!
//! The hazard that makes this the last thing to ship: an appbar that is never
//! unregistered permanently shrinks the user's desktop work area. Every other
//! window on the machine maximises into the smaller rectangle, and nothing short
//! of a reboot or another app calling `ABM_REMOVE` with the same handle puts it
//! back. So registration must be owned by a guard that releases on drop, on
//! panic, and on `WM_ENDSESSION` — which is exactly why it is not being bolted
//! on in the same pass as the rest.
//!
//! [`release_all`] is already called from the tray's Quit handler so the exit
//! path is correct before any registration code lands.

/// Release every appbar registration this process holds.
///
/// A no-op until docking is implemented — wired up now so the quit path does
/// not have to change later, which is the change most likely to be forgotten.
pub fn release_all() {}
