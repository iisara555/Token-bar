//! Polling loop: one task per provider.
//!
//! Each provider gets its own cadence taken from its documented limits, its own
//! backoff, and its own wake-up handle. They never share a timer — a rate-limited
//! Anthropic must not delay a cheap DeepSeek balance read.

use chrono::Utc;
use rand::Rng;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::providers::{
    provider_for, DateRange, FetchCtx, ProviderError, ProviderId, Status, UsageSnapshot,
};
use crate::state::AppState;

/// Event the UI listens on. Payload is a single [`UsageSnapshot`].
pub const EVENT_UPDATED: &str = "usage://updated";

/// Never back off past this, or a provider that had one bad hour stays dark all day.
const MAX_BACKOFF: Duration = Duration::from_secs(30 * 60);
/// How long to idle when a provider is disabled or has no key. Cheap re-check.
const IDLE: Duration = Duration::from_secs(60);

pub fn spawn_all(app: AppHandle) {
    for id in ProviderId::ALL {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { run(app, id).await });
    }
}

async fn run(app: AppHandle, id: ProviderId) {
    let provider = provider_for(id);
    let mut backoff = Backoff::new(provider.poll_interval());

    loop {
        // Read everything needed for this tick up front, so no Tauri state
        // handle is held across the network await.
        let (cfg, http) = {
            let state = app.state::<AppState>();
            (state.config.get(), state.http.clone())
        };
        let pcfg = cfg.provider(id);

        if !pcfg.enabled {
            sleep_or_wake(&app, id, IDLE).await;
            continue;
        }

        let use_oauth = crate::oauth::should_use(id, pcfg.auth_mode);

        // Credential lookup happens here, per tick, so saving a key in settings
        // takes effect on the next nudge without any restart. OAuth adapters
        // read the official CLI credential themselves and never copy it here.
        let key = if provider.needs_key() && !use_oauth {
            match crate::secrets::get(id) {
                Ok(Some(k)) => k,
                Ok(None) => {
                    publish(&app, UsageSnapshot::empty(id, Status::NotConfigured));
                    wait_for_wake(&app, id).await;
                    continue;
                }
                Err(e) => {
                    publish(
                        &app,
                        UsageSnapshot::empty(id, Status::Error).with_message(e.to_string()),
                    );
                    sleep_or_wake(&app, id, IDLE).await;
                    continue;
                }
            }
        } else {
            secrecy::SecretString::from(String::new())
        };

        let ctx = FetchCtx {
            client: http,
            key,
            use_oauth,
            options: pcfg.options.clone(),
            range: DateRange::last_days(cfg.window_days),
        };

        let result = provider.fetch(&ctx).await;
        let state = app.state::<AppState>();

        let snapshot = match result {
            Ok(mut snap) => {
                backoff.reset();
                // Providers that report only a balance get their spend derived
                // from how far that balance has fallen.
                if snap.cost_cents.is_none() && snap.balance_cents.is_some() {
                    let since = Utc::now() - chrono::Duration::days(cfg.window_days);
                    if let Ok(Some(derived)) = state.store.derived_spend_cents(id, since) {
                        snap.cost_cents = Some(derived);
                    }
                }
                snap
            }
            Err(err) => {
                let status = err.status();
                let msg = err.to_string();
                log::warn!("{id} fetch failed: {msg}");

                if let ProviderError::RateLimited {
                    retry_after: Some(d),
                } = &err
                {
                    backoff.honour(*d);
                } else {
                    backoff.bump();
                }

                match status {
                    // A rejected key will stay rejected. Stop burning requests
                    // and wait until settings change.
                    Status::AuthError | Status::NotConfigured => {
                        let snap = UsageSnapshot::empty(id, status).with_message(msg);
                        state.store.put(&snap).ok();
                        drop(state);
                        publish(&app, snap);
                        wait_for_wake(&app, id).await;
                        continue;
                    }
                    _ => state
                        .store
                        .stale_fallback(id, msg.clone())
                        .unwrap_or_else(|_| UsageSnapshot::empty(id, status).with_message(msg)),
                }
            }
        };

        state.store.put(&snapshot).ok();
        drop(state);
        publish(&app, snapshot);

        let wait = backoff.next();
        sleep_or_wake(&app, id, wait).await;
    }
}

