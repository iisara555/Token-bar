//! Bridge to official Claude Code, Codex and Kimi Code OAuth sessions.
//!
//! Credentials are re-read from the official client files on every poll. When
//! Claude Code's short-lived access token has expired, Token Bar uses the
//! refresh token through Anthropic's OAuth endpoint and writes the rotated pair
//! back to the same official file so Claude Code and Token Bar share the login.

use chrono::{TimeZone, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::sync::Mutex;

use crate::config::AuthMode;
use crate::providers::{ProviderError, ProviderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthStatus {
    Connected,
    Expired,
    NotFound,
}

pub struct OAuthCredential {
    pub access_token: SecretString,
    pub account_id: Option<String>,
}

pub fn supports(id: ProviderId) -> bool {
    matches!(
        id,
        ProviderId::Anthropic | ProviderId::Openai | ProviderId::Kimi
    )
}

pub fn should_use(id: ProviderId, mode: AuthMode) -> bool {
    if !supports(id) {
        return false;
    }
    match mode {
        AuthMode::Oauth => true,
        AuthMode::ApiKey => false,
        // An expired login still counts as the selected source: silently
        // switching to a billable API key would be surprising and expensive.
        AuthMode::Auto => status(id) != OAuthStatus::NotFound,
    }
}

pub fn status(id: ProviderId) -> OAuthStatus {
    match id {
        ProviderId::Anthropic => match read_claude_file() {
            Ok(file) => match file.claude_ai_oauth {
                Some(oauth) if !oauth.access_token.trim().is_empty() => {
                    let expired = oauth
                        .expires_at
                        .and_then(|millis| Utc.timestamp_millis_opt(millis).single())
                        .is_some_and(|at| at <= Utc::now());
                    let refresh_expired = oauth
                        .refresh_token_expires_at
                        .and_then(|millis| Utc.timestamp_millis_opt(millis).single())
                        .is_some_and(|at| at <= Utc::now());
                    if (expired
                        && oauth
                            .refresh_token
                            .as_deref()
                            .map(str::is_empty)
                            .unwrap_or(true))
                        || refresh_expired
                    {
                        OAuthStatus::Expired
                    } else {
                        OAuthStatus::Connected
                    }
                }
                _ => OAuthStatus::NotFound,
            },
            Err(_) => OAuthStatus::NotFound,
        },
        ProviderId::Openai => match read_codex_file() {
            Ok(file)
                if file
                    .tokens
                    .as_ref()
                    .is_some_and(|tokens| !tokens.access_token.trim().is_empty()) =>
            {
                OAuthStatus::Connected
            }
            _ => OAuthStatus::NotFound,
        },
        ProviderId::Kimi => match read_kimi_file() {
            Ok(file) if !file.access_token.trim().is_empty() => {
                if file.expires_at.is_some_and(kimi_expired) {
                    OAuthStatus::Expired
                } else {
                    OAuthStatus::Connected
                }
            }
            _ => OAuthStatus::NotFound,
        },
        _ => OAuthStatus::NotFound,
    }
}

pub fn credential(id: ProviderId) -> Result<OAuthCredential, ProviderError> {
    match id {
        ProviderId::Anthropic => claude_credential(),
        ProviderId::Openai => codex_credential(),
        ProviderId::Kimi => kimi_credential(),
        _ => Err(ProviderError::Config(
            "OAuth is not supported for this provider".into(),
        )),
    }
}

/// Resolve a credential for a network request. Anthropic OAuth access tokens
/// are short-lived, so this async variant can refresh them without blocking the
/// Tauri runtime. Other providers keep the original read-only path.
pub async fn credential_async(
    id: ProviderId,
    client: &reqwest::Client,
    force_refresh: bool,
) -> Result<OAuthCredential, ProviderError> {
    if id == ProviderId::Anthropic {
        claude_credential_async(client, force_refresh).await
    } else {
        credential(id)
    }
}

async fn claude_credential_async(
    client: &reqwest::Client,
    force_refresh: bool,
) -> Result<OAuthCredential, ProviderError> {
    let path = claude_credentials_path()
        .map_err(|_| ProviderError::Oauth("Claude Code login was not found on this PC".into()))?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| ProviderError::Oauth("Claude Code login was not found on this PC".into()))?;
    let mut file: ClaudeCredentials = serde_json::from_str(&raw)
        .map_err(|_| ProviderError::Oauth("Claude Code credentials could not be read".into()))?;
    let current = file
        .claude_ai_oauth
        .as_ref()
        .filter(|oauth| !oauth.access_token.trim().is_empty())
        .ok_or_else(|| ProviderError::Oauth("Claude Code login was not found on this PC".into()))?;

    let expired = current
        .expires_at
        .and_then(|millis| Utc.timestamp_millis_opt(millis).single())
        .is_some_and(|at| at <= Utc::now());
    if !force_refresh && !expired {
        return Ok(OAuthCredential {
            access_token: SecretString::from(current.access_token.clone()),
            account_id: None,
        });
    }

    let refresh_token = current.refresh_token.clone().ok_or_else(|| {
        ProviderError::Oauth("Claude Code login expired; sign in with Claude Code again".into())
    })?;
    if refresh_token.trim().is_empty() {
        return Err(ProviderError::Oauth(
            "Claude Code login expired; sign in with Claude Code again".into(),
        ));
    }

    // Refresh tokens are normally single-use. Serialize refreshes within this
    // process, then re-read the official file in case Claude Code refreshed it
    // while we were waiting for the lock.
    let lock = CLAUDE_REFRESH_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().await;
    let latest_raw = std::fs::read_to_string(&path).unwrap_or(raw.clone());
    file = serde_json::from_str(&latest_raw)
        .map_err(|_| ProviderError::Oauth("Claude Code credentials could not be read".into()))?;
    let latest = file
        .claude_ai_oauth
        .as_ref()
        .filter(|oauth| !oauth.access_token.trim().is_empty())
        .ok_or_else(|| ProviderError::Oauth("Claude Code login was not found on this PC".into()))?;
    let latest_expired = latest
        .expires_at
        .and_then(|millis| Utc.timestamp_millis_opt(millis).single())
        .is_some_and(|at| at <= Utc::now());
    if !force_refresh && !latest_expired {
        return Ok(OAuthCredential {
            access_token: SecretString::from(latest.access_token.clone()),
            account_id: None,
        });
    }
    let refresh_token = latest
        .refresh_token
        .clone()
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| {
            ProviderError::Oauth("Claude Code login expired; sign in with Claude Code again".into())
        })?;

    // Parse and validate the document we are going to write back *before*
    // spending the refresh token. Refresh tokens are single-use, so discovering
    // a structural problem afterwards would burn the user's login for nothing.
    let mut document: serde_json::Value = serde_json::from_str(&latest_raw)
        .map_err(|_| ProviderError::Oauth("Claude Code credentials could not be read".into()))?;
    if !document
        .get("claudeAiOauth")
        .is_some_and(serde_json::Value::is_object)
    {
        return Err(ProviderError::Oauth(
            "Claude Code credentials could not be read".into(),
        ));
    }

    let token = refresh_claude_token(client, &refresh_token).await?;
    let access_token = token.access_token.clone();
    let expires_at = token
        .expires_at
        .or_else(|| {
            token
                .expires_in
                .map(|seconds| Utc::now().timestamp_millis() + seconds * 1000)
        })
        .ok_or_else(|| ProviderError::Oauth("Claude Code refresh returned no expiry".into()))?;
    let oauth = document
        .get_mut("claudeAiOauth")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| ProviderError::Oauth("Claude Code credentials could not be read".into()))?;
    oauth.insert(
        "accessToken".into(),
        serde_json::Value::String(access_token.clone()),
    );
    if let Some(refresh_token) = token.refresh_token {
        oauth.insert(
            "refreshToken".into(),
            serde_json::Value::String(refresh_token),
        );
    }
    oauth.insert("expiresAt".into(), serde_json::Value::from(expires_at));
    if let Some(refresh_expires_at) = token.refresh_token_expires_at {
        oauth.insert(
            "refreshTokenExpiresAt".into(),
            serde_json::Value::from(refresh_expires_at),
        );
    }
    let encoded = serde_json::to_vec_pretty(&document)
        .map_err(|_| ProviderError::Oauth("Claude Code credentials could not be saved".into()))?;
    write_credentials(&path, &encoded)
        .map_err(|_| ProviderError::Oauth("Claude Code credentials could not be saved".into()))?;

    Ok(OAuthCredential {
        access_token: SecretString::from(access_token),
        account_id: None,
    })
}

