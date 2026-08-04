//! Bridge to official Claude Code, Codex and Kimi Code OAuth sessions — and,
//! for Antigravity, a login Token Bar performs itself.
//!
//! Credentials are re-read from the official client files on every poll. When
//! Claude Code's short-lived access token has expired, Token Bar uses the
//! refresh token through Anthropic's OAuth endpoint and writes the rotated pair
//! back to the same official file so Claude Code and Token Bar share the login.
//!
//! Antigravity has no official client file to read: it is a separate Google
//! account login with its own OAuth client, so Token Bar runs its own PKCE
//! sign-in (`antigravity_login`) and keeps the resulting refresh token in the
//! OS credential store under a service name of its own — never under the
//! `com.tokenbar.app` service `secrets.rs` uses for pasted API keys, so it
//! never shows up as a garbled "fingerprint" in Settings.

use base64::Engine;
use chrono::{TimeZone, Utc};
use keyring::Entry;
use rand::RngCore;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
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
        ProviderId::Anthropic | ProviderId::Openai | ProviderId::Kimi | ProviderId::Antigravity
    )
}

pub fn should_use(id: ProviderId, mode: AuthMode) -> bool {
    if !supports(id) {
        return false;
    }
    // Antigravity has no API-key fallback to opt into — Auto/OAuth/ApiKey all
    // mean the same thing: use the login if one is connected.
    if id == ProviderId::Antigravity {
        return status(id) != OAuthStatus::NotFound;
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
        // Google's access tokens expire hourly by design and refresh silently;
        // unlike Claude Code there is no separate file another process could
        // have already refreshed, so — like Codex — the only local signal
        // worth showing is whether a login was ever completed.
        ProviderId::Antigravity => match read_antigravity_tokens() {
            Ok(tokens) if !tokens.refresh_token.trim().is_empty() => OAuthStatus::Connected,
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
        ProviderId::Antigravity => Err(ProviderError::Config(
            "Antigravity requires the async OAuth path".into(),
        )),
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
    match id {
        ProviderId::Anthropic => claude_credential_async(client, force_refresh).await,
        ProviderId::Antigravity => antigravity_credential_async(client, force_refresh).await,
        _ => credential(id),
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

// ---------------------------------------------------------------------------
// Antigravity — Token Bar's own Google OAuth login
// ---------------------------------------------------------------------------
//
// Unlike Claude Code/Codex/Kimi Code, there is no existing official client
// login to read: Antigravity is a Google account sign-in with its own OAuth
// client, so Token Bar runs the PKCE dance itself via a loopback redirect and
// keeps the refresh token in the OS credential store.
//
// The client id and secret are read from the environment rather than
// compiled in: they are Google's identifier and secret for a real registered
// OAuth client, and while an "installed application" client secret is not
// confidential the way an API key is, it is still a credential issued to
// someone else's Google Cloud project, not one this app is entitled to
// redistribute inside its own published source. A packager sets
// `ANTIGRAVITY_CLIENT_ID`/`ANTIGRAVITY_CLIENT_SECRET` at build or launch time
// — see README for what to put there.

const ANTIGRAVITY_SERVICE: &str = "com.tokenbar.app.oauth";
const ANTIGRAVITY_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const ANTIGRAVITY_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const ANTIGRAVITY_SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs";
/// How long the local loopback server waits for the browser round trip before
/// giving up and telling the user to try again.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Serialize, Deserialize)]
struct AntigravityTokens {
    access_token: String,
    refresh_token: String,
    /// Milliseconds since epoch, matching how the Claude credential file
    /// already spells expiry in this module.
    expires_at: i64,
}

#[derive(Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

const ANTIGRAVITY_NOT_CONFIGURED: &str =
    "Antigravity needs ANTIGRAVITY_CLIENT_ID and ANTIGRAVITY_CLIENT_SECRET set for this build — see README";

/// Baked in by the release build (see `.github/workflows/build.yml`, same
/// pattern as the `APPLE_*` signing secrets) when the repository has one
/// configured, with a runtime env var as a fallback for `cargo tauri dev`.
fn antigravity_client_id() -> Result<String, ProviderError> {
    option_env!("ANTIGRAVITY_CLIENT_ID")
        .map(str::to_string)
        .or_else(|| std::env::var("ANTIGRAVITY_CLIENT_ID").ok())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| ProviderError::Config(ANTIGRAVITY_NOT_CONFIGURED.into()))
}

fn antigravity_client_secret() -> Result<String, ProviderError> {
    option_env!("ANTIGRAVITY_CLIENT_SECRET")
        .map(str::to_string)
        .or_else(|| std::env::var("ANTIGRAVITY_CLIENT_SECRET").ok())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| ProviderError::Config(ANTIGRAVITY_NOT_CONFIGURED.into()))
}

fn antigravity_entry() -> Result<Entry, ProviderError> {
    Entry::new(ANTIGRAVITY_SERVICE, "antigravity")
        .map_err(|e| ProviderError::Oauth(format!("credential store unavailable: {e}")))
}

fn read_antigravity_tokens() -> Result<AntigravityTokens, ProviderError> {
    let raw = antigravity_entry()?.get_password().map_err(|_| {
        ProviderError::Oauth("Antigravity is not connected — use Connect in Settings".into())
    })?;
    serde_json::from_str(&raw)
        .map_err(|_| ProviderError::Oauth("Antigravity credentials could not be read".into()))
}

fn write_antigravity_tokens(tokens: &AntigravityTokens) -> Result<(), ProviderError> {
    let json = serde_json::to_string(tokens)
        .map_err(|_| ProviderError::Oauth("Antigravity credentials could not be saved".into()))?;
    antigravity_entry()?
        .set_password(&json)
        .map_err(|e| ProviderError::Oauth(format!("Antigravity credentials could not be saved: {e}")))
}

async fn antigravity_credential_async(
    client: &reqwest::Client,
    force_refresh: bool,
) -> Result<OAuthCredential, ProviderError> {
    let mut tokens = read_antigravity_tokens()?;
    let expired = tokens.expires_at <= Utc::now().timestamp_millis();
    if !force_refresh && !expired {
        return Ok(OAuthCredential {
            access_token: SecretString::from(tokens.access_token.clone()),
            account_id: None,
        });
    }

    let client_id = antigravity_client_id()?;
    let client_secret = antigravity_client_secret()?;
    let refreshed = google_token_request(
        client,
        &[
            ("client_id", &client_id),
            ("client_secret", &client_secret),
            ("refresh_token", &tokens.refresh_token),
            ("grant_type", "refresh_token"),
        ],
        "Antigravity token refresh failed; connect again in Settings",
    )
    .await?;

    tokens.access_token = refreshed.access_token;
    tokens.expires_at = Utc::now().timestamp_millis() + refreshed.expires_in.unwrap_or(3600) * 1000;
    if let Some(refresh_token) = refreshed.refresh_token {
        tokens.refresh_token = refresh_token;
    }
    write_antigravity_tokens(&tokens)?;

    Ok(OAuthCredential {
        access_token: SecretString::from(tokens.access_token),
        account_id: None,
    })
}

async fn google_token_request(
    client: &reqwest::Client,
    form: &[(&str, &str)],
    failure_message: &str,
) -> Result<GoogleTokenResponse, ProviderError> {
    let resp = client
        .post(ANTIGRAVITY_TOKEN_URL)
        .form(form)
        .send()
        .await
        .map_err(|_| ProviderError::Oauth(failure_message.to_string()))?;
    if !resp.status().is_success() {
        return Err(ProviderError::Oauth(failure_message.to_string()));
    }
    resp.json()
        .await
        .map_err(|_| ProviderError::Oauth("Google returned an unexpected sign-in response".into()))
}

/// Run the interactive PKCE sign-in: open the system browser, wait for the
/// loopback redirect, exchange the code, and store the resulting refresh
/// token. Returns once the login is usable — the caller (a Tauri command)
/// nudges the scheduler afterwards so the bar picks it up immediately.
pub async fn antigravity_login(client: &reqwest::Client) -> Result<(), ProviderError> {
    let client_id = antigravity_client_id()?;
    let client_secret = antigravity_client_secret()?;

    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);
    let state = random_url_safe(16);

    let code = run_loopback_login(&client_id, &challenge, &state).await?;

    let token = google_token_request(
        client,
        &[
            ("client_id", &client_id),
            ("client_secret", &client_secret),
            ("code", &code.code),
            ("code_verifier", &verifier),
            ("redirect_uri", &code.redirect_uri),
            ("grant_type", "authorization_code"),
        ],
        "Google rejected the sign-in; try Connect again",
    )
    .await?;

    let refresh_token = token.refresh_token.ok_or_else(|| {
        ProviderError::Oauth(
            "Google did not return a refresh token — remove Token Bar's access at \
             myaccount.google.com/permissions and try Connect again"
                .into(),
        )
    })?;

    write_antigravity_tokens(&AntigravityTokens {
        access_token: token.access_token,
        refresh_token,
        expires_at: Utc::now().timestamp_millis() + token.expires_in.unwrap_or(3600) * 1000,
    })
}

pub fn antigravity_logout() -> Result<(), ProviderError> {
    match antigravity_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(ProviderError::Oauth(format!("could not disconnect: {e}"))),
    }
}

