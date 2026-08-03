//! Mistral.
//!
//! Usage and billing live in the Mistral console only; there is no public
//! programmatic endpoint. Falls back to manual entry.

use async_trait::async_trait;
use std::time::Duration;

use super::{manual, Caps, FetchCtx, Provider, ProviderError, ProviderId, UsageSnapshot};

pub struct Mistral;

#[async_trait]
impl Provider for Mistral {
    fn id(&self) -> ProviderId {
        ProviderId::Mistral
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
            ProviderId::Mistral,
            &ctx.options,
            "Mistral publishes no usage API — showing your manual entry",
        ))
    }
}
