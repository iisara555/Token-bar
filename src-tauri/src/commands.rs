//! The IPC surface.
//!
//! Note what is *not* here: there is no command that returns a stored API key.
//! The frontend can save one and clear one, and can read a fingerprint, but the
//! secret itself never crosses into the webview. That is the whole reason the
//! HTTP calls live in Rust.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalSize, Runtime, State};

use crate::config::{AuthMode, GlassPref};
use crate::providers::{provider_for, Caps, ProviderId, UsageSnapshot};
use crate::state::AppState;
use crate::windows::{floating, BAR, SETTINGS};

type CmdResult<T> = Result<T, String>;
const EVENT_CONFIG: &str = "config://changed";

fn announce_config<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app.emit(EVENT_CONFIG, ()) {
        log::warn!("could not emit config change: {error}");
    }
}

fn parse_id(raw: &str) -> CmdResult<ProviderId> {
    ProviderId::parse(raw).ok_or_else(|| format!("unknown provider {raw:?}"))
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: ProviderId,
    pub name: &'static str,
    pub enabled: bool,
    pub auth_mode: AuthMode,
    pub uses_oauth: bool,
    pub oauth_status: Option<crate::oauth::OAuthStatus>,
    pub needs_key: bool,
    /// Whether this adapter actually reads `manual_spend_usd`. Drives whether
    /// Settings offers the field at all.
    pub manual_entry: bool,
    /// Whether a credential is stored. Never the credential.
    pub has_key: bool,
    /// e.g. `sk-ant-admin01-…4f2a`. Safe to render.
    pub fingerprint: Option<String>,
    pub caps: Caps,
    pub required_options: Vec<&'static str>,
    pub options: std::collections::BTreeMap<String, String>,
    pub poll_seconds: u64,
    /// Fixed, provider-owned page opened by the Settings Link button. URLs
    /// never come from the webview, so this command cannot be used as a shell
    /// launcher for arbitrary input.
    pub link_url: Option<&'static str>,
    pub link_label: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    pub glass: GlassPref,
    pub glass_mode: crate::vibrancy::GlassMode,
    pub window_days: i64,
    pub warn_at: f64,
    pub hotkey: String,
    pub click_through: bool,
    pub compact: bool,
    /// macOS only. Always reported so Settings can show the current state.
    pub allow_in_notch: bool,
    pub providers: Vec<ProviderView>,
    pub snapshots: Vec<UsageSnapshot>,
    pub version: String,
}

#[tauri::command]
pub fn get_app_view<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<AppView> {
    let cfg = state.config.get();

    let providers = ProviderId::ALL
        .into_iter()
        .map(|id| {
            let p = provider_for(id);
            let pc = cfg.provider(id);
            let fingerprint = crate::secrets::stored_fingerprint(id);
            let oauth_status = crate::oauth::supports(id).then(|| crate::oauth::status(id));
            ProviderView {
                id,
                name: id.display_name(),
                enabled: pc.enabled,
                auth_mode: pc.auth_mode,
                uses_oauth: crate::oauth::should_use(id, pc.auth_mode),
                oauth_status,
                needs_key: p.needs_key(),
                manual_entry: p.manual_entry(),
                has_key: fingerprint.is_some(),
                fingerprint,
                caps: p.caps(),
                required_options: p.required_options().to_vec(),
                options: pc.options,
                poll_seconds: p.poll_interval().as_secs(),
                link_url: provider_link(id).map(|(url, _)| url),
                link_label: provider_link(id).map(|(_, label)| label),
            }
        })
        .collect();

    let snapshots = state.store.all_latest().map_err(|e| e.to_string())?;

    Ok(AppView {
        glass: cfg.glass,
        glass_mode: crate::current_glass_mode(&app),
        window_days: cfg.window_days,
        warn_at: cfg.warn_at,
        hotkey: cfg.hotkey.clone(),
        click_through: cfg.bar.click_through,
        compact: cfg.bar.compact,
        allow_in_notch: cfg.bar.allow_in_notch,
        providers,
        snapshots,
        version: app.package_info().version.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_glass<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    glass: GlassPref,
) -> CmdResult<()> {
    state
        .config
        .update(|c| c.glass = glass)
        .map_err(|e| e.to_string())?;
    crate::reapply_glass(&app);
    announce_config(&app);
    Ok(())
}

#[tauri::command]
pub fn set_window_days<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    days: i64,
) -> CmdResult<()> {
    let days = days.clamp(1, 30);
    state
        .config
        .update(|c| c.window_days = days)
        .map_err(|e| e.to_string())?;
    state.nudge_all();
    announce_config(&app);
    Ok(())
}

/// Where the bar starts warning, as a fraction of a budget or quota window.
///
/// Clamped rather than validated: the useful range is "well before the ceiling"
/// to "almost at it", and a threshold of 0 would paint the rail amber at rest
/// while 1.0 would make it indistinguishable from being over.
#[tauri::command]
pub fn set_warn_at<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    fraction: f64,
) -> CmdResult<()> {
    if !fraction.is_finite() {
        return Err("warning threshold must be a number".into());
    }
    let fraction = fraction.clamp(0.5, 0.95);
    state
        .config
        .update(|c| c.warn_at = fraction)
        .map_err(|e| e.to_string())?;
    crate::refresh_tray(&app);
    announce_config(&app);
    Ok(())
}