struct LoopbackCode {
    code: String,
    redirect_uri: String,
}

/// Bind a local port, open the consent screen with it as the redirect target,
/// and block (off the async runtime) for the single callback request the
/// browser makes when the user finishes signing in.
async fn run_loopback_login(
    client_id: &str,
    challenge: &str,
    state: &str,
) -> Result<LoopbackCode, ProviderError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| ProviderError::Oauth(format!("could not open a local port for sign-in: {e}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| ProviderError::Oauth(format!("could not prepare the local port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| ProviderError::Oauth(e.to_string()))?
        .port();
    let redirect_uri = format!("http://localhost:{port}/oauth-callback");

    let auth_url = build_authorize_url(client_id, &redirect_uri, challenge, state);
    open_browser(&auth_url)
        .map_err(|e| ProviderError::Oauth(format!("could not open a browser: {e}")))?;

    let expected_state = state.to_string();
    let code = tokio::task::spawn_blocking(move || accept_callback(listener, &expected_state))
        .await
        .map_err(|_| ProviderError::Oauth("sign-in was interrupted".into()))??;

    Ok(LoopbackCode { code, redirect_uri })
}

fn build_authorize_url(client_id: &str, redirect_uri: &str, challenge: &str, state: &str) -> String {
    format!(
        "{ANTIGRAVITY_AUTH_URL}?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code\
         &scope={scope}&code_challenge={challenge}&code_challenge_method=S256\
         &access_type=offline&prompt=consent&state={state}",
        client_id = urlencode(client_id),
        redirect_uri = urlencode(redirect_uri),
        scope = urlencode(ANTIGRAVITY_SCOPES),
        challenge = urlencode(challenge),
        state = urlencode(state),
    )
}