/// Replace the official client's credentials file without ever leaving it
/// truncated.
///
/// This file belongs to Claude Code, not to Token Bar: a half-written save here
/// signs the user out of the CLI they work in, so it gets the same
/// write-then-rename treatment the app already gives its own config, and the
/// temporary file is locked down before the rename rather than after — a file
/// created under the default umask would otherwise become world-readable at the
/// instant it takes the real file's place.
fn write_credentials(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tokenbar-tmp");
    std::fs::write(&tmp, bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0o600);
        if let Err(e) = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }

    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

async fn refresh_claude_token(
    client: &reqwest::Client,
    refresh_token: &str,
) -> Result<ClaudeTokenResponse, ProviderError> {
    const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
    });
    // Claude Code moved its OAuth token exchange to platform.claude.com. Keep
    // the older console host as a narrow compatibility fallback for existing
    // installations; only retry when the first host clearly lacks the route,
    // never after an authentication or validation response.
    let response = client
        .post("https://platform.claude.com/v1/oauth/token")
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, "claude-cli/1.0.0 (external)")
        .json(&body)
        .send()
        .await
        .map_err(|_| {
            ProviderError::Oauth("Claude Code token refresh failed; sign in again".into())
        })?;
    let response = if matches!(response.status().as_u16(), 404 | 405 | 501) {
        client
            .post("https://console.anthropic.com/v1/oauth/token")
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, "claude-cli/1.0.0 (external)")
            .json(&body)
            .send()
            .await
            .map_err(|_| {
                ProviderError::Oauth("Claude Code token refresh failed; sign in again".into())
            })?
    } else {
        response
    };
    if !response.status().is_success() {
        return Err(ProviderError::Oauth(
            "Claude Code token refresh failed; sign in again".into(),
        ));
    }
    response
        .json::<ClaudeTokenResponse>()
        .await
        .map_err(|_| ProviderError::Oauth("Claude Code refresh returned an invalid token".into()))
}