/// Rebind the show/hide accelerator.
///
/// The new binding is proved against the OS before it is written down: an
/// accelerator already taken by another app fails here, and the user keeps the
/// shortcut that was working rather than silently losing both.
#[tauri::command]
pub fn set_hotkey<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    accelerator: String,
) -> CmdResult<()> {
    let accelerator = accelerator.trim().to_string();
    if accelerator.is_empty() {
        return Err("shortcut is empty".into());
    }

    let previous = state.config.get().hotkey;
    if accelerator == previous {
        return Ok(());
    }

    if let Err(e) = crate::register_hotkey(&app, &accelerator, Some(previous.as_str())) {
        // Put the working shortcut back before reporting the failure.
        let _ = crate::register_hotkey(&app, &previous, None);
        return Err(format!(
            "{accelerator} could not be registered — another app may already use it ({e})"
        ));
    }

    state
        .config
        .update(|c| c.hotkey = accelerator)
        .map_err(|e| e.to_string())?;
    announce_config(&app);
    Ok(())
}

#[tauri::command]
pub fn set_provider_enabled<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    provider: String,
    enabled: bool,
) -> CmdResult<()> {
    let id = parse_id(&provider)?;
    state
        .config
        .update(|c| c.providers.entry(id).or_default().enabled = enabled)
        .map_err(|e| e.to_string())?;
    state.nudge(id);
    announce_config(&app);
    Ok(())
}

#[tauri::command]
pub fn set_provider_auth_mode<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    provider: String,
    auth_mode: AuthMode,
) -> CmdResult<()> {
    let id = parse_id(&provider)?;
    if !crate::oauth::supports(id) && auth_mode == AuthMode::Oauth {
        return Err(
            "OAuth usage is currently available for Anthropic, OpenAI and Kimi Code".into(),
        );
    }
    state
        .config
        .update(|c| c.providers.entry(id).or_default().auth_mode = auth_mode)
        .map_err(|e| e.to_string())?;
    state.nudge(id);
    announce_config(&app);
    Ok(())
}

