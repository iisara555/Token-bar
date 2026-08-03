//! OpenRouter credits.
//!
//! `/api/v1/credits` needs a provisioning/management key and reports lifetime
//! credits purchased vs used. A plain inference key only works on `/api/v1/key`,
//! which reports usage against an optional limit — so we try the richer
//! endpoint first and fall back.
//!
//! Docs: https://openrouter.ai/docs/api_reference/limits

use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::time::Duration;

use super::{
    dollars_to_cents, http_error, Caps, FetchCtx, Provider, ProviderError, ProviderId, Status,
    UsageSnapshot, USER_AGENT,
};

const CREDITS_URL: &str = "https://openrouter.ai/api/v1/credits";
const KEY_URL: &str = "https://openrouter.ai/api/v1/key";

pub struct OpenRouter;

#[async_trait]
impl Provider for OpenRouter {
    fn id(&self) -> ProviderId {
        ProviderId::Openrouter
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_secs(5 * 60)
    }

    fn caps(&self) -> Caps {
        Caps {
            cost: true,
            tokens: false,
            balance: true,
            series: false,
        }
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
        match fetch_credits(ctx).await {
            Ok(snap) => Ok(snap),
            // A non-management key is rejected on /credits but valid on /key.
            Err(ProviderError::Auth) | Err(ProviderError::Http { status: 404, .. }) => {
                fetch_key(ctx).await
            }
            Err(e) => Err(e),
        }
    }
}

fn request(ctx: &FetchCtx, url: &str) -> reqwest::RequestBuilder {
    ctx.client
        .get(url)
        .bearer_auth(ctx.key.expose_secret())
        .header(reqwest::header::USER_AGENT, USER_AGENT)
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

#[derive(Deserialize)]
struct Credits {
    #[serde(default)]
    total_credits: f64,
    #[serde(default)]
    total_usage: f64,
}

#[derive(Deserialize)]
struct KeyInfo {
    #[serde(default)]
    usage: f64,
    /// `null` means no cap on the key.
    #[serde(default)]
    limit: Option<f64>,
}

async fn fetch_credits(ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
    let resp = request(ctx, CREDITS_URL).send().await?;
    if !resp.status().is_success() {
        return Err(http_error(resp).await);
    }
    let body: Envelope<Credits> = resp.json().await?;

    let mut snap = UsageSnapshot::empty(ProviderId::Openrouter, Status::Ok);
    snap.cost_cents = Some(dollars_to_cents(body.data.total_usage));
    snap.balance_cents = Some(dollars_to_cents(
        body.data.total_credits - body.data.total_usage,
    ));
    Ok(snap)
}

async fn fetch_key(ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
    let resp = request(ctx, KEY_URL).send().await?;
    if !resp.status().is_success() {
        return Err(http_error(resp).await);
    }
    let body: Envelope<KeyInfo> = resp.json().await?;

    let mut snap = UsageSnapshot::empty(ProviderId::Openrouter, Status::Ok);
    snap.cost_cents = Some(dollars_to_cents(body.data.usage));
    snap.balance_cents = body
        .data
        .limit
        .map(|l| dollars_to_cents(l - body.data.usage));
    snap.message = Some("inference key: lifetime usage only".into());
    Ok(snap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_is_credits_minus_usage() {
        let body: Envelope<Credits> =
            serde_json::from_str(include_str!("../../fixtures/openrouter_credits.json")).unwrap();
        assert_eq!(dollars_to_cents(body.data.total_credits), 10_000);
        assert_eq!(dollars_to_cents(body.data.total_usage), 4_237);
        assert_eq!(
            dollars_to_cents(body.data.total_credits - body.data.total_usage),
            5_763
        );
    }

    #[test]
    fn uncapped_key_reports_no_balance() {
        let body: Envelope<KeyInfo> =
            serde_json::from_str(r#"{"data":{"label":"dev","usage":2.5,"limit":null}}"#).unwrap();
        assert!(body.data.limit.is_none());
        assert_eq!(dollars_to_cents(body.data.usage), 250);
    }
}
