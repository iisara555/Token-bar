//! Credential storage.
//!
//! Provider admin keys grant organisation-wide billing read. They live in the
//! platform credential store and nowhere else: not in config.json, not in the
//! SQLite cache, not in localStorage, and never across the IPC boundary into
//! the webview.
//!
//! The store is chosen at compile time by the `keyring` features in Cargo.toml:
//! the Windows Credential Manager (DPAPI-backed) on Windows, the login Keychain
//! (Security framework) on macOS. Both encrypt at rest and scope the entry to
//! the logged-in user, and in both cases it is the OS, not this app, that
//! decides when to prompt for access.
//!
//! The UI can ask for a *fingerprint* — enough to tell two keys apart, not
//! enough to use one.

use keyring::Entry;
use secrecy::SecretString;

use crate::providers::ProviderId;

const SERVICE: &str = "com.tokenbar.app";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("credential store unavailable: {0}")]
    Store(String),
}

fn entry(id: ProviderId) -> Result<Entry, SecretError> {
    Entry::new(SERVICE, id.as_str()).map_err(|e| SecretError::Store(e.to_string()))
}

pub fn set(id: ProviderId, key: &str) -> Result<(), SecretError> {
    entry(id)?
        .set_password(key.trim())
        .map_err(|e| SecretError::Store(e.to_string()))
}

pub fn get(id: ProviderId) -> Result<Option<SecretString>, SecretError> {
    match entry(id)?.get_password() {
        Ok(pw) => Ok(Some(SecretString::from(pw))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretError::Store(e.to_string())),
    }
}

pub fn delete(id: ProviderId) -> Result<(), SecretError> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretError::Store(e.to_string())),
    }
}

/// A safe, stable label for a stored key: keeps the recognisable prefix and the
/// last four characters, drops everything in between. This is what the settings
/// window displays.
///
/// Everything here counts *characters*, not bytes. A key is normally ASCII, but
/// this runs on whatever the user pasted, and slicing a stray multi-byte
/// character down the middle is a panic — which, with `panic = "abort"`, takes
/// the whole app with it every time the settings window is opened.
pub fn fingerprint(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().collect();
    if chars.len() <= 10 {
        return "•".repeat(chars.len().max(4));
    }
    // Keep everything up to the last `-` or `_` inside the first 16 characters,
    // so `sk-ant-admin01-` survives as a recognisable prefix.
    let prefix_len = chars
        .iter()
        .take(16)
        .enumerate()
        .filter(|(_, c)| **c == '-' || **c == '_')
        .map(|(i, _)| i + 1)
        .next_back()
        .unwrap_or(3);
    let prefix: String = chars[..prefix_len].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}…{tail}")
}

/// Fingerprint of whatever is currently stored, if anything.
pub fn stored_fingerprint(id: ProviderId) -> Option<String> {
    use secrecy::ExposeSecret;
    get(id)
        .ok()
        .flatten()
        .map(|s| fingerprint(s.expose_secret()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_keeps_the_prefix_and_the_tail_only() {
        let fp = fingerprint("sk-ant-admin01-AbCdEfGhIjKlMnOpQrSt4f2a");
        assert!(fp.starts_with("sk-ant-admin01-"), "{fp}");
        assert!(fp.ends_with("4f2a"), "{fp}");
        assert!(!fp.contains("AbCdEfGhIjKl"), "{fp}");
    }

    #[test]
    fn fingerprint_handles_a_prefixless_key() {
        let fp = fingerprint("0123456789abcdef");
        assert!(fp.ends_with("cdef"));
        assert!(!fp.contains("456789ab"));
    }

    #[test]
    fn short_input_is_fully_masked() {
        assert_eq!(fingerprint("abc"), "••••");
        assert!(!fingerprint("abcdefghij").contains('a'));
    }

    /// A key is meant to be ASCII, but this is fed whatever was pasted into the
    /// field. Slicing on a byte index used to abort the process here — and
    /// because the key is stored before it is fingerprinted, it aborted again on
    /// every subsequent launch.
    #[test]
    fn a_multibyte_key_is_masked_rather_than_panicking() {
        let secret = "sk-ant-admin01-ทดสอบรหัสผ่านลับ";
        let fp = fingerprint(secret);
        assert!(fp.starts_with("sk-ant-admin01-"), "{fp}");
        assert!(!fp.contains("ทดสอบ"), "raw key survived: {fp}");

        // A prefixless multi-byte key exercises the other slice boundary.
        assert!(fingerprint("パスワードパスワードパスワード").contains('…'));
    }

    #[test]
    fn short_multibyte_input_is_masked_by_character_count() {
        // Three characters, nine bytes. The mask counts what the user sees.
        assert_eq!(fingerprint("パスワ"), "••••");
    }
}
