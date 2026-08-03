//! Google Gemini.
//!
//! Gemini has no first-party usage endpoint. Real cost data lives in GCP Cloud
//! Billing, which needs a service account, a signed-JWT OAuth exchange and
//! either a BigQuery billing export or the Cloud Billing API — and lands hours
//! to a day behind. That is phase 2 work; wiring it half-done would put a
//! confidently wrong number on the bar.
//!
//! Until then this reports manual entry, and says why.

use async_trait::async_trait;
use std::time::Duration;

use super::{manual, Caps, FetchCtx, Provider, ProviderError, ProviderId, UsageSnapshot};

pub struct Gemini;

#[async_trait]
impl Provider for Gemini {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_secs(30 * 60)
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

    fn manual_entry(&self) -> bool {
        true
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
        Ok(manual::snapshot(
            ProviderId::Gemini,
            &ctx.options,
            "Gemini cost needs GCP Cloud Billing (phase 2) — showing your manual entry",
        ))
    }
}
