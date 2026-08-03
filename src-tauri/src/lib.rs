//! Token Bar — always-on-top AI usage toolbar.

pub mod commands;
pub mod config;
pub mod oauth;
pub mod providers;
pub mod scheduler;
pub mod secrets;
pub mod state;
pub mod store;
pub mod vibrancy;
pub mod windows;

use tauri::{AppHandle, Manager, Runtime, WindowEvent};

use crate::config::ThemeMode;
use crate::providers::Status;
use crate::state::AppState;
use crate::vibrancy::GlassMode;
use crate::windows::{floating, tray, BAR, POPOVER, SETTINGS};

/// What the native backdrop resolved to on this machine.
pub fn current_glass_mode<R: Runtime>(app: &AppHandle<R>) -> GlassMode {
    app.state::<AppState>()
        .glass_mode
        .lock()
        .map(|g| *g)
        .unwrap_or(GlassMode::Css)
}

/// Re-run the backdrop decision across every window. Called when the theme or
/// the glass preference changes.
pub fn reapply_glass<R: Runtime>(app: &AppHandle<R>) {
    let (pref, theme) = {
        let cfg = app.state::<AppState>().config.get();
        (cfg.glass, cfg.theme)
    };

    // `System` follows whatever the OS reports for the bar window.
    let dark = match theme {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => app
            .get_webview_window(BAR)
            .and_then(|w| w.theme().ok())
            .map(|t| t == tauri::Theme::Dark)
            .unwrap_or(true),
    };

    let mut resolved = GlassMode::Solid;
    for label in [BAR, POPOVER, SETTINGS] {
        if let Some(w) = app.get_webview_window(label) {
            // The bar is the window whose result we report: it is the one the
            // user actually stares at.
            let mode = if label == BAR {
                vibrancy::apply_bar(&w, pref)
            } else {
                vibrancy::apply(&w, pref, dark)
            };
            if label == BAR {
                resolved = mode;
            }
        }
    }

    if let Ok(mut slot) = app.state::<AppState>().glass_mode.lock() {
        *slot = resolved;
    }
    use tauri::Emitter;
    let _ = app.emit("glass://changed", resolved);
}

/// Recompute the tray badge and tooltip from whatever is currently cached.
pub fn refresh_tray<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let cfg = state.config.get();
    let Ok(snapshots) = state.store.all_latest() else {
        return;
    };

    let mut total_cents = 0i64;
    let mut worst = tray::Badge::Normal;
    let mut any = false;

    for snap in &snapshots {
        if !cfg.provider(snap.provider).enabled {
            continue;
        }
        if let Some(c) = snap.cost_cents {
            total_cents += c;
            any = true;
        }
        if let (Some(spent), Some(budget)) = (snap.cost_cents, cfg.budget_cents(snap.provider)) {
            if budget > 0 {
                let ratio = spent as f64 / budget as f64;
                if ratio >= 1.0 {
                    worst = tray::Badge::Over;
                } else if ratio >= cfg.warn_at && worst != tray::Badge::Over {
                    worst = tray::Badge::Warning;
                }
            }
        }
    }

    let stale = snapshots
        .iter()
        .filter(|s| cfg.provider(s.provider).enabled)
        .any(|s| matches!(s.status, Status::Stale | Status::Error));

    let tooltip = if any {
        format!(
            "Token Bar — ${:.2} over {} days{}",
            total_cents as f64 / 100.0,
            cfg.window_days,
            if stale { " (some data stale)" } else { "" }
        )
    } else {
        "Token Bar — no data yet".to_string()
    };

    tray::update(app, worst, &tooltip);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Second launch just brings the existing bar forward.
            if let Some(w) = app.get_webview_window(BAR) {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();

            // Token Bar is a menu bar utility, not an application with windows
            // a user manages. `Accessory` is what drops it out of the Dock and
            // out of Cmd-Tab, and stops it taking over the menu bar whenever one
            // of its panels happens to get focus — the difference between a
            // status item and an app that merely has one.
            #[cfg(target_os = "macos")]
            let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let dir = handle
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            std::fs::create_dir_all(&dir).ok();

            let cfg_store = config::ConfigStore::load(&dir.join("config.json"));
            let cache = store::Store::open(&dir.join("cache.db"))
                .map_err(|e| format!("cache database: {e}"))?;
            let cfg = cfg_store.get();
            app.manage(AppState::new(cfg_store, cache));

            reapply_glass(&handle);

            if let Some(bar) = handle.get_webview_window(BAR) {
                // An always-on-top bar that disappears when you switch Spaces is
                // an always-on-top bar for one Space. macOS ties window
                // visibility to Spaces by default; Windows has no equivalent
                // concept and ignores this.
                let _ = bar.set_visible_on_all_workspaces(true);
                floating::restore_position(&bar);
                floating::set_click_through(&bar, cfg.bar.click_through);
                if cfg.bar.visible {
                    let _ = bar.show();
                }
            }

            tray::build(&handle)?;
            if let Err(e) = register_hotkey(&handle, &cfg.hotkey, None) {
                // A clash with another app is not fatal — everything else still
                // works, and the tray can still show the bar.
                log::warn!("could not register hotkey {:?}: {e}", cfg.hotkey);
            }
            refresh_tray(&handle);

            // Keep the tray in step with incoming data.
            {
                use tauri::Listener;
                let h = handle.clone();
                handle.listen(scheduler::EVENT_UPDATED, move |_| refresh_tray(&h));
            }

            scheduler::spawn_all(handle.clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing a secondary window should hide it, not tear down the app.
            WindowEvent::CloseRequested { api, .. } if window.label() != BAR => {
                api.prevent_close();
                let _ = window.hide();
            }
            // The popover is a transient panel: dismiss it when it loses focus.
            WindowEvent::Focused(false) if window.label() == POPOVER => {
                let _ = window.hide();
            }
            WindowEvent::Destroyed if window.label() == BAR => {
                windows::appbar::release_all();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_view,
            commands::get_snapshots,
            commands::refresh,
            commands::set_theme,
            commands::set_glass,
            commands::set_window_days,
            commands::set_warn_at,
            commands::set_hotkey,
            commands::set_provider_enabled,
            commands::set_provider_auth_mode,
            commands::set_provider_option,
            commands::save_provider_key,
            commands::clear_provider_key,
            commands::bar_set_size,
            commands::bar_dropped,
            commands::set_click_through,
            commands::set_compact,
            commands::set_allow_in_notch,
            commands::open_settings,
            commands::open_provider_link,
            commands::hide_window,
            commands::quit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Token Bar");
}

/// Bind the show/hide accelerator, replacing whatever was bound before.
///
/// Returns the error rather than only logging it, so the settings command can
/// refuse to persist an accelerator the OS would not accept and leave the
/// working one in place.
pub fn register_hotkey<R: Runtime>(
    app: &AppHandle<R>,
    accelerator: &str,
    previous: Option<&str>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    if let Some(previous) = previous {
        let _ = app.global_shortcut().unregister(previous);
    }

    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(accelerator, move |_, _, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if let Some(w) = handle.get_webview_window(BAR) {
                if w.is_visible().unwrap_or(false) {
                    let _ = w.hide();
                } else {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .map_err(|e| format!("{e}"))
}