fn kimi_credential() -> Result<OAuthCredential, ProviderError> {
    let file = read_kimi_file()
        .map_err(|_| ProviderError::Oauth("Kimi Code login was not found on this PC".into()))?;
    if file.access_token.trim().is_empty() {
        return Err(ProviderError::Oauth(
            "Kimi Code login was not found on this PC".into(),
        ));
    }
    if file.expires_at.is_some_and(kimi_expired) {
        return Err(ProviderError::Oauth(
            "Kimi Code login expired; sign in with Kimi Code again".into(),
        ));
    }
    Ok(OAuthCredential {
        access_token: SecretString::from(file.access_token),
        account_id: None,
    })
}

fn claude_credential() -> Result<OAuthCredential, ProviderError> {
    let file = read_claude_file()
        .map_err(|_| ProviderError::Oauth("Claude Code login was not found on this PC".into()))?;
    let oauth = file
        .claude_ai_oauth
        .filter(|oauth| !oauth.access_token.trim().is_empty())
        .ok_or_else(|| ProviderError::Oauth("Claude Code login was not found on this PC".into()))?;

    if oauth
        .expires_at
        .and_then(|millis| Utc.timestamp_millis_opt(millis).single())
        .is_some_and(|at| at <= Utc::now())
    {
        return Err(ProviderError::Oauth(
            "Claude Code login expired; sign in with Claude Code again".into(),
        ));
    }

    Ok(OAuthCredential {
        access_token: SecretString::from(oauth.access_token),
        account_id: None,
    })
}

fn codex_credential() -> Result<OAuthCredential, ProviderError> {
    let file = read_codex_file()
        .map_err(|_| ProviderError::Oauth("Codex login was not found on this PC".into()))?;
    let tokens = file
        .tokens
        .filter(|tokens| !tokens.access_token.trim().is_empty())
        .ok_or_else(|| ProviderError::Oauth("Codex login was not found on this PC".into()))?;

    Ok(OAuthCredential {
        access_token: SecretString::from(tokens.access_token),
        account_id: tokens.account_id.filter(|id| !id.trim().is_empty()),
    })
}

fn read_claude_file() -> Result<ClaudeCredentials, std::io::Error> {
    let raw = std::fs::read_to_string(claude_credentials_path()?)?;
    serde_json::from_str(&raw).map_err(std::io::Error::other)
}

fn read_codex_file() -> Result<CodexCredentials, std::io::Error> {
    let raw = std::fs::read_to_string(codex_credentials_path()?)?;
    serde_json::from_str(&raw).map_err(std::io::Error::other)
}

