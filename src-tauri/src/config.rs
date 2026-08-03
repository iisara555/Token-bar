//! Non-secret settings, persisted as JSON next to the app data dir.
//!
//! Nothing in here is a credential. Keys live in the Windows Credential
//! Manager (see [`crate::secrets`]); this file only ever holds identifiers,
//! budgets and window geometry, so it is safe to read, back up or sync.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::providers::ProviderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// Prefer an existing Claude Code / Codex / Kimi Code login, then fall back to an API key.
    #[default]
    Auto,
    Oauth,
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GlassPref {
    /// Follow the OS: native backdrop when transparency effects are on.
    #[default]
    Auto,
    /// Force the translucent look even if Windows says otherwise.
    On,
    /// Opaque surfaces. Also what `Auto` resolves to when the user has turned
    /// transparency effects off in Windows.
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub enabled: bool,
    /// Anthropic, OpenAI and Kimi can read the user's existing official client
    /// login. Other providers ignore this and continue to use their API key.
    #[serde(default)]
    pub auth_mode: AuthMode,
    /// Non-secret per-provider settings: `team_id`, `budget_usd`,
    /// `manual_spend_usd`. Never credentials.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auth_mode: AuthMode::default(),
            options: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BarConfig {
    /// Saved position keyed by monitor name. A bare x/y breaks the moment a
    /// second display is unplugged, so geometry is always per-monitor.
    #[serde(default)]
    pub positions: BTreeMap<String, (i32, i32)>,
    #[serde(default)]
    pub click_through: bool,
    #[serde(default)]
    pub compact: bool,
    #[serde(default = "yes")]
    pub visible: bool,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            positions: BTreeMap::new(),
            click_through: false,
            compact: false,
            visible: true,
        }
    }
}

fn yes() -> bool {
    true
}

fn default_hotkey() -> String {
    "CmdOrControl+Alt+U".into()
}

fn default_window_days() -> i64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub glass: GlassPref,
    #[serde(default)]
    pub providers: BTreeMap<ProviderId, ProviderConfig>,
    #[serde(default)]
    pub bar: BarConfig,
    #[serde(default = "default_window_days")]
    pub window_days: i64,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    /// Warn at this fraction of budget. 0.8 = amber at 80%.
    #[serde(default = "default_warn")]
    pub warn_at: f64,
}

fn default_warn() -> f64 {
    0.8
}

impl Default for Config {
    fn default() -> Self {
        let mut providers = BTreeMap::new();
        // Start with the two adapters that report real dollars enabled, so a
        // first run has something to show as soon as a key is pasted in.
        for id in [ProviderId::Anthropic, ProviderId::Openai] {
            providers.insert(
                id,
                ProviderConfig {
                    enabled: true,
                    ..Default::default()
                },
            );
        }
        Self {
            glass: GlassPref::default(),
            providers,
            bar: BarConfig::default(),
            window_days: default_window_days(),
            hotkey: default_hotkey(),
            warn_at: default_warn(),
        }
    }
}

impl Config {
    pub fn provider(&self, id: ProviderId) -> ProviderConfig {
        self.providers.get(&id).cloned().unwrap_or_default()
    }

    pub fn enabled_providers(&self) -> Vec<ProviderId> {
        ProviderId::ALL
            .into_iter()
            .filter(|id| self.provider(*id).enabled)
            .collect()
    }

    pub fn budget_cents(&self, id: ProviderId) -> Option<i64> {
        self.provider(id)
            .options
            .get("budget_usd")
            .and_then(|v| v.trim().parse::<f64>().ok())
            .map(crate::providers::dollars_to_cents)
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub struct ConfigStore {
    path: PathBuf,
    inner: Mutex<Config>,
}

impl ConfigStore {
    /// Load from disk, falling back to defaults. A corrupt file is moved aside
    /// rather than deleted — losing someone's budgets to a stray byte would be
    /// worse than leaving a .bak around.
    pub fn load(path: &Path) -> Self {
        let cfg = match std::fs::read_to_string(path) {
            Ok(raw) => match serde_json::from_str::<Config>(&raw) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("config.json unreadable ({e}); starting fresh");
                    let _ = std::fs::rename(path, path.with_extension("json.bak"));
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        };
        Self {
            path: path.to_path_buf(),
            inner: Mutex::new(cfg),
        }
    }

    pub fn get(&self) -> Config {
        self.inner.lock().expect("config mutex").clone()
    }

    /// Mutate and persist in one step so the file can never drift from memory.
    pub fn update<F: FnOnce(&mut Config)>(&self, f: F) -> std::io::Result<Config> {
        let snapshot = {
            let mut guard = self.inner.lock().expect("config mutex");
            f(&mut guard);
            guard.clone()
        };
        self.write(&snapshot)?;
        Ok(snapshot)
    }

    fn write(&self, cfg: &Config) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(cfg)?;
        // Write-then-rename: a crash mid-write leaves the old config intact.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_the_two_dollar_reporting_providers() {
        let cfg = Config::default();
        assert_eq!(
            cfg.enabled_providers(),
            vec![ProviderId::Anthropic, ProviderId::Openai]
        );
    }

    #[test]
    fn budget_parses_from_options() {
        let mut cfg = Config::default();
        cfg.providers.insert(
            ProviderId::Groq,
            ProviderConfig {
                enabled: true,
                options: [("budget_usd".to_string(), "25.5".to_string())]
                    .into_iter()
                    .collect(),
                ..ProviderConfig::default()
            },
        );
        assert_eq!(cfg.budget_cents(ProviderId::Groq), Some(2550));
        assert_eq!(cfg.budget_cents(ProviderId::Anthropic), None);
    }

    #[test]
    fn unknown_fields_and_missing_fields_both_round_trip() {
        // Both directions of compatibility in one case: `theme` is a field this
        // build no longer has, written by every build before the app went
        // black-only, and `futureSetting` stands in for one a later build might
        // add. Neither may stop a config from loading — a user who downgrades
        // should not lose their budgets to a stray key.
        let raw = r#"{"theme":"dark","futureSetting":42}"#;
        let cfg: Config = serde_json::from_str(raw).expect("loads");
        assert_eq!(cfg.hotkey, "CmdOrControl+Alt+U");
        assert_eq!(cfg.window_days, 30);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join("token-bar-cfg-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.json");
        let _ = std::fs::remove_file(&path);

        let store = ConfigStore::load(&path);
        store.update(|c| c.window_days = 7).unwrap();

        let reloaded = ConfigStore::load(&path);
        assert_eq!(reloaded.get().window_days, 7);
        let _ = std::fs::remove_file(&path);
    }
}
