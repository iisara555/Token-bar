//! Z.AI / GLM Coding Plan quota.
//!
//! Z.AI's coding-plan dashboard is backed by a small monitor endpoint. It is
//! not advertised as a public API, so this adapter is deliberately defensive:
//! unknown limit entries are ignored and a parse error is shown instead of a
//! made-up percentage when the response shape changes.

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use secrecy::ExposeSecret;
use serde_json::Value;
use std::time::Duration;

use super::{
    http_error, Caps, FetchCtx, Provider, ProviderError, ProviderId, QuotaWindow, Status,
    UsageSnapshot, USER_AGENT,
};

const URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

pub struct Zai;

#[async_trait]
impl Provider for Zai {
    fn id(&self) -> ProviderId {
        ProviderId::Zai
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
        let (five_hour, week) = parse_limits(&body)?;
        let mut snap = UsageSnapshot::empty(ProviderId::Zai, Status::Ok);
        snap.limits.five_hour = five_hour;
        snap.limits.week = week;
        Ok(snap)
    }
}

/// A limit as the response describes it, before it has been assigned a meaning.
///
/// The endpoint does not state how long any of its windows runs, so the length
/// cannot be read off the response — it follows from which slot the entry lands
/// in, and is therefore filled in at that point rather than guessed at here.
struct RawLimit {
    name: String,
    used_percent: f64,
    resets_at: Option<DateTime<Utc>>,
}

const FIVE_HOUR_MINUTES: u64 = 5 * 60;
const WEEK_MINUTES: u64 = 7 * 24 * 60;

fn parse_limits(body: &Value) -> Result<(Option<QuotaWindow>, Option<QuotaWindow>), ProviderError> {
    let mut found: Vec<RawLimit> = Vec::new();
    walk(body, "", &mut found);
    if found.is_empty() {
        return Err(ProviderError::Parse(
            "Z.AI quota response did not include recognised limits".into(),
        ));
    }
    let mut five_hour = None;
    let mut week = None;
    for limit in found {
        let label = limit.name.to_ascii_lowercase();
        let window = |minutes| QuotaWindow::new(limit.used_percent, limit.resets_at, minutes);
        if label.contains("week") || label.contains("7day") || label.contains("seven") {
            week = Some(window(WEEK_MINUTES));
        } else if label.contains("token") || five_hour.is_none() {
            five_hour = Some(window(FIVE_HOUR_MINUTES));
        } else if week.is_none() {
            week = Some(window(WEEK_MINUTES));
        }
    }
    Ok((five_hour, week))
}

fn walk(value: &Value, path: &str, found: &mut Vec<RawLimit>) {
    match value {
        Value::Object(map) => {
            let limit = map
                .get("limit")
                .and_then(number)
                .or_else(|| map.get("total").and_then(number));
            let used = map.get("used").and_then(number);
            let percentage = map
                .get("percentage")
                .and_then(number)
                .or_else(|| map.get("usedPercentage").and_then(number));
            if limit.is_some() || percentage.is_some() {
                let used_percent = percentage.unwrap_or_else(|| {
                    limit
                        .filter(|n| *n > 0.0)
                        .and_then(|limit| used.map(|n| (n / limit) * 100.0))
                        .unwrap_or(0.0)
                });
                let name = map
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or(path)
                    .to_string();
                let resets_at = map
                    .get("nextResetTime")
                    .or_else(|| map.get("resetTime"))
                    .and_then(parse_reset);
                found.push(RawLimit {
                    name,
                    used_percent,
                    resets_at,
                });
            }
            for (key, child) in map {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                walk(child, &next, found);
            }
        }
        Value::Array(items) => {
            for child in items {
                walk(child, path, found);
            }
        }
        _ => {}
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().trim_end_matches('%').parse().ok())
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
    fn parses_the_current_monitor_shape() {
        let body = serde_json::json!({"data":{"limits":[
            {"type":"TOKENS_LIMIT","percentage":22,"nextResetTime":1893459600000_i64},
            {"type":"WEEK_LIMIT","percentage":41,"nextResetTime":1894064400000_i64}
        ]}});
        let (five, week) = parse_limits(&body).unwrap();
        assert_eq!(five.unwrap().used_percent, 22.0);
        assert_eq!(week.unwrap().used_percent, 41.0);
    }

    /// The response says nothing about how long its windows run, so the length
    /// has to follow the slot the entry was sorted into. Reporting the weekly
    /// quota as a five-hour one was simply false.
    #[test]
    fn each_window_reports_the_length_of_the_slot_it_landed_in() {
        let body = serde_json::json!({"data":{"limits":[
            {"type":"TOKENS_LIMIT","percentage":22},
            {"type":"WEEK_LIMIT","percentage":41}
        ]}});
        let (five, week) = parse_limits(&body).unwrap();
        assert_eq!(five.unwrap().window_minutes, 300);
        assert_eq!(week.unwrap().window_minutes, 10_080);
    }
}