fn read_kimi_file() -> Result<KimiCredentials, std::io::Error> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("KIMI_CODE_HOME") {
        let dir = PathBuf::from(dir);
        candidates.push(dir.join("credentials").join("oauth-kimi-code.json"));
        candidates.push(dir.join("credentials").join("kimi-code.json"));
    }
    let home = home_dir()?;
    for root in [home.join(".kimi-code"), home.join(".kimi")] {
        let credentials = root.join("credentials");
        candidates.push(credentials.join("oauth-kimi-code.json"));
        candidates.push(credentials.join("kimi-code.json"));
        candidates.push(credentials.join("oauth_kimi_code.json"));
    }
    let mut last_error = None;
    for path in candidates {
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<KimiCredentials>(&raw) {
                Ok(file) => return Ok(file),
                Err(e) => last_error = Some(std::io::Error::other(e)),
            },
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Kimi credentials not found")
    }))
}

fn kimi_expired(value: i64) -> bool {
    let at = if value > 100_000_000_000 {
        Utc.timestamp_millis_opt(value).single()
    } else {
        Utc.timestamp_opt(value, 0).single()
    };
    at.is_some_and(|at| at <= Utc::now())
}

fn claude_credentials_path() -> Result<PathBuf, std::io::Error> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join(".credentials.json"));
    }
    Ok(home_dir()?.join(".claude").join(".credentials.json"))
}

fn codex_credentials_path() -> Result<PathBuf, std::io::Error> {
    if let Some(dir) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(dir).join("auth.json"));
    }
    Ok(home_dir()?.join(".codex").join("auth.json"))
}

fn home_dir() -> Result<PathBuf, std::io::Error> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "user home not found"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCredentials {
    claude_ai_oauth: Option<ClaudeOAuth>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOAuth {
    access_token: String,
    expires_at: Option<i64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    refresh_token_expires_at: Option<i64>,
}

#[derive(Deserialize)]
struct ClaudeTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    expires_at: Option<i64>,
    #[serde(default)]
    refresh_token_expires_at: Option<i64>,
}

#[derive(Deserialize)]
struct CodexCredentials {
    tokens: Option<CodexTokens>,
}

#[derive(Deserialize)]
struct CodexTokens {
    access_token: String,
    account_id: Option<String>,
}

#[derive(Deserialize)]
struct KimiCredentials {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    expires_at: Option<i64>,
}

static CLAUDE_REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_shape_without_exposing_refresh_token() {
        let file: ClaudeCredentials = serde_json::from_str(
            r#"{"claudeAiOauth":{"accessToken":"secret","expiresAt":4102444800000,"refreshToken":"ignored"}}"#,
        )
        .unwrap();
        assert_eq!(file.claude_ai_oauth.unwrap().access_token, "secret");
    }

    #[test]
    fn parses_codex_shape() {
        let file: CodexCredentials = serde_json::from_str(
            r#"{"tokens":{"access_token":"secret","account_id":"account-1"}}"#,
        )
        .unwrap();
        let tokens = file.tokens.unwrap();
        assert_eq!(tokens.account_id.as_deref(), Some("account-1"));
    }

    #[test]
    fn credentials_are_replaced_without_leaving_a_stray_temp_file() {
        let dir = std::env::temp_dir().join("token-bar-oauth-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".credentials.json");
        std::fs::write(&path, br#"{"claudeAiOauth":{"accessToken":"old"}}"#).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // What the official clients actually write it as.
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }

        write_credentials(&path, br#"{"claudeAiOauth":{"accessToken":"new"}}"#).unwrap();

        let back = std::fs::read_to_string(&path).unwrap();
        assert!(back.contains("new"), "{back}");
        assert!(
            !path.with_extension("tokenbar-tmp").exists(),
            "temp file was left behind"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The replacement must not be looser than what it replaced.
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o077;
            assert_eq!(mode, 0, "credentials became group/world readable");
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parses_kimi_code_shape_and_seconds_expiry() {
        let file: KimiCredentials = serde_json::from_str(
            r#"{"access_token":"secret","refresh_token":"ignored","expires_at":4102444800}"#,
        )
        .unwrap();
        assert_eq!(file.access_token, "secret");
        assert!(!kimi_expired(file.expires_at.unwrap()));
    }
}
