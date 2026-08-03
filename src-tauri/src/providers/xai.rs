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

/// Balance field names, in cents and in dollars.
const BALANCE_CENTS_KEYS: [&str; 3] = ["balance_cents", "amount_cents", "credits_cents"];
const BALANCE_DOLLAR_KEYS: [&str; 6] = [
    "balance",
    "prepaid_balance",
    "total_balance",
    "remaining_balance",
    "credits",
    "amount",
];

/// Spend field names. Deliberately disjoint from the balance list: the usage
/// endpoint and the balance endpoint have similar shapes, and reading a
/// `credits` or `balance` field out of the usage response would report money
/// *left* as money *spent* — an error that grows the reported spend as the
/// account is topped up.
const USAGE_CENTS_KEYS: [&str; 3] = ["usage_cents", "spend_cents", "cost_cents"];
const USAGE_DOLLAR_KEYS: [&str; 6] = [
    "total_usage",
    "usage",
    "total_spend",
    "spend",
    "total_cost",
    "cost",
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
        let balance_cents = extract_balance(&balance).ok_or_else(|| {
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
            snap.cost_cents = extract_usage(&usage);
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

/// Depth-first search for the first field named by `cents_keys` or
/// `dollar_keys`, returning cents. Accepts JSON numbers and decimal strings.
fn extract(v: &Value, cents_keys: &[&str], dollar_keys: &[&str]) -> Option<i64> {
    match v {
        Value::Object(map) => {
            for k in cents_keys {
                if let Some(n) = map.get(*k).and_then(as_f64) {
                    return Some(n.round() as i64);
                }
            }
            for k in dollar_keys {
                if let Some(n) = map.get(*k).and_then(as_f64) {
                    return Some(dollars_to_cents(n));
                }
            }
            map.values()
                .find_map(|c| extract(c, cents_keys, dollar_keys))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|c| extract(c, cents_keys, dollar_keys)),
        _ => None,
    }
}

fn extract_balance(v: &Value) -> Option<i64> {
    extract(v, &BALANCE_CENTS_KEYS, &BALANCE_DOLLAR_KEYS)
}

fn extract_usage(v: &Value) -> Option<i64> {
    extract(v, &USAGE_CENTS_KEYS, &USAGE_DOLLAR_KEYS)
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
        assert_eq!(extract_balance(&v), Some(2_575));
    }

    #[test]
    fn prefers_an_explicit_cents_field_over_a_dollar_one() {
        let v: Value = serde_json::from_str(r#"{"balance_cents":1234,"balance":99.0}"#).unwrap();
        assert_eq!(extract_balance(&v), Some(1234));
    }

    #[test]
    fn accepts_a_decimal_string_amount() {
        let v: Value = serde_json::from_str(r#"{"data":{"balance":"12.50"}}"#).unwrap();
        assert_eq!(extract_balance(&v), Some(1250));
    }

    #[test]
    fn unknown_shape_yields_none_so_the_caller_can_report_it() {
        let v: Value = serde_json::from_str(r#"{"status":"ok","team":"abc"}"#).unwrap();
        assert_eq!(extract_balance(&v), None);
        assert_eq!(top_level_keys(&v), "status, team");
    }

    /// The two endpoints have similar shapes. Reading remaining credit out of
    /// the usage response reported it as spend, so a top-up looked like a
    /// purchase — the reading had to be wrong in the alarming direction.
    #[test]
    fn a_balance_field_is_never_read_as_spend() {
        let usage: Value =
            serde_json::from_str(r#"{"data":{"balance":25.75,"credits":100.0}}"#).unwrap();
        assert_eq!(extract_usage(&usage), None);
    }

    #[test]
    fn reads_spend_from_the_usage_shape() {
        let usage: Value = serde_json::from_str(r#"{"data":{"total_usage":"4.20"}}"#).unwrap();
        assert_eq!(extract_usage(&usage), Some(420));
        // And the balance parser does not pick up a spend figure either.
        assert_eq!(extract_balance(&usage), None);
    }
}
