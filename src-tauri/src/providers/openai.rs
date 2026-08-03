//! OpenAI Costs + Usage API.
//!
//! Needs an **admin key** (`sk-admin-…`) created at
//! platform.openai.com/settings/organization/admin-keys — a normal project key
//! gets 401 here. ChatGPT Plus/Pro subscription usage is not covered by any
//! API; only platform (API) spend shows up.
//!
//! Docs: https://developers.openai.com/cookbook/examples/completions_usage_api

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

use super::{
    dollars_to_cents, http_error, Bucket, Caps, FetchCtx, Provider, ProviderError, ProviderId,
    QuotaWindow, Status, TokenBreakdown, UsageSnapshot, USER_AGENT,
};

const BASE: &str = "https://api.openai.com/v1/organization";
const MAX_PAGES: usize = 40;

pub struct OpenAi;

#[async_trait]
impl Provider for OpenAi {
    fn id(&self) -> ProviderId {
        ProviderId::Openai
    }

    /// The costs endpoint only buckets by day, so a fast poll buys nothing.
    fn poll_interval(&self) -> Duration {
        Duration::from_secs(15 * 60)
    }

    fn caps(&self) -> Caps {
        Caps {
            cost: true,
            tokens: true,
            balance: false,
            series: true,
        }
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
        if ctx.use_oauth {
            return fetch_oauth_usage(ctx).await;
        }

        let start = ctx.range.start.timestamp();

        let costs = fetch_costs(ctx, start).await?;
        let usage = fetch_usage(ctx, start).await?;

        let mut merged: BTreeMap<DateTime<Utc>, Bucket> = BTreeMap::new();
        for (t, cents) in costs {
            merged
                .entry(t)
                .or_insert_with(|| Bucket {
                    start: t,
                    cost_cents: None,
                    tokens: Default::default(),
                })
                .cost_cents = Some(cents);
        }
        for (t, tk) in usage {
            merged
                .entry(t)
                .or_insert_with(|| Bucket {
                    start: t,
                    cost_cents: None,
                    tokens: Default::default(),
                })
                .tokens = tk;
        }

        let mut snap = UsageSnapshot::empty(ProviderId::Openai, Status::Ok);
        snap.series = merged.into_values().collect();
        snap.source_latency_secs = 24 * 3600; // daily buckets; today's is partial
        Ok(snap.summarize())
    }
}

// ---------------------------------------------------------------------------
// ChatGPT subscription limits (existing Codex OAuth login)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CodexUsage {
    rate_limit: Option<CodexRateLimit>,
}

#[derive(Deserialize)]
struct CodexRateLimit {
    primary_window: Option<CodexWindow>,
    secondary_window: Option<CodexWindow>,
}

#[derive(Deserialize)]
struct CodexWindow {
    used_percent: f64,
    limit_window_seconds: u64,
    reset_at: Option<i64>,
}

async fn fetch_oauth_usage(ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
    let credential = crate::oauth::credential(ProviderId::Openai)?;
    let mut request = ctx
        .client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(credential.access_token.expose_secret())
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "codex-cli");
    if let Some(account_id) = credential.account_id {
        request = request.header("ChatGPT-Account-Id", account_id);
    }

    let resp = request.send().await?;
    if !resp.status().is_success() {
        return Err(http_error(resp).await);
    }
    let body: CodexUsage = resp.json().await?;
    let mut snap = UsageSnapshot::empty(ProviderId::Openai, Status::Ok);

    if let Some(rate_limit) = body.rate_limit {
        for window in [rate_limit.primary_window, rate_limit.secondary_window]
            .into_iter()
            .flatten()
        {
            let resets_at = window
                .reset_at
                .and_then(|timestamp| Utc.timestamp_opt(timestamp, 0).single());
            let quota = QuotaWindow::new(
                window.used_percent,
                resets_at,
                window.limit_window_seconds.div_ceil(60),
            );
            if window.limit_window_seconds <= 12 * 60 * 60 {
                snap.limits.five_hour = Some(quota);
            } else if window.limit_window_seconds >= 6 * 24 * 60 * 60 {
                snap.limits.week = Some(quota);
            }
        }
    }

    Ok(snap)
}

fn request(ctx: &FetchCtx, url: &str) -> reqwest::RequestBuilder {
    ctx.client
        .get(url)
        .bearer_auth(ctx.key.expose_secret())
        .header(reqwest::header::USER_AGENT, USER_AGENT)
}

