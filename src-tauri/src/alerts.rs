//! Threshold alerts.
//!
//! The tray badge already carries budget state, but it carries it passively: a
//! user has to look at it to learn anything, and the whole point of a budget
//! threshold is that it matters at the moment it is crossed rather than the
//! next time someone glances at the menu bar. This is the part that speaks.
//!
//! Alerts are **edge-triggered**. A provider that is over budget is over budget
//! on every poll for the rest of the window, and notifying each time would turn
//! the one message worth reading into noise the user silences — after which the
//! feature is worse than not having it. A notification fires only when a
//! provider's level goes *up*, and the level has to fall back down before that
//! provider can raise the same alarm again.

use serde::Serialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::providers::{ProviderId, UsageSnapshot};
use crate::state::AppState;

/// How close to a ceiling a provider is. Ordered, because the whole mechanism
/// is "did this go up".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize)]
pub enum Level {
    #[default]
    Normal,
    Warning,
    Over,
}

/// Recompute every enabled provider's level and notify on the ones that rose.
///
/// Runs on the same event the tray listens to, so an alert and the badge that
/// backs it can never disagree about what the data said.
pub fn check<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let cfg = state.config.get();
    let Ok(snapshots) = state.store.all_latest() else {
        return;
    };

    let mut raised: Vec<(ProviderId, Level, String)> = Vec::new();
    {
        let Ok(mut seen) = state.alerts.lock() else {
            return;
        };
        for snap in &snapshots {
            if !cfg.provider(snap.provider).enabled {
                continue;
            }
            let Some((level, detail)) = assess(snap, cfg.budget_cents(snap.provider), cfg.warn_at)
            else {
                // Nothing measurable. Clear any past level so that a provider
                // which stops reporting does not keep an old alarm latched.
                seen.remove(&snap.provider);
                continue;
            };

            let previous = seen.get(&snap.provider).copied().unwrap_or_default();
            seen.insert(snap.provider, level);
            if level > previous {
                raised.push((snap.provider, level, detail));
            }
        }
    }

    // The lock is released before any notification goes out: the notification
    // API is the platform's, and holding a mutex across it would put an OS call
    // on the path of every other reader of this map.
    for (id, level, detail) in raised {
        let title = match level {
            Level::Over => format!("{} — limit reached", id.display_name()),
            Level::Warning => format!("{} — running low", id.display_name()),
            Level::Normal => continue,
        };
        if let Err(e) = app
            .notification()
            .builder()
            .title(title)
            .body(detail)
            .show()
        {
            log::warn!("could not show {id} alert: {e}");
        }
    }
}

/// What this snapshot says about how close the provider is to a ceiling.
///
/// Returns `None` when there is no ceiling to be close to. That is the common
/// case — a pay-as-you-go account with no budget set has spend but nothing to
/// measure it against — and inventing a threshold for it would mean alerting on
/// an amount the user never called a limit.
fn assess(
    snap: &UsageSnapshot,
    budget_cents: Option<i64>,
    warn_at: f64,
) -> Option<(Level, String)> {
    // A rate-limit window is the provider's own ceiling and outranks a budget:
    // it is the one that stops work when it runs out.
    let windows = [
        (snap.limits.five_hour.as_ref(), "5-hour limit"),
        (snap.limits.week.as_ref(), "weekly limit"),
    ];
    let mut worst: Option<(Level, String)> = None;
    for (window, label) in windows {
        let Some(window) = window else { continue };
        let remaining = window.remaining_percent;
        let level = if remaining <= 0.0 {
            Level::Over
        } else if remaining <= (1.0 - warn_at) * 100.0 {
            Level::Warning
        } else {
            Level::Normal
        };
        let detail = if level == Level::Over {
            format!("The {label} is used up.")
        } else {
            format!("{}% of the {label} is left.", remaining.round())
        };
        // `is_none_or` would say this in one line, but it is stable only from
        // 1.82 and this crate builds back to the 1.77 in Cargo.toml.
        let beats_current = match &worst {
            Some((seen, _)) => level > *seen,
            None => true,
        };
        if beats_current {
            worst = Some((level, detail));
        }
    }
    if let Some(found) = worst {
        return Some(found);
    }

    // Otherwise a budget the user set here, if they set one.
    let (spent, budget) = (snap.cost_cents?, budget_cents?);
    if budget <= 0 {
        return None;
    }
    let ratio = spent as f64 / budget as f64;
    let level = if ratio >= 1.0 {
        Level::Over
    } else if ratio >= warn_at {
        Level::Warning
    } else {
        Level::Normal
    };
    let money = |c: i64| format!("${:.2}", c as f64 / 100.0);
    Some((
        level,
        format!("{} of the {} budget spent.", money(spent), money(budget)),
    ))
}

