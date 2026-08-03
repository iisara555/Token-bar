//! Kimi Code managed-plan usage.
//!
//! Kimi publishes the coding-plan usage endpoint used by its official CLI.
//! The endpoint accepts the Kimi Code OAuth token (and, where supported, an
//! API token) and exposes both the rolling five-hour window and the weekly
//! window.

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use secrecy::ExposeSecret;
use serde_json::Value;
use std::time::Duration;

use super::{
    http_error, Caps, FetchCtx, Provider, ProviderError, ProviderId, QuotaWindow, Status,
    UsageSnapshot, USER_AGENT,
};

const URL: &str = "https://api.kimi.com/coding/v1/usages";

pub struct Kimi;

#[async_trait]
impl Provider for Kimi {
    fn id(&self) -> ProviderId {
        ProviderId::Kimi
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
        let token = if ctx.use_oauth {
            crate::oauth::credential(ProviderId::Kimi)?.access_token
        } else {
            ctx.key.clone()
        };
        let resp = ctx
            .client
            .get(URL)
            .bearer_auth(token.expose_secret())
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(http_error(resp).await);
        }
        let body: Value = resp.json().await?;
        let limits = parse_limits(&body)?;
        let mut snap = UsageSnapshot::empty(ProviderId::Kimi, Status::Ok);
        snap.limits.five_hour = limits.five_hour;
        snap.limits.week = limits.week;
        Ok(snap)
    }
}

struct ParsedLimits {
    five_hour: Option<QuotaWindow>,
    week: Option<QuotaWindow>,
}

fn parse_limits(body: &Value) -> Result<ParsedLimits, ProviderError> {
    let mut five_hour = None;
    let mut week = None;

    // The summary usage is the weekly plan quota in the current API.
    if let Some(usage) = body.get("usage") {
        if let Some(window) = parse_window(usage, 10_080) {
            week = Some(window);
        }
    }

    if let Some(limits) = body.get("limits").and_then(Value::as_array) {
        for item in limits {
            let minutes = item
                .get("window")
                .and_then(|w| number(w.get("duration").unwrap_or(&Value::Null)))
                .map(|duration| {
                    let unit = item
                        .get("window")
                        .and_then(|w| w.get("timeUnit"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if unit.contains("DAY") {
                        duration * 24.0 * 60.0
                    } else if unit.contains("HOUR") {
                        duration * 60.0
                    } else {
                        duration
                    }
                })
                .unwrap_or(0.0) as u64;
            let detail = item.get("detail").unwrap_or(item);
            let window = parse_window(detail, minutes.max(1));
            if let Some(window) = window {
                if minutes <= 6 * 60 || (minutes == 0 && five_hour.is_none()) {
                    five_hour = Some(window);
                } else if minutes >= 6 * 24 * 60 {
                    week = Some(window);
                } else if five_hour.is_none() {
                    five_hour = Some(window);
                }
            }
        }
    }

    if five_hour.is_none() && week.is_none() {
        return Err(ProviderError::Parse(
            "Kimi usage response did not include recognised quota windows".into(),
        ));
    }
    Ok(ParsedLimits { five_hour, week })
}

fn parse_window(value: &Value, minutes: u64) -> Option<QuotaWindow> {
    let limit = number(value.get("limit")?)?;
    if limit <= 0.0 {
        return None;
    }
    let used = value.get("used").and_then(number).or_else(|| {
        value
            .get("remaining")
            .or_else(|| value.get("remain"))
            .and_then(number)
            .map(|remaining| (limit - remaining).max(0.0))
    })?;
    Some(QuotaWindow::new(
        (used / limit) * 100.0,
        parse_reset(value.get("resetTime")),
        minutes,
    ))
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
}

fn parse_reset(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let value = value?;
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
    fn parses_kimi_usage_windows() {
        let body: Value = serde_json::json!({
            "usage": {"used": "40", "limit": "1000", "resetTime": "2030-01-08T00:00:00Z"},
            "limits": [{"window": {"duration": 300, "timeUnit": "TIME_UNIT_MINUTE"}, "detail": {"used": "10", "limit": "100", "resetTime": "2030-01-01T05:00:00Z"}}]
        });
        let parsed = parse_limits(&body).unwrap();
        assert_eq!(parsed.five_hour.unwrap().used_percent, 10.0);
        assert_eq!(parsed.week.unwrap().used_percent, 4.0);
    }
}
