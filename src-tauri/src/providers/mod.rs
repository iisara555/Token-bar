//! Provider abstraction.
//!
//! Every adapter is a thin fetch + parse. All normalization lives here so the
//! UI never has to branch on which provider a number came from.

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

pub mod anthropic;
pub mod deepseek;
pub mod gemini;
pub mod groq;
pub mod kimi;
pub mod manual;
pub mod minimax;
pub mod mistral;
pub mod openai;
pub mod openrouter;
pub mod xai;
pub mod zai;

/// User-Agent sent on every provider request. Anthropic explicitly asks
/// integrations to identify themselves so they can understand usage patterns.
pub const USER_AGENT: &str = concat!(
    "TokenBar/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/tokenbar/token-bar)"
);

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Anthropic,
    Openai,
    Kimi,
    Zai,
    Minimax,
    Openrouter,
    Xai,
    Deepseek,
    Gemini,
    Groq,
    Mistral,
}

impl ProviderId {
    pub const ALL: [ProviderId; 11] = [
        ProviderId::Anthropic,
        ProviderId::Openai,
        ProviderId::Kimi,
        ProviderId::Zai,
        ProviderId::Minimax,
        ProviderId::Openrouter,
        ProviderId::Xai,
        ProviderId::Deepseek,
        ProviderId::Gemini,
        ProviderId::Groq,
        ProviderId::Mistral,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Anthropic => "anthropic",
            ProviderId::Openai => "openai",
            ProviderId::Kimi => "kimi",
            ProviderId::Zai => "zai",
            ProviderId::Minimax => "minimax",
            ProviderId::Openrouter => "openrouter",
            ProviderId::Xai => "xai",
            ProviderId::Deepseek => "deepseek",
            ProviderId::Gemini => "gemini",
            ProviderId::Groq => "groq",
            ProviderId::Mistral => "mistral",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            ProviderId::Anthropic => "Anthropic",
            ProviderId::Openai => "OpenAI",
            ProviderId::Kimi => "Kimi",
            ProviderId::Zai => "Z.AI",
            ProviderId::Minimax => "MiniMax",
            ProviderId::Openrouter => "OpenRouter",
            ProviderId::Xai => "xAI",
            ProviderId::Deepseek => "DeepSeek",
            ProviderId::Gemini => "Gemini",
            ProviderId::Groq => "Groq",
            ProviderId::Mistral => "Mistral",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        ProviderId::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Fresh data straight from the provider.
    Ok,
    /// Served from cache because the last fetch failed or the upstream source
    /// is inherently delayed (Gemini billing export).
    Stale,
    /// Key rejected. The scheduler stops polling until the key changes.
    AuthError,
    /// Backing off after a 429.
    RateLimited,
    /// Provider publishes no usage API; only a manual budget is available.
    Unsupported,
    /// Enabled but no credential stored yet.
    NotConfigured,
    /// Network or parse failure.
    Error,
}

impl Status {
    /// Whether the scheduler should keep polling on this status.
    pub fn is_pollable(self) -> bool {
        !matches!(
            self,
            Status::AuthError | Status::Unsupported | Status::NotConfigured
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBreakdown {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl TokenBreakdown {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    pub fn add(&mut self, other: &TokenBreakdown) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }
}

/// One day of history. Drives the sparkline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub start: DateTime<Utc>,
    pub cost_cents: Option<i64>,
    pub tokens: TokenBreakdown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caps {
    pub cost: bool,
    pub tokens: bool,
    pub balance: bool,
    pub series: bool,
}

/// One provider-enforced subscription window. Percentages are kept in both
/// directions so every UI surface can say "remaining" without reinterpreting
/// the provider's response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub resets_at: Option<DateTime<Utc>>,
    pub window_minutes: u64,
}

impl QuotaWindow {
    pub fn new(used_percent: f64, resets_at: Option<DateTime<Utc>>, window_minutes: u64) -> Self {
        let used_percent = used_percent.clamp(0.0, 100.0);
        Self {
            used_percent,
            remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
            resets_at,
            window_minutes,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimits {
    pub five_hour: Option<QuotaWindow>,
    pub week: Option<QuotaWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub provider: ProviderId,
    pub status: Status,
    /// Period-to-date spend in USD cents. `None` when the provider reports only
    /// a prepaid balance (DeepSeek) or nothing at all (Groq, Mistral).
    pub cost_cents: Option<i64>,
    pub tokens: TokenBreakdown,
    /// Remaining prepaid credit, USD cents. Prepaid providers only.
    pub balance_cents: Option<i64>,
    /// Subscription quotas exposed by OAuth-backed Claude Code / Codex
    /// accounts. API-key billing adapters leave both windows empty.
    #[serde(default)]
    pub limits: UsageLimits,
    /// Daily history, oldest first.
    pub series: Vec<Bucket>,
    pub fetched_at: DateTime<Utc>,
    /// How far behind real time this provider's data inherently runs. Drives
    /// the staleness badge — Gemini billing export can be a day out.
    pub source_latency_secs: u64,
    /// Present when `status` is not `Ok`. Always redacted.
    pub message: Option<String>,
}

impl UsageSnapshot {
    pub fn empty(provider: ProviderId, status: Status) -> Self {
        Self {
            provider,
            status,
            cost_cents: None,
            tokens: TokenBreakdown::default(),
            balance_cents: None,
            limits: UsageLimits::default(),
            series: Vec::new(),
            fetched_at: Utc::now(),
            source_latency_secs: 0,
            message: None,
        }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(redact(&msg.into()));
        self
    }

    /// Roll the daily series up into the headline numbers. Adapters build the
    /// series; this keeps the totals consistent across all of them.
    pub fn summarize(mut self) -> Self {
        if !self.series.is_empty() {
            let mut tokens = TokenBreakdown::default();
            let mut cost: Option<i64> = None;
            for b in &self.series {
                tokens.add(&b.tokens);
                if let Some(c) = b.cost_cents {
                    cost = Some(cost.unwrap_or(0) + c);
                }
            }
            if tokens.total() > 0 {
                self.tokens = tokens;
            }
            if cost.is_some() {
                self.cost_cents = cost;
            }
        }
        self
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("credentials rejected")]
    Auth,
    #[error("OAuth sign-in required: {0}")]
    Oauth(String),
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    #[error("provider returned HTTP {status}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("could not parse response: {0}")]
    Parse(String),
    #[error("provider publishes no usage API")]
    Unsupported,
    #[error("missing configuration: {0}")]
    Config(String),
}

impl ProviderError {
    pub fn status(&self) -> Status {
        match self {
            ProviderError::Auth | ProviderError::Oauth(_) => Status::AuthError,
            ProviderError::RateLimited { .. } => Status::RateLimited,
            ProviderError::Unsupported => Status::Unsupported,
            ProviderError::Config(_) => Status::NotConfigured,
            _ => Status::Error,
        }
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_decode() {
            ProviderError::Parse(redact(&e.to_string()))
        } else {
            ProviderError::Network(redact(&e.to_string()))
        }
    }
}

/// Defence in depth. We never deliberately format a key into an error, but a
/// provider echoing one back in a 400 body would otherwise reach the UI and the
/// log file. Anything that looks like a key gets masked to its last 4 chars.
pub fn redact(input: &str) -> String {
    const PREFIXES: [&str; 6] = ["sk-", "xai-", "gsk_", "or-v1-", "AIza", "ya29."];
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    'outer: while !rest.is_empty() {
        for p in PREFIXES {
            if rest.starts_with(p) {
                let end = rest
                    .find(|c: char| {
                        !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
                    })
                    .unwrap_or(rest.len());
                let token = &rest[..end];
                // Keep enough to identify which key, never enough to use it.
                if token.len() > 8 {
                    out.push_str(p);
                    out.push('…');
                    out.push_str(&token[token.len() - 4..]);
                } else {
                    out.push_str("…");
                }
                rest = &rest[end..];
                continue 'outer;
            }
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

// ---------------------------------------------------------------------------
// Fetch context
// ---------------------------------------------------------------------------

/// The window of history to request. Providers cap this differently — Anthropic
/// allows at most 31 daily buckets — so adapters clamp as needed.
#[derive(Debug, Clone, Copy)]
pub struct DateRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl DateRange {
    pub fn last_days(n: i64) -> Self {
        let end = Utc::now();
        Self {
            start: (end - ChronoDuration::days(n))
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            end,
        }
    }

    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days().max(1)
    }
}

pub struct FetchCtx {
    pub client: reqwest::Client,
    pub key: SecretString,
    /// True when the adapter should read the existing official CLI OAuth
    /// credential on this tick instead of using `key`.
    pub use_oauth: bool,
    /// Non-secret per-provider settings: xAI `team_id`, Gemini `project_id`,
    /// manual budgets. Never holds credentials.
    pub options: BTreeMap<String, String>,
    pub range: DateRange,
}

impl FetchCtx {
    pub fn option(&self, name: &str) -> Result<&str, ProviderError> {
        self.options
            .get(name)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::Config(name.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// How often the scheduler may poll. Chosen per provider from documented
    /// limits and data freshness, never a uniform interval:
    /// hammering a daily-bucket endpoint every 30s buys nothing.
    fn poll_interval(&self) -> Duration;

    fn caps(&self) -> Caps;

    /// Extra non-secret settings this provider needs before it can run.
    fn required_options(&self) -> &'static [&'static str] {
        &[]
    }

    /// Whether this provider needs a stored credential at all.
    fn needs_key(&self) -> bool {
        true
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError>;
}

pub fn registry() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(anthropic::Anthropic),
        Box::new(openai::OpenAi),
        Box::new(kimi::Kimi),
        Box::new(zai::Zai),
        Box::new(minimax::MiniMax),
        Box::new(openrouter::OpenRouter),
        Box::new(xai::XAi),
        Box::new(deepseek::DeepSeek),
        Box::new(gemini::Gemini),
        Box::new(groq::Groq),
        Box::new(mistral::Mistral),
    ]
}

pub fn provider_for(id: ProviderId) -> Box<dyn Provider> {
    match id {
        ProviderId::Anthropic => Box::new(anthropic::Anthropic),
        ProviderId::Openai => Box::new(openai::OpenAi),
        ProviderId::Kimi => Box::new(kimi::Kimi),
        ProviderId::Zai => Box::new(zai::Zai),
        ProviderId::Minimax => Box::new(minimax::MiniMax),
        ProviderId::Openrouter => Box::new(openrouter::OpenRouter),
        ProviderId::Xai => Box::new(xai::XAi),
        ProviderId::Deepseek => Box::new(deepseek::DeepSeek),
        ProviderId::Gemini => Box::new(gemini::Gemini),
        ProviderId::Groq => Box::new(groq::Groq),
        ProviderId::Mistral => Box::new(mistral::Mistral),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Convert a USD *dollar* amount to cents without float drift at the boundary.
pub fn dollars_to_cents(dollars: f64) -> i64 {
    (dollars * 100.0).round() as i64
}

/// Map a non-success HTTP status onto a `ProviderError`. Bodies are truncated
/// and redacted before they can reach the UI.
pub async fn http_error(resp: reqwest::Response) -> ProviderError {
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs);
    let body = resp.text().await.unwrap_or_default();
    let body = redact(body.chars().take(400).collect::<String>().trim());

    match status.as_u16() {
        401 | 403 => ProviderError::Auth,
        429 => ProviderError::RateLimited { retry_after },
        s => ProviderError::Http { status: s, body },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_keys_but_keeps_the_sentence_readable() {
        let msg = "invalid key sk-ant-admin01-AbCdEfGhIjKlMnOp4f2a for workspace";
        let out = redact(msg);
        assert!(!out.contains("AbCdEfGhIjKl"), "raw key survived: {out}");
        assert!(out.contains("sk-…"), "no fingerprint left: {out}");
        assert!(out.ends_with("for workspace"));
    }

    #[test]
    fn redacts_every_known_prefix() {
        for k in [
            "sk-proj-0123456789abcdef",
            "xai-0123456789abcdef",
            "gsk_0123456789abcdef",
            "AIzaSy0123456789abcdef",
        ] {
            let out = redact(&format!("bad token {k}!"));
            assert!(!out.contains("0123456789"), "{k} survived: {out}");
        }
    }

    #[test]
    fn summarize_rolls_series_into_totals() {
        let mut snap = UsageSnapshot::empty(ProviderId::Anthropic, Status::Ok);
        snap.series = vec![
            Bucket {
                start: Utc::now(),
                cost_cents: Some(125),
                tokens: TokenBreakdown {
                    input: 10,
                    output: 5,
                    ..Default::default()
                },
            },
            Bucket {
                start: Utc::now(),
                cost_cents: Some(75),
                tokens: TokenBreakdown {
                    input: 20,
                    output: 1,
                    ..Default::default()
                },
            },
        ];
        let snap = snap.summarize();
        assert_eq!(snap.cost_cents, Some(200));
        assert_eq!(snap.tokens.input, 30);
        assert_eq!(snap.tokens.output, 6);
    }

    #[test]
    fn money_conversion_is_exact_at_the_cent() {
        assert_eq!(dollars_to_cents(0.07), 7);
        assert_eq!(dollars_to_cents(12.345), 1235);
        assert_eq!(dollars_to_cents(1234.56), 123456);
    }

    #[test]
    fn quota_keeps_used_and_remaining_percentages_consistent() {
        let normal = QuotaWindow::new(37.5, None, 300);
        assert_eq!(normal.used_percent, 37.5);
        assert_eq!(normal.remaining_percent, 62.5);
        assert_eq!(normal.window_minutes, 300);

        let over = QuotaWindow::new(140.0, None, 10_080);
        assert_eq!(over.used_percent, 100.0);
        assert_eq!(over.remaining_percent, 0.0);
    }
}
