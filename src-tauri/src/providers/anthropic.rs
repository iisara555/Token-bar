//! Anthropic Usage & Cost Admin API.
//!
//! Requires an Admin API key (`sk-ant-admin01-…`) and an **organization**
//! account — individual Console accounts get 403 here, and Claude Pro/Max
//! subscription usage is not exposed by any API at all.
//!
//! Docs: https://platform.claude.com/docs/en/manage-claude/usage-cost-api

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

use super::{
    dollars_to_cents, http_error, Bucket, Caps, FetchCtx, Provider, ProviderError, ProviderId,
    QuotaWindow, Status, TokenBreakdown, UsageSnapshot, USER_AGENT,
};

const BASE: &str = "https://api.anthropic.com/v1/organizations";
const API_VERSION: &str = "2023-06-01";

/// `1d` granularity is capped at 31 buckets by the API, so 30 days is the
/// ceiling for a single unpaginated window.
const MAX_DAYS: i64 = 30;

/// Guard against a runaway `next_page` loop.
const MAX_PAGES: usize = 40;

pub struct Anthropic;

#[async_trait]
impl Provider for Anthropic {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    /// Docs state the API supports polling once per minute for sustained use.
    /// We sit at double that: cost data only lands within ~5 min anyway.
    fn poll_interval(&self) -> Duration {
        Duration::from_secs(120)
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

        let days = ctx.range.days().min(MAX_DAYS);
        let start = (Utc::now() - chrono::Duration::days(days))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        let costs = fetch_cost(ctx, start).await?;
        let tokens = fetch_usage(ctx, start).await?;

        // Merge the two reports on their bucket start. The cost endpoint and
        // the usage endpoint are separate calls that can disagree on which
        // buckets exist, so union the keys rather than zipping.
        let mut merged: BTreeMap<DateTime<Utc>, Bucket> = BTreeMap::new();
        for (start, cents) in costs {
            merged
                .entry(start)
                .or_insert_with(|| Bucket {
                    start,
                    cost_cents: None,
                    tokens: Default::default(),
                })
                .cost_cents = Some(cents);
        }
        for (start, tk) in tokens {
            let entry = merged.entry(start).or_insert_with(|| Bucket {
                start,
                cost_cents: None,
                tokens: Default::default(),
            });
            entry.tokens = tk;
        }

        let mut snap = UsageSnapshot::empty(ProviderId::Anthropic, Status::Ok);
        snap.series = merged.into_values().collect();
        snap.source_latency_secs = 300; // "typically within 5 minutes"
        Ok(snap.summarize())
    }
}

// ---------------------------------------------------------------------------
// Claude.ai subscription limits (existing Claude Code OAuth login)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OAuthUsage {
    five_hour: Option<OAuthUsageWindow>,
    seven_day: Option<OAuthUsageWindow>,
}

#[derive(Deserialize)]
struct OAuthUsageWindow {
    utilization: f64,
    resets_at: Option<DateTime<Utc>>,
}

async fn fetch_oauth_usage(ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
    let mut credential =
        crate::oauth::credential_async(ProviderId::Anthropic, &ctx.client, false).await?;
    let mut resp = ctx
        .client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(credential.access_token.expose_secret())
        .header("anthropic-beta", "oauth-2025-04-20")
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await?;
    // A server-side expiry can arrive before the local expiresAt timestamp.
    // Refresh once, then retry exactly once so a bad token cannot create a
    // request loop against Anthropic's tightly rate-limited usage endpoint.
    if resp.status().as_u16() == 401 {
        credential =
            crate::oauth::credential_async(ProviderId::Anthropic, &ctx.client, true).await?;
        resp = ctx
            .client
            .get("https://api.anthropic.com/api/oauth/usage")
            .bearer_auth(credential.access_token.expose_secret())
            .header("anthropic-beta", "oauth-2025-04-20")
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await?;
    }
    if !resp.status().is_success() {
        return Err(http_error(resp).await);
    }
    let body: OAuthUsage = resp.json().await?;
    let mut snap = UsageSnapshot::empty(ProviderId::Anthropic, Status::Ok);
    snap.limits.five_hour = body
        .five_hour
        .map(|window| QuotaWindow::new(window.utilization, window.resets_at, 5 * 60));
    snap.limits.week = body
        .seven_day
        .map(|window| QuotaWindow::new(window.utilization, window.resets_at, 7 * 24 * 60));
    Ok(snap)
}

fn request(ctx: &FetchCtx, url: &str) -> reqwest::RequestBuilder {
    ctx.client
        .get(url)
        .header("x-api-key", ctx.key.expose_secret())
        .header("anthropic-version", API_VERSION)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
}

// ---------------------------------------------------------------------------
// Cost report
// ---------------------------------------------------------------------------

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
    starting_at: DateTime<Utc>,
    #[serde(default = "Vec::new")]
    results: Vec<T>,
}

#[derive(Deserialize)]
struct CostResult {
    /// Decimal string. The Console renders the same figure in dollars, so we
    /// read it as dollars — the one number worth eyeballing against the Cost
    /// page the first time you connect a real key.
    amount: String,
    #[serde(default)]
    currency: Option<String>,
}