fn provider_link(id: ProviderId) -> Option<(&'static str, &'static str)> {
    Some(match id {
        ProviderId::Anthropic => ("https://claude.ai/settings", "Link Claude.ai"),
        ProviderId::Openai => ("https://chatgpt.com/", "Link ChatGPT"),
        ProviderId::Kimi => ("https://www.kimi.com/code", "Link Kimi"),
        ProviderId::Zai => ("https://z.ai/subscribe", "Open Z.AI"),
        ProviderId::Minimax => (
            "https://platform.minimaxi.com/subscribe/token-plan",
            "Open MiniMax",
        ),
        ProviderId::Openrouter => ("https://openrouter.ai/settings/keys", "Open OpenRouter"),
        ProviderId::Xai => ("https://console.x.ai/", "Open xAI"),
        ProviderId::Deepseek => ("https://platform.deepseek.com/api_keys", "Open DeepSeek"),
        ProviderId::Gemini => ("https://aistudio.google.com/app/apikey", "Open Google AI"),
        ProviderId::Groq => ("https://console.groq.com/keys", "Open Groq"),
        ProviderId::Mistral => ("https://console.mistral.ai/api-keys/", "Open Mistral"),
    })
}

#[tauri::command]
pub fn set_provider_option<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    provider: String,
    key: String,
    value: String,
) -> CmdResult<()> {
    let id = parse_id(&provider)?;
    // Options are identifiers and small numbers — team ids, budgets. Anything
    // beyond that is a mistake or a paste, and config.json is rewritten in full
    // on every change, so there is no reason to carry it.
    if key.len() > MAX_OPTION_KEY_LEN || value.len() > MAX_OPTION_VALUE_LEN {
        return Err("that value is too long for a settings field".into());
    }
    // Guard against a credential being pasted into a plain-text option field,
    // where it would land in config.json instead of the credential store.
    if looks_like_a_key(&value) {
        return Err(
            "that looks like an API key — use the key field so it goes to the credential store"
                .into(),
        );
    }
    state
        .config
        .update(|c| {
            let entry = c.providers.entry(id).or_default();
            if value.trim().is_empty() {
                entry.options.remove(&key);
            } else {
                entry.options.insert(key, value.trim().to_string());
            }
        })
        .map_err(|e| e.to_string())?;
    state.nudge(id);
    announce_config(&app);
    Ok(())
}

const MAX_OPTION_KEY_LEN: usize = 64;
const MAX_OPTION_VALUE_LEN: usize = 256;

fn looks_like_a_key(v: &str) -> bool {
    let v = v.trim();
    v.len() >= 20
        && ["sk-", "xai-", "gsk_", "or-v1-", "AIza"]
            .iter()
            .any(|p| v.starts_with(p))
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn save_provider_key<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    provider: String,
    key: String,
) -> CmdResult<String> {
    let id = parse_id(&provider)?;
    let key = key.trim();
    if key.is_empty() {
        return Err("key is empty".into());
    }
    // Derive the label *before* storing. Anything that can go wrong with the
    // key's own text should go wrong while the credential store is still
    // untouched, rather than leaving a stored key the app then trips over on
    // every launch.
    let fingerprint = crate::secrets::fingerprint(key);
    crate::secrets::set(id, key).map_err(|e| e.to_string())?;
    // A new key deserves an immediate retry even if the old one was rejected.
    state.nudge(id);
    announce_config(&app);
    Ok(fingerprint)
}

