//! DeepSeek prepaid balance.
//!
//! The only figure DeepSeek exposes is remaining balance — there is no usage
//! or token endpoint. Spend-to-date is therefore derived by the store as the
//! drop between consecutive balance readings, not reported by the API.
//!
//! Docs: https://api-docs.deepseek.com/api/get-user-balance/

use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::time::Duration;

use super::{
    dollars_to_cents, http_error, Caps, FetchCtx, Provider, ProviderError, ProviderId, Status,
    UsageSnapshot, USER_AGENT,
};

const URL: &str = "https://api.deepseek.com/user/balance";

pub struct DeepSeek;

#[async_trait]
impl Provider for DeepSeek {
    fn id(&self) -> ProviderId {
        ProviderId::Deepseek
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_secs(5 * 60)
    }

    fn caps(&self) -> Caps {
        Caps {
            cost: false,
            tokens: false,
            balance: true,
            series: false,
        }
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<UsageSnapshot, ProviderError> {
        let resp = ctx
            .client
            .get(URL)
            .bearer_auth(ctx.key.expose_secret())
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(http_error(resp).await);
        }
        let body: BalanceResponse = resp.json().await?;

        let info = body
            .pick_usd()
            .ok_or_else(|| ProviderError::Parse("no balance entry in response".into()))?;
        let dollars: f64 = info
            .total_balance
            .trim()
            .parse()
            .map_err(|_| ProviderError::Parse("bad balance figure".into()))?;

        let mut snap = UsageSnapshot::empty(ProviderId::Deepseek, Status::Ok);
        snap.balance_cents = Some(dollars_to_cents(dollars));
        if !body.is_available {
            snap.message = Some("account not available for API calls".into());
        }
        Ok(snap)
    }
}

#[derive(Deserialize)]
struct BalanceResponse {
    #[serde(default)]
    is_available: bool,
    #[serde(default)]
    balance_infos: Vec<BalanceInfo>,
}

#[derive(Deserialize)]
struct BalanceInfo {
    #[serde(default)]
    currency: String,
    total_balance: String,
}

impl BalanceResponse {
    /// Accounts can hold both CNY and USD wallets. Prefer USD so the figure
    /// can be added to the cross-provider total; otherwise take what is there
    /// rather than showing nothing.
    fn pick_usd(&self) -> Option<&BalanceInfo> {
        self.balance_infos
            .iter()
            .find(|b| b.currency.eq_ignore_ascii_case("usd"))
            .or_else(|| self.balance_infos.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_usd_wallet_over_cny() {
        let body: BalanceResponse =
            serde_json::from_str(include_str!("../../fixtures/deepseek_balance.json")).unwrap();
        let info = body.pick_usd().unwrap();
        assert_eq!(info.currency, "USD");
        assert_eq!(
            dollars_to_cents(info.total_balance.parse().unwrap()),
            11_000
        );
    }

    #[test]
    fn falls_back_to_the_only_wallet_present() {
        let body: BalanceResponse = serde_json::from_str(
            r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"50.00","granted_balance":"0.00","topped_up_balance":"50.00"}]}"#,
        )
        .unwrap();
        assert_eq!(body.pick_usd().unwrap().currency, "CNY");
    }

    #[test]
    fn empty_balance_list_is_an_error_not_a_zero() {
        let body: BalanceResponse =
            serde_json::from_str(r#"{"is_available":false,"balance_infos":[]}"#).unwrap();
        assert!(body.pick_usd().is_none());
    }
}