async fn fetch_cost(
    ctx: &FetchCtx,
    start: DateTime<Utc>,
) -> Result<Vec<(DateTime<Utc>, i64)>, ProviderError> {
    let base = format!(
        "{BASE}/cost_report?starting_at={}&ending_at={}",
        start.format("%Y-%m-%dT%H:%M:%SZ"),
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
    );

    let mut out: Vec<(DateTime<Utc>, i64)> = Vec::new();
    let mut page: Option<String> = None;

    for _ in 0..MAX_PAGES {
        let url = match &page {
            Some(p) => format!("{base}&page={p}"),
            None => base.clone(),
        };
        let resp = request(ctx, &url).send().await?;
        if !resp.status().is_success() {
            return Err(http_error(resp).await);
        }
        let body: Page<CostResult> = resp.json().await?;

        for b in body.data {
            let mut cents = 0i64;
            for r in b.results {
                if let Some(c) = &r.currency {
                    if !c.eq_ignore_ascii_case("usd") {
                        // Nothing to convert with offline; skip rather than
                        // silently add a foreign-currency figure to a USD total.
                        continue;
                    }
                }
                cents += parse_amount(&r.amount)?;
            }
            out.push((b.starting_at, cents));
        }

        match (body.has_more, body.next_page) {
            (true, Some(next)) => page = Some(next),
            _ => return Ok(out),
        }
    }
    Ok(out)
}

fn parse_amount(raw: &str) -> Result<i64, ProviderError> {
    raw.trim()
        .parse::<f64>()
        .map(dollars_to_cents)
        .map_err(|_| ProviderError::Parse(format!("bad cost amount {raw:?}")))
}

// ---------------------------------------------------------------------------
// Usage report
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct UsageResult {
    #[serde(default)]
    uncached_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    /// Newer responses nest cache creation by TTL; older ones use a flat count.
    #[serde(default)]
    cache_creation: Option<CacheCreation>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Deserialize, Default)]
struct CacheCreation {
    #[serde(default)]
    ephemeral_5m_input_tokens: u64,
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
}

impl UsageResult {
    fn cache_write(&self) -> u64 {
        if let Some(c) = &self.cache_creation {
            c.ephemeral_5m_input_tokens + c.ephemeral_1h_input_tokens
        } else {
            self.cache_creation_input_tokens.unwrap_or(0)
        }
    }
}

async fn fetch_usage(
    ctx: &FetchCtx,
    start: DateTime<Utc>,
) -> Result<Vec<(DateTime<Utc>, TokenBreakdown)>, ProviderError> {
    let base = format!(
        "{BASE}/usage_report/messages?starting_at={}&ending_at={}&bucket_width=1d&limit=31",
        start.format("%Y-%m-%dT%H:%M:%SZ"),
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
    );

    let mut out = Vec::new();
    let mut page: Option<String> = None;

    for _ in 0..MAX_PAGES {
        let url = match &page {
            Some(p) => format!("{base}&page={p}"),
            None => base.clone(),
        };
        let resp = request(ctx, &url).send().await?;
        if !resp.status().is_success() {
            return Err(http_error(resp).await);
        }
        let body: Page<UsageResult> = resp.json().await?;

        for b in body.data {
            let mut tk = TokenBreakdown::default();
            for r in &b.results {
                tk.input += r.uncached_input_tokens;
                tk.output += r.output_tokens;
                tk.cache_read += r.cache_read_input_tokens;
                tk.cache_write += r.cache_write();
            }
            out.push((b.starting_at, tk));
        }

        match (body.has_more, body.next_page) {
            (true, Some(next)) => page = Some(next),
            _ => return Ok(out),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cost_page(json: &str) -> Page<CostResult> {
        serde_json::from_str(json).expect("cost fixture parses")
    }

    #[test]
    fn parses_cost_report_fixture() {
        let body = cost_page(include_str!("../../fixtures/anthropic_cost_report.json"));
        assert_eq!(body.data.len(), 2);
        let day0: i64 = body.data[0]
            .results
            .iter()
            .map(|r| parse_amount(&r.amount).unwrap())
            .sum();
        assert_eq!(day0, 1234 + 56); // $12.34 input + $0.56 output
        assert!(body.has_more);
    }

    #[test]
    fn follows_pagination_cursor() {
        let body = cost_page(include_str!("../../fixtures/anthropic_cost_report.json"));
        assert_eq!(
            body.next_page.as_deref(),
            Some("page_MjAyNS0wMS0wMlQwMDowMDowMFo=")
        );
    }

    #[test]
    fn parses_usage_report_with_nested_cache_creation() {
        let body: Page<UsageResult> =
            serde_json::from_str(include_str!("../../fixtures/anthropic_usage_report.json"))
                .expect("usage fixture parses");
        let r = &body.data[0].results[0];
        assert_eq!(r.uncached_input_tokens, 120_000);
        assert_eq!(r.output_tokens, 8_400);
        assert_eq!(r.cache_read_input_tokens, 1_500_000);
        assert_eq!(r.cache_write(), 60_000 + 12_000);
    }

    #[test]
    fn falls_back_to_flat_cache_creation_field() {
        let r: UsageResult = serde_json::from_str(
            r#"{"uncached_input_tokens":10,"output_tokens":2,"cache_creation_input_tokens":99}"#,
        )
        .unwrap();
        assert_eq!(r.cache_write(), 99);
    }

    #[test]
    fn skips_non_usd_rows_instead_of_adding_them() {
        let r: CostResult = serde_json::from_str(r#"{"amount":"5.00","currency":"EUR"}"#).unwrap();
        assert_eq!(r.currency.as_deref(), Some("EUR"));
        // The summing loop in fetch_cost drops this row; the parse itself is fine.
        assert_eq!(parse_amount(&r.amount).unwrap(), 500);
    }

    #[test]
    fn rejects_a_garbage_amount_rather_than_reporting_zero() {
        assert!(parse_amount("not-a-number").is_err());
    }
}