/// The per-provider levels last seen. Lives in [`AppState`].
pub type Seen = BTreeMap<ProviderId, Level>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{QuotaWindow, Status, UsageLimits};

    fn snap() -> UsageSnapshot {
        UsageSnapshot::empty(ProviderId::Anthropic, Status::Ok)
    }

    #[test]
    fn nothing_to_measure_against_is_not_an_alert() {
        // Spend with no budget is the ordinary pay-as-you-go case.
        let mut s = snap();
        s.cost_cents = Some(50_000);
        assert!(assess(&s, None, 0.8).is_none());
        assert!(assess(&snap(), Some(1000), 0.8).is_none(), "no spend yet");
    }

    #[test]
    fn a_budget_crosses_at_the_configured_fraction() {
        let mut s = snap();
        s.cost_cents = Some(790);
        assert_eq!(assess(&s, Some(1000), 0.8).unwrap().0, Level::Normal);
        s.cost_cents = Some(800);
        assert_eq!(assess(&s, Some(1000), 0.8).unwrap().0, Level::Warning);
        s.cost_cents = Some(1000);
        assert_eq!(assess(&s, Some(1000), 0.8).unwrap().0, Level::Over);
    }

    #[test]
    fn a_zero_budget_is_not_permanently_over() {
        let mut s = snap();
        s.cost_cents = Some(1);
        assert!(assess(&s, Some(0), 0.8).is_none());
    }

    /// A rate-limit window is the provider's own ceiling: it stops work when it
    /// runs out, where a budget is a number the user chose to watch.
    #[test]
    fn a_quota_window_outranks_a_budget() {
        let mut s = snap();
        s.cost_cents = Some(0);
        s.limits = UsageLimits {
            five_hour: Some(QuotaWindow::new(100.0, None, 300)),
            week: None,
        };
        let (level, detail) = assess(&s, Some(100_000), 0.8).unwrap();
        assert_eq!(level, Level::Over);
        assert!(detail.contains("5-hour"), "{detail}");
    }

    #[test]
    fn the_worse_of_the_two_windows_is_the_one_reported() {
        let mut s = snap();
        s.limits = UsageLimits {
            five_hour: Some(QuotaWindow::new(10.0, None, 300)),
            week: Some(QuotaWindow::new(100.0, None, 10_080)),
        };
        let (level, detail) = assess(&s, None, 0.8).unwrap();
        assert_eq!(level, Level::Over);
        assert!(detail.contains("weekly"), "{detail}");
    }

    #[test]
    fn warn_at_moves_the_window_threshold_with_it() {
        let mut s = snap();
        s.limits = UsageLimits {
            five_hour: Some(QuotaWindow::new(85.0, None, 300)),
            week: None,
        };
        // 15% left: under a 0.8 threshold that is a warning, under 0.9 it is not.
        assert_eq!(assess(&s, None, 0.8).unwrap().0, Level::Warning);
        assert_eq!(assess(&s, None, 0.9).unwrap().0, Level::Normal);
    }

    #[test]
    fn levels_order_so_a_rise_is_detectable() {
        assert!(Level::Over > Level::Warning);
        assert!(Level::Warning > Level::Normal);
        assert_eq!(Level::default(), Level::Normal);
    }
}
