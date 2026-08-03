//! Shared application state, owned by Tauri and reachable from commands,
//! the scheduler and the tray.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

use crate::config::ConfigStore;
use crate::providers::{ProviderId, USER_AGENT};
use crate::store::Store;
use crate::vibrancy::GlassMode;

pub struct AppState {
    pub config: ConfigStore,
    pub store: Store,
    pub http: reqwest::Client,
    /// What the native backdrop actually resolved to. The frontend reads this
    /// to pick a token set rather than assuming translucency it may not have.
    pub glass_mode: Mutex<GlassMode>,
    /// One wake-up handle per provider. Firing it makes the scheduler abandon
    /// its current sleep and re-read config — that is how "Refresh now" and
    /// "key saved" take effect without restarting the app.
    pub wake: BTreeMap<ProviderId, Arc<Notify>>,
}

impl AppState {
    pub fn new(config: ConfigStore, store: Store) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            // Provider APIs are plain HTTPS JSON; no redirects are expected and
            // following one could leak the Authorization header to another host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("http client");

        let wake = ProviderId::ALL
            .into_iter()
            .map(|id| (id, Arc::new(Notify::new())))
            .collect();

        Self {
            config,
            store,
            http,
            wake,
            glass_mode: Mutex::new(GlassMode::Css),
        }
    }

    pub fn nudge(&self, id: ProviderId) {
        if let Some(n) = self.wake.get(&id) {
            n.notify_one();
        }
    }

    pub fn nudge_all(&self) {
        for n in self.wake.values() {
            n.notify_one();
        }
    }
}
