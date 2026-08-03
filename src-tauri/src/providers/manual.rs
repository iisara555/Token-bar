//! Manual-entry fallback.
//!
//! Used by providers that publish no usage or balance endpoint (Groq, Mistral)
//! and by flat-rate subscriptions (Claude Pro/Max, ChatGPT Plus) which no API
//! covers. The user types what they know; the bar shows it next to the live
//! figures so one glance still gives a complete picture.
//!
//! Everything here is read from non-secret options — no credential involved.

use std::collections::BTreeMap;

use super::{dollars_to_cents, ProviderId, Status, UsageSnapshot};

/// Build a snapshot from the user's own numbers.
///
/// `manual_spend_usd` — what they believe they have spent this period.
/// `budget_usd`       — the cap they want the bar to measure against.
pub fn snapshot(
    provider: ProviderId,
    options: &BTreeMap<String, String>,
    note: &str,
) -> UsageSnapshot {
    let mut snap = UsageSnapshot::empty(provider, Status::Unsupported);
    snap.cost_cents = read_usd(options, "manual_spend_usd");
    snap.balance_cents = match (read_usd(options, "budget_usd"), snap.cost_cents) {
        (Some(budget), Some(spent)) => Some(budget - spent),
        (Some(budget), None) => Some(budget),
        _ => None,
    };
    snap.message = Some(note.to_string());
    snap
}

fn read_usd(options: &BTreeMap<String, String>, key: &str) -> Option<i64> {
    options
        .get(key)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<f64>().ok())
        .map(dollars_to_cents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn remaining_budget_is_budget_minus_spend() {
        let snap = snapshot(
            ProviderId::Groq,
            &opts(&[("budget_usd", "50"), ("manual_spend_usd", "12.50")]),
            "no usage API",
        );
        assert_eq!(snap.cost_cents, Some(1250));
        assert_eq!(snap.balance_cents, Some(3750));
        assert_eq!(snap.status, Status::Unsupported);
    }

    #[test]
    fn budget_alone_is_shown_as_fully_remaining() {
        let snap = snapshot(ProviderId::Mistral, &opts(&[("budget_usd", "20")]), "x");
        assert_eq!(snap.balance_cents, Some(2000));
        assert_eq!(snap.cost_cents, None);
    }

    #[test]
    fn junk_input_is_ignored_rather_than_read_as_zero() {
        let snap = snapshot(ProviderId::Groq, &opts(&[("budget_usd", "abc")]), "x");
        assert_eq!(snap.balance_cents, None);
    }

    #[test]
    fn no_options_still_produces_a_renderable_snapshot() {
        let snap = snapshot(ProviderId::Groq, &opts(&[]), "no usage API");
        assert_eq!(snap.cost_cents, None);
        assert_eq!(snap.balance_cents, None);
        assert_eq!(snap.message.as_deref(), Some("no usage API"));
    }
}
