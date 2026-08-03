//! MiniMax Token Plan usage.
//!
//! MiniMax documents a read-only token-plan endpoint for API keys. The exact
//! response has gained fields over time, so the parser looks for the stable
//! used/remaining + limit pairs and ignores unrelated account metadata.

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use secrecy::ExposeSecret;
use serde_json::Value;
use std::time::Duration;

use super::{
    http_error, Caps, FetchCtx, Provider, ProviderError, ProviderId, QuotaWindow, Status,
    UsageSnapshot, USER_AGENT,
};

const URL: &str = "https://www.minimaxi.com/v1/token_plan/remains";

pub struct MiniMax;

#[async_trait]
impl Provider for MiniMax {
    fn id(&self) -> ProviderId {
        ProviderId::Minimax
    }
    fn poll_interval(&self) -> Duration {
        Duration::from_secs(5 * 60)
    }
    fn caps(&self) -> Caps {
        Caps {
            cost: false,
            tokens: false,
            balance: false,
            series: false,
        }
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
        let resp = ctx
            .client
            .get(URL)
            .bearer_auth(ctx.key.expose_secret())
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(http_error(resp).await);
        }
        let body: Value = resp.json().await?;
        let windows = parse_windows(&body)?;
        let mut snap = UsageSnapshot::empty(ProviderId::Minimax, Status::Ok);
        snap.limits.five_hour = windows.0;
        snap.limits.week = windows.1;
        Ok(snap)
    }
}

/// A quota as the response describes it, before it has been assigned a meaning.
///
/// The endpoint states a used/limit pair but never how long the window runs, so
/// the length follows from the slot the entry is sorted into and is filled in
/// there rather than assumed here.
struct RawQuota {
    path: String,
    used_percent: f64,
    resets_at: Option<DateTime<Utc>>,
}

const FIVE_HOUR_MINUTES: u64 = 5 * 60;
const WEEK_MINUTES: u64 = 7 * 24 * 60;

fn parse_windows(
    body: &Value,
) -> Result<(Option<QuotaWindow>, Option<QuotaWindow>), ProviderError> {
    let mut quotas = Vec::new();
    walk(body, "", &mut quotas);
    if quotas.is_empty() {
        return Err(ProviderError::Parse(
            "MiniMax token-plan response did not include recognised quotas".into(),
        ));
    }
    let mut five_hour = None;
    let mut week = None;
    for quota in quotas {
        let label = quota.path.to_ascii_lowercase();
        let window = |minutes| QuotaWindow::new(quota.used_percent, quota.resets_at, minutes);
        if label.contains("week") || label.contains("7day") || label.contains("weekly") {
            week = Some(window(WEEK_MINUTES));
        } else if five_hour.is_none() {
            five_hour = Some(window(FIVE_HOUR_MINUTES));
        } else if week.is_none() {
            week = Some(window(WEEK_MINUTES));
        }
    }
    Ok((five_hour, week))
}

fn walk(value: &Value, path: &str, quotas: &mut Vec<RawQuota>) {
    match value {
        Value::Object(map) => {
            let limit = map
                .get("limit")
                .or_else(|| map.get("total"))
                .and_then(number);
            let used = map.get("used").and_then(number);
            let remaining = map
                .get("remaining")
                .or_else(|| map.get("remain"))
                .and_then(number);
            if let Some(limit) = limit.filter(|n| *n > 0.0) {
                let used = used.or_else(|| remaining.map(|n| (limit - n).max(0.0)));
                if let Some(used) = used {
                    let resets_at = map
                        .get("resetTime")
                        .or_else(|| map.get("reset_at"))
                        .or_else(|| map.get("nextResetTime"))
                        .and_then(parse_reset);
                    quotas.push(RawQuota {
                        path: path.to_string(),
                        used_percent: (used / limit) * 100.0,
                        resets_at,
                    });
                }
            }
            for (key, child) in map {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                walk(child, &next, quotas);
            }
        }
        Value::Array(items) => {
            for child in items {
                walk(child, path, quotas);
            }
        }
        _ => {}
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
}

fn parse_reset(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(raw) = value.as_str() {
        return raw.parse().ok();
    }
    let n = number(value)?;
    if n > 100_000_000_000.0 {
        Utc.timestamp_millis_opt(n as i64).single()
    } else {
        Utc.timestamp_opt(n as i64, 0).single()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_token_plan_response() {
        let body = serde_json::json!({"data":{"five_hour":{"used":"12","limit":"100","resetTime":"2030-01-01T05:00:00Z"},"weekly":{"remaining":800,"limit":1000,"resetTime":"2030-01-08T00:00:00Z"}}});
        let (five, week) = parse_windows(&body).unwrap();
        assert_eq!(five.unwrap().used_percent, 12.0);
        assert_eq!(week.unwrap().used_percent, 20.0);
    }

    /// The response carries no window length, so it has to come from the slot.
    /// Labelling the weekly plan quota a five-hour window was just wrong.
    #[test]
    fn each_window_reports_the_length_of_the_slot_it_landed_in() {
        let body = serde_json::json!({"data":{
            "five_hour":{"used":12,"limit":100},
            "weekly":{"used":200,"limit":1000}
        }});
        let (five, week) = parse_windows(&body).unwrap();
        assert_eq!(five.unwrap().window_minutes, 300);
        assert_eq!(week.unwrap().window_minutes, 10_080);
    }
}