#[tauri::command]
pub fn clear_provider_key<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    provider: String,
) -> CmdResult<()> {
    let id = parse_id(&provider)?;
    crate::secrets::delete(id).map_err(|e| e.to_string())?;
    state.nudge(id);
    announce_config(&app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn refresh(state: State<'_, AppState>, provider: Option<String>) -> CmdResult<()> {
    match provider {
        Some(raw) => state.nudge(parse_id(&raw)?),
        None => state.nudge_all(),
    }
    Ok(())
}

#[tauri::command]
pub fn get_snapshots(state: State<'_, AppState>) -> CmdResult<Vec<UsageSnapshot>> {
    state.store.all_latest().map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Window control
// ---------------------------------------------------------------------------

/// The bar resizes itself to fit however many providers are enabled, so the
/// pill never carries dead space.
#[tauri::command]
pub fn bar_set_size<R: Runtime>(app: AppHandle<R>, width: u32, height: u32) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window(BAR) {
        let scale = w.scale_factor().unwrap_or(1.0);
        let physical = PhysicalSize::new(
            (width as f64 * scale).round() as u32,
            (height as f64 * scale).round() as u32,
        );
        w.set_size(physical).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn bar_dropped<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window(BAR) {
        floating::snap_to_edge(&w);
    }
    Ok(())
}

#[tauri::command]
pub fn set_click_through<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    on: bool,
) -> CmdResult<()> {
    state
        .config
        .update(|c| c.bar.click_through = on)
        .map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window(BAR) {
        floating::set_click_through(&w, on);
    }
    announce_config(&app);
    Ok(())
}

#[tauri::command]
pub fn set_compact<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    on: bool,
) -> CmdResult<()> {
    state
        .config
        .update(|c| c.bar.compact = on)
        .map_err(|e| e.to_string())?;
    announce_config(&app);
    Ok(())
}

/// macOS only: allow the bar into the menu bar strip beside the camera housing.
///
/// Re-runs placement immediately, so turning it off pulls a bar that is up there
/// back down rather than leaving it stranded over the menu bar.
#[tauri::command]
pub fn set_allow_in_notch<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    on: bool,
) -> CmdResult<()> {
    state
        .config
        .update(|c| c.bar.allow_in_notch = on)
        .map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window(BAR) {
        floating::reapply_placement(&w);
    }
    announce_config(&app);
    Ok(())
}

#[tauri::command]
pub fn open_settings<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        w.show().map_err(|e| e.to_string())?;
        let _ = w.set_focus();
    }
    Ok(())
}

/// Open a provider-owned sign-in, usage or API-key page. The URL is selected
/// solely from the parsed provider id; callers cannot supply a shell URL.
#[tauri::command]
pub fn open_provider_link(provider: String) -> CmdResult<()> {
    let id = parse_id(&provider)?;
    let (url, _) = provider_link(id).ok_or_else(|| "provider has no link".to_string())?;

    #[cfg(target_os = "windows")]
    {
        // Prefer Chrome because the Link action is intended to reach the
        // user's existing Claude.ai/ChatGPT session. Fall back to the Windows
        // default browser when Chrome is not installed.
        let chrome_candidates = [
            std::env::var_os("LOCALAPPDATA").map(|root| {
                std::path::PathBuf::from(root).join("Google/Chrome/Application/chrome.exe")
            }),
            std::env::var_os("PROGRAMFILES").map(|root| {
                std::path::PathBuf::from(root).join("Google/Chrome/Application/chrome.exe")
            }),
            std::env::var_os("PROGRAMFILES(X86)").map(|root| {
                std::path::PathBuf::from(root).join("Google/Chrome/Application/chrome.exe")
            }),
        ];
        if let Some(chrome) = chrome_candidates
            .into_iter()
            .flatten()
            .find(|path| path.is_file())
        {
            std::process::Command::new(chrome)
                .args(["--new-tab", url])
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            std::process::Command::new("cmd")
                .args(["/c", "start", "", url])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn hide_window<R: Runtime>(app: AppHandle<R>, label: String) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window(&label) {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn quit<R: Runtime>(app: AppHandle<R>) {
    crate::windows::appbar::release_all();
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_pasted_key_in_a_plain_option_field() {
        assert!(looks_like_a_key("sk-ant-admin01-AbCdEfGhIjKlMnOpQrSt"));
        assert!(looks_like_a_key("xai-0123456789abcdefghijklmn"));
    }

    #[test]
    fn ordinary_option_values_are_allowed() {
        assert!(!looks_like_a_key("team_01HZX9QK7M"));
        assert!(!looks_like_a_key("25.00"));
        assert!(!looks_like_a_key("sk-short"));
    }

    #[test]
    fn unknown_provider_is_an_error_not_a_panic() {
        assert!(parse_id("not-a-provider").is_err());
        assert_eq!(parse_id("anthropic").unwrap(), ProviderId::Anthropic);
    }
}
