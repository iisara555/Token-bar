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

fn parse_limits(body: &Value) -> Result<(Option<QuotaWindow>, Option<QuotaWindow>), ProviderError> {
    let mut found: Vec<(String, QuotaWindow)> = Vec::new();
    walk(body, "", &mut found);
    if found.is_empty() {
        return Err(ProviderError::Parse(
            "Z.AI quota response did not include recognised limits".into(),
        ));
    }
    let mut five_hour = None;
    let mut week = None;
    for (name, window) in found {
        let label = name.to_ascii_lowercase();
        if label.contains("week") || label.contains("7day") || label.contains("seven") {
            week = Some(window);
        } else if label.contains("token") || five_hour.is_none() {
            five_hour = Some(window);
        } else if week.is_none() {
            week = Some(window);
        }
    }
    Ok((five_hour, week))
}

fn walk(value: &Value, path: &str, found: &mut Vec<(String, QuotaWindow)>) {
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
                let reset = map
                    .get("nextResetTime")
                    .or_else(|| map.get("resetTime"))
                    .and_then(parse_reset);
                found.push((name, QuotaWindow::new(used_percent, reset, 300)));
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
}