fn to_utc(unix: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(unix, 0).single().unwrap_or_else(Utc::now)
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    data: Vec<TimeBucket<T>>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    next_page: Option<String>,
}

#[derive(Deserialize)]
struct TimeBucket<T> {
    start_time: i64,
    #[serde(default = "Vec::new")]
    results: Vec<T>,
}

#[derive(Deserialize)]
struct CostResult {
    amount: Amount,
}

#[derive(Deserialize)]
struct Amount {
    value: f64,
    #[serde(default)]
    currency: Option<String>,
}

#[derive(Deserialize)]
struct UsageResult {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    /// Cached prompt tokens are *included* in `input_tokens`, so they are
    /// subtracted out below rather than added — double-counting them would
    /// inflate the headline figure on cache-heavy workloads.
    #[serde(default)]
    input_cached_tokens: u64,
}

async fn fetch_page<T: for<'de> Deserialize<'de>>(
    ctx: &FetchCtx,
    base: &str,
) -> Result<Vec<TimeBucket<T>>, ProviderError> {
    let mut out = Vec::new();
    let mut page: Option<String> = None;

    for _ in 0..MAX_PAGES {
        let url = match &page {
            Some(p) => format!("{base}&page={p}"),
            None => base.to_string(),
        };
        let resp = request(ctx, &url).send().await?;
        if !resp.status().is_success() {
            return Err(http_error(resp).await);
        }
        let body: Page<T> = resp.json().await?;
        out.extend(body.data);

        match (body.has_more, body.next_page) {
            (true, Some(next)) => page = Some(next),
            _ => break,
        }
    }
    Ok(out)
}

async fn fetch_costs(
    ctx: &FetchCtx,
    start: i64,
) -> Result<Vec<(DateTime<Utc>, i64)>, ProviderError> {
    let base = format!("{BASE}/costs?start_time={start}&bucket_width=1d&limit=31");
    let buckets: Vec<TimeBucket<CostResult>> = fetch_page(ctx, &base).await?;

    Ok(buckets
        .into_iter()
        .map(|b| {
            let cents = b
                .results
                .iter()
                .filter(|r| {
                    r.amount
                        .currency
                        .as_deref()
                        .map_or(true, |c| c.eq_ignore_ascii_case("usd"))
                })
                .map(|r| dollars_to_cents(r.amount.value))
                .sum();
            (to_utc(b.start_time), cents)
        })
        .collect())
}

async fn fetch_usage(
    ctx: &FetchCtx,
    start: i64,
) -> Result<Vec<(DateTime<Utc>, TokenBreakdown)>, ProviderError> {
    let base = format!("{BASE}/usage/completions?start_time={start}&bucket_width=1d&limit=31");
    let buckets: Vec<TimeBucket<UsageResult>> = fetch_page(ctx, &base).await?;

    Ok(buckets
        .into_iter()
        .map(|b| {
            let mut tk = TokenBreakdown::default();
            for r in &b.results {
                tk.input += r.input_tokens.saturating_sub(r.input_cached_tokens);
                tk.output += r.output_tokens;
                tk.cache_read += r.input_cached_tokens;
            }
            (to_utc(b.start_time), tk)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_costs_fixture() {
        let body: Page<CostResult> =
            serde_json::from_str(include_str!("../../fixtures/openai_costs.json")).unwrap();
        assert_eq!(body.data.len(), 2);
        let cents: i64 = body.data[0]
            .results
            .iter()
            .map(|r| dollars_to_cents(r.amount.value))
            .sum();
        assert_eq!(cents, 642); // $5.20 + $1.22
        assert!(!body.has_more);
    }

    #[test]
    fn cached_input_is_not_double_counted() {
        let body: Page<UsageResult> =
            serde_json::from_str(include_str!("../../fixtures/openai_usage.json")).unwrap();
        let r = &body.data[0].results[0];
        // Fixture: input_tokens 500_000 of which 400_000 cached.
        assert_eq!(
            r.input_tokens.saturating_sub(r.input_cached_tokens),
            100_000
        );
        assert_eq!(r.input_cached_tokens, 400_000);
    }

    #[test]
    fn treats_missing_currency_as_usd() {
        let r: CostResult = serde_json::from_str(r#"{"amount":{"value":1.5}}"#).unwrap();
        assert!(r.amount.currency.is_none());
        assert_eq!(dollars_to_cents(r.amount.value), 150);
    }
}
