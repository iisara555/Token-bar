//! Antigravity — Google's agentic IDE.
//!
//! Antigravity signs in with its own Google OAuth client and its own quota
//! pool, entirely separate from a Gemini API key (`gemini.rs`) and from the
//! Gemini CLI's own OAuth client. Google publishes no documentation for any
//! of this: the OAuth client id, the token endpoint, and the `v1internal`
//! quota calls below are reverse-engineered by several independent
//! open-source Antigravity integrations, though `loadCodeAssist` and
//! `retrieveUserQuota` are the same Cloud Code Assist endpoints Google's own
//! open-source `gemini-cli` uses. Expect this to need updates if Google
//! reshapes the internal API — the honest failure mode here is a `Parse`
//! error, not a confidently wrong percentage.
//!
//! The login flow itself lives in `crate::oauth`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::time::Duration;

use super::{
    http_error, Caps, FetchCtx, Provider, ProviderError, ProviderId, QuotaWindow, Status,
    UsageSnapshot, USER_AGENT,
};

const CLOUDCODE_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";

pub struct Antigravity;

#[async_trait]
impl Provider for Antigravity {
    fn id(&self) -> ProviderId {
        ProviderId::Antigravity
    }

    // An undocumented, reverse-engineered endpoint is not one to hammer.
    fn poll_interval(&self) -> Duration {
        Duration::from_secs(10 * 60)
    }

    fn caps(&self) -> Caps {
        Caps {
            cost: false,
            tokens: false,
            balance: false,
            series: false,
        }
    }

    fn needs_key(&self) -> bool {
        false
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
        let credential =
            crate::oauth::credential_async(ProviderId::Antigravity, &ctx.client, false).await?;
        let token = credential.access_token.expose_secret();
        let project = load_project(ctx, token).await?;
        let buckets = retrieve_quota(ctx, token, &project).await?;

        let tightest = tightest(&buckets).ok_or_else(|| {
            ProviderError::Parse("Antigravity quota response had no recognised buckets".into())
        })?;

        let mut snap = UsageSnapshot::empty(ProviderId::Antigravity, Status::Ok);
        // Google does not label these buckets with a fixed window the way
        // Anthropic's five-hour/weekly pair does — a reset under a day out
        // reads as the tighter of the two meters, everything else as the wider one.
        if tightest.minutes <= 24 * 60 {
            snap.limits.five_hour = Some(tightest.quota);
        } else {
            snap.limits.week = Some(tightest.quota);
        }
        Ok(snap)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadCodeAssistResponse {
    #[serde(default)]
    cloudaicompanion_project: Option<String>,
}

async fn load_project(ctx: &FetchCtx, token: &str) -> Result<String, ProviderError> {
    let resp = ctx
        .client
        .post(format!("{CLOUDCODE_BASE}:loadCodeAssist"))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .json(&serde_json::json!({
            "metadata": {
                "ideType": "IDE_UNSPECIFIED",
                "platform": "PLATFORM_UNSPECIFIED",
                "pluginType": "GEMINI",
            }
        }))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(http_error(resp).await);
    }
    let body: LoadCodeAssistResponse = resp.json().await?;
    body.cloudaicompanion_project
        .filter(|p| !p.is_empty())
        .ok_or_else(|| ProviderError::Parse("Antigravity did not return a Cloud project".into()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetrieveUserQuotaResponse {
    #[serde(default)]
    buckets: Vec<Bucket>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Bucket {
    #[serde(default)]
    remaining_fraction: Option<f64>,
    #[serde(default)]
    reset_time: Option<DateTime<Utc>>,
}

async fn retrieve_quota(
    ctx: &FetchCtx,
    token: &str,
    project: &str,
) -> Result<Vec<Bucket>, ProviderError> {
    let resp = ctx
        .client
        .post(format!("{CLOUDCODE_BASE}:retrieveUserQuota"))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .json(&serde_json::json!({ "project": project }))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(http_error(resp).await);
    }
    let body: RetrieveUserQuotaResponse = resp.json().await?;
    Ok(body.buckets)
}

struct Tightest {
    quota: QuotaWindow,
    minutes: i64,
}

/// The bucket with the least quota remaining — the one worth surfacing on a
/// bar that only has room for one number per provider.
fn tightest(buckets: &[Bucket]) -> Option<Tightest> {
    buckets
        .iter()
        .filter_map(|b| {
            let fraction = b.remaining_fraction?;
            let used_percent = (1.0 - fraction).clamp(0.0, 1.0) * 100.0;
            let minutes = b
                .reset_time
                .map(|reset| (reset - Utc::now()).num_minutes().max(1))
                .unwrap_or(24 * 60);
            Some((
                used_percent,
                Tightest {
                    quota: QuotaWindow::new(used_percent, b.reset_time, minutes as u64),
                    minutes,
                },
            ))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, tightest)| tightest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_bucket_with_the_least_quota_left() {
        let buckets = vec![
            Bucket {
                remaining_fraction: Some(0.8),
                reset_time: Some(Utc::now() + chrono::Duration::hours(2)),
            },
            Bucket {
                remaining_fraction: Some(0.1),
                reset_time: Some(Utc::now() + chrono::Duration::hours(3)),
            },
        ];
        let tightest = tightest(&buckets).unwrap();
        assert_eq!(tightest.quota.used_percent, 90.0);
    }

    #[test]
    fn a_bucket_missing_a_reset_time_falls_back_to_a_day() {
        let buckets = vec![Bucket {
            remaining_fraction: Some(0.5),
            reset_time: None,
        }];
        let tightest = tightest(&buckets).unwrap();
        assert_eq!(tightest.minutes, 24 * 60);
        assert!(tightest.quota.resets_at.is_none());
    }

    #[test]
    fn no_recognised_buckets_is_none_not_a_zero() {
        let buckets = vec![Bucket {
            remaining_fraction: None,
            reset_time: None,
        }];
        assert!(tightest(&buckets).is_none());
    }
}