/// Poll a non-blocking accept for up to [`LOGIN_TIMEOUT`], reply to the
/// browser with a page it can be closed from, and hand back the `code` query
/// parameter. Runs inside `spawn_blocking` — this is plain blocking I/O.
fn accept_callback(
    listener: std::net::TcpListener,
    expected_state: &str,
) -> Result<String, ProviderError> {
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request
                    .lines()
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("");
                let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
                let params = parse_query(query);

                let ok = params.contains_key("code");
                let body = if ok {
                    "<html><body>Signed in. You can close this tab and return to Token Bar.</body></html>"
                } else {
                    "<html><body>Sign-in did not complete. You can close this tab.</body></html>"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();

                if let Some(err) = params.get("error") {
                    return Err(ProviderError::Oauth(format!("Google sign-in was cancelled ({err})")));
                }
                let code = params
                    .get("code")
                    .cloned()
                    .ok_or_else(|| ProviderError::Oauth("Google did not return a sign-in code".into()))?;
                if params.get("state").map(String::as_str).unwrap_or("") != expected_state {
                    return Err(ProviderError::Oauth(
                        "sign-in response did not match this request".into(),
                    ));
                }
                return Ok(code);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(ProviderError::Oauth("sign-in timed out — try Connect again".into()));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(ProviderError::Oauth(format!("local sign-in listener failed: {e}"))),
        }
    }
}

fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}

fn pkce_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn random_url_safe(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn urldecode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(&input[i + 1..i + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?;
            let v = it.next().unwrap_or("");
            Some((urldecode(k), urldecode(v)))
        })
        .collect()
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

    /// RFC 7636 Appendix B's worked example — the one place we can check the
    /// PKCE math against a published answer instead of just our own code.
    #[test]
    fn pkce_challenge_matches_the_rfc_7636_test_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn pkce_verifier_is_url_safe_and_unique() {
        let a = pkce_verifier();
        let b = pkce_verifier();
        assert_ne!(a, b);
        assert!(a.len() >= 43, "{a}");
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn urlencode_round_trips_reserved_characters() {
        let raw = "a b+c/d?e=f&g";
        assert_eq!(urldecode(&urlencode(raw)), raw);
    }

    #[test]
    fn parse_query_decodes_a_code_with_slashes_and_a_state() {
        let params = parse_query("code=4%2F0Ab_c-d&state=xyz%3D%3D&scope=a%20b");
        assert_eq!(params.get("code").map(String::as_str), Some("4/0Ab_c-d"));
        assert_eq!(params.get("state").map(String::as_str), Some("xyz=="));
        assert_eq!(params.get("scope").map(String::as_str), Some("a b"));
    }

    #[test]
    fn authorize_url_asks_for_a_code_challenge_and_the_right_redirect() {
        let url = build_authorize_url(
            "test-client-id.apps.googleusercontent.com",
            "http://localhost:54321/oauth-callback",
            "chal",
            "st",
        );
        assert!(url.starts_with(ANTIGRAVITY_AUTH_URL));
        assert!(url.contains("code_challenge=chal"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A54321%2Foauth-callback"));
        assert!(url.contains(&format!(
            "client_id={}",
            urlencode("test-client-id.apps.googleusercontent.com")
        )));
    }

    #[test]
    fn antigravity_tokens_round_trip_through_json() {
        let tokens = AntigravityTokens {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            expires_at: 123,
        };
        let json = serde_json::to_string(&tokens).unwrap();
        let back: AntigravityTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_token, "at");
        assert_eq!(back.refresh_token, "rt");
        assert_eq!(back.expires_at, 123);
    }
}
