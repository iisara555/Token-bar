//! xAI Management API — prepaid balance and usage.
//!
//! Base host is `management-api.x.ai`, separate from the inference host, and
//! every endpoint is scoped to a team id. The management key and the inference
//! key are different credentials.
//!
//! Docs: https://docs.x.ai/docs/management-api/billing
//!
//! The exact response shape is not publicly pinned down the way Anthropic's and
//! OpenAI's are, so the parser probes a set of candidate field names rather
//! than binding to one guess. If none match we surface a parse error naming the
//! keys we did see — better than silently reporting $0.00.

use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde_json::Value;
use std::time::Duration;

use super::{
    dollars_to_cents, http_error, Caps, FetchCtx, Provider, ProviderError, ProviderId, Status,
    UsageSnapshot, USER_AGENT,
};

const BASE: &str = "https://management-api.x.ai/v1";

/// Field names that carry an amount already denominated in cents.
const CENTS_KEYS: [&str; 3] = ["balance_cents", "amount_cents", "credits_cents"];
/// Field names that carry an amount denominated in dollars.
const DOLLAR_KEYS: [&str; 6] = [
    "balance",
    "prepaid_balance",
    "total_balance",
    "remaining_balance",
    "credits",
    "amount",
];

pub struct XAi;

#[async_trait]
impl Provider for XAi {
    fn id(&self) -> ProviderId {
        ProviderId::Xai
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

    fn required_options(&self) -> &'static [&'static str] {
        &["team_id"]
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
        let team = ctx.option("team_id")?;

        let balance =
            get_json(ctx, &format!("{BASE}/billing/teams/{team}/prepaid/balance")).await?;
        let balance_cents = extract_amount(&balance).ok_or_else(|| {
            ProviderError::Parse(format!(
                "no recognisable balance field; response had keys [{}]",
                top_level_keys(&balance)
            ))
        })?;

        let mut snap = UsageSnapshot::empty(ProviderId::Xai, Status::Ok);
        snap.balance_cents = Some(balance_cents);

        // Usage is a nice-to-have: a shape change there should not blank out a
        // balance we already read successfully.
        if let Ok(usage) = get_json(ctx, &format!("{BASE}/billing/teams/{team}/usage")).await {
            snap.cost_cents = extract_amount(&usage);
        }

        Ok(snap)
    }
}

async fn get_json(ctx: &FetchCtx, url: &str) -> Result<Value, ProviderError> {
    let resp = ctx
        .client
        .get(url)
        .bearer_auth(ctx.key.expose_secret())
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(http_error(resp).await);
    }
    Ok(resp.json().await?)
}

fn top_level_keys(v: &Value) -> String {
    match v {
        Value::Object(m) => m.keys().cloned().collect::<Vec<_>>().join(", "),
        other => format!("<{}>", type_name(other)),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Depth-first search for the first amount-shaped field, in cents.
/// Accepts both JSON numbers and decimal strings.
fn extract_amount(v: &Value) -> Option<i64> {
    match v {
        Value::Object(map) => {
            for k in CENTS_KEYS {
                if let Some(n) = map.get(k).and_then(as_f64) {
                    return Some(n.round() as i64);
                }
            }
            for k in DOLLAR_KEYS {
                if let Some(n) = map.get(k).and_then(as_f64) {
                    return Some(dollars_to_cents(n));
                }
            }
            map.values().find_map(extract_amount)
        }
        Value::Array(items) => items.iter().find_map(extract_amount),
        _ => None,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_nested_dollar_balance() {
        let v: Value =
            serde_json::from_str(include_str!("../../fixtures/xai_balance.json")).unwrap();
        assert_eq!(extract_amount(&v), Some(2_575));
    }

    #[test]
    fn prefers_an_explicit_cents_field_over_a_dollar_one() {
        let v: Value = serde_json::from_str(r#"{"balance_cents":1234,"balance":99.0}"#).unwrap();
        assert_eq!(extract_amount(&v), Some(1234));
    }

    #[test]
    fn accepts_a_decimal_string_amount() {
        let v: Value = serde_json::from_str(r#"{"data":{"balance":"12.50"}}"#).unwrap();
        assert_eq!(extract_amount(&v), Some(1250));
    }

    #[test]
    fn unknown_shape_yields_none_so_the_caller_can_report_it() {
        let v: Value = serde_json::from_str(r#"{"status":"ok","team":"abc"}"#).unwrap();
        assert_eq!(extract_amount(&v), None);
        assert_eq!(top_level_keys(&v), "status, team");
    }
}