fn publish(app: &AppHandle, snap: UsageSnapshot) {
    if let Err(e) = app.emit(EVENT_UPDATED, &snap) {
        log::warn!("could not emit update: {e}");
    }
}

/// Sleep, unless someone asks for a refresh first.
async fn sleep_or_wake(app: &AppHandle, id: ProviderId, dur: Duration) {
    let notify = {
        let state = app.state::<AppState>();
        state.wake.get(&id).cloned()
    };
    match notify {
        Some(n) => {
            tokio::select! {
                _ = tokio::time::sleep(dur) => {}
                _ = n.notified() => {}
            }
        }
        None => tokio::time::sleep(dur).await,
    }
}

async fn wait_for_wake(app: &AppHandle, id: ProviderId) {
    let notify = {
        let state = app.state::<AppState>();
        state.wake.get(&id).cloned()
    };
    match notify {
        Some(n) => n.notified().await,
        None => tokio::time::sleep(IDLE).await,
    }
}

// ---------------------------------------------------------------------------
// Backoff
// ---------------------------------------------------------------------------

/// Exponential backoff with jitter, anchored to the provider's normal cadence.
///
/// Jitter matters even with a single client: without it, every provider that
/// failed during the same network blip would retry in lockstep forever.
#[derive(Debug)]
pub struct Backoff {
    base: Duration,
    failures: u32,
    forced: Option<Duration>,
}

impl Backoff {
    pub fn new(base: Duration) -> Self {
        Self {
            base,
            failures: 0,
            forced: None,
        }
    }

    pub fn reset(&mut self) {
        self.failures = 0;
        self.forced = None;
    }

    pub fn bump(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }

    /// A server-supplied `Retry-After` wins over our own curve.
    pub fn honour(&mut self, d: Duration) {
        self.failures = self.failures.saturating_add(1);
        self.forced = Some(d);
    }

    pub fn next(&mut self) -> Duration {
        let raw = match self.forced.take() {
            Some(d) => d,
            None if self.failures == 0 => self.base,
            None => {
                let factor = 2u32.saturating_pow(self.failures.min(10));
                self.base.saturating_mul(factor)
            }
        };
        let capped = raw.min(MAX_BACKOFF).max(Duration::from_secs(5));
        jitter(capped)
    }
}

/// ±20% so retries spread out instead of stacking.
fn jitter(d: Duration) -> Duration {
    let secs = d.as_secs_f64();
    let factor = rand::thread_rng().gen_range(0.8..1.2);
    Duration::from_secs_f64(secs * factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_interval_is_the_providers_own_cadence() {
        let mut b = Backoff::new(Duration::from_secs(120));
        let d = b.next();
        assert!(
            d >= Duration::from_secs(96) && d <= Duration::from_secs(144),
            "{d:?}"
        );
    }

    #[test]
    fn failures_grow_the_interval() {
        let mut b = Backoff::new(Duration::from_secs(60));
        b.bump();
        b.bump();
        let d = b.next();
        // 60 * 2^2 = 240s, ±20%
        assert!(d >= Duration::from_secs(192), "{d:?}");
    }

    #[test]
    fn backoff_is_capped() {
        let mut b = Backoff::new(Duration::from_secs(300));
        for _ in 0..20 {
            b.bump();
        }
        assert!(b.next() <= Duration::from_secs_f64(MAX_BACKOFF.as_secs_f64() * 1.2));
    }

    #[test]
    fn success_clears_the_penalty() {
        let mut b = Backoff::new(Duration::from_secs(60));
        for _ in 0..5 {
            b.bump();
        }
        b.reset();
        assert!(b.next() <= Duration::from_secs(72));
    }

    #[test]
    fn retry_after_overrides_the_curve() {
        let mut b = Backoff::new(Duration::from_secs(60));
        b.honour(Duration::from_secs(600));
        let d = b.next();
        assert!(
            d >= Duration::from_secs(480) && d <= Duration::from_secs(720),
            "{d:?}"
        );
    }

    #[test]
    fn never_sleeps_for_less_than_five_seconds() {
        let mut b = Backoff::new(Duration::from_millis(1));
        assert!(b.next() >= Duration::from_secs(4));
    }
}
