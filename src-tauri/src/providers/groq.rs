//! Groq.
//!
//! Groq exposes spend limits in the console but publishes no balance or usage
//! endpoint, so there is nothing to poll. Falls back to manual entry rather
//! than pretending the number is live.

use async_trait::async_trait;
use std::time::Duration;

use super::{manual, Caps, FetchCtx, Provider, ProviderError, ProviderId, UsageSnapshot};

pub struct Groq;

#[async_trait]
impl Provider for Groq {
    fn id(&self) -> ProviderId {
        ProviderId::Groq
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_secs(3600)
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
        Ok(manual::snapshot(
            ProviderId::Groq,
            &ctx.options,
            "Groq publishes no usage API — showing your manual entry",
        ))
    }
}
