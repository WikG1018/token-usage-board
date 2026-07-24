use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    /// API 基础地址，如 `https://platform.xiaomimimo.com`
    pub base_url: String,
    pub cookies: Vec<(String, String)>,
    pub extra_headers: Vec<(String, String)>,
    pub obtained_at: i64,
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("keyring error: {0}")]
    Keyring(String),
    #[error("serde error: {0}")]
    Serde(String),
}

const SERVICE: &str = "token-usage-board";

fn entry(provider_id: &str) -> Result<keyring::Entry, CredentialError> {
    keyring::Entry::new(SERVICE, provider_id).map_err(|e| CredentialError::Keyring(e.to_string()))
}

pub struct CredentialStore;

impl CredentialStore {
    pub fn save(provider_id: &str, cred: &Credential) -> Result<(), CredentialError> {
        let json =
            serde_json::to_string(cred).map_err(|e| CredentialError::Serde(e.to_string()))?;
        entry(provider_id)?
            .set_password(&json)
            .map_err(|e| CredentialError::Keyring(e.to_string()))
    }

    pub fn get(provider_id: &str) -> Result<Option<Credential>, CredentialError> {
        let entry = entry(provider_id)?;
        match entry.get_password() {
            Ok(json) => {
                let cred = serde_json::from_str(&json)
                    .map_err(|e| CredentialError::Serde(e.to_string()))?;
                Ok(Some(cred))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CredentialError::Keyring(e.to_string())),
        }
    }

    pub fn clear(provider_id: &str) -> Result<(), CredentialError> {
        match entry(provider_id)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CredentialError::Keyring(e.to_string())),
        }
    }

    pub fn is_present(provider_id: &str) -> bool {
        matches!(Self::get(provider_id), Ok(Some(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        format!("mimo_test_{}", SEQ.fetch_add(1, Ordering::SeqCst))
    }

    #[test]
    fn roundtrip() {
        let id = unique_id();
        let cred = Credential {
            base_url: "https://example.com".into(),
            cookies: vec![("session".into(), "abc123".into())],
            extra_headers: vec![("x-req".into(), "1".into())],
            obtained_at: 1_800_000_000,
        };
        CredentialStore::save(&id, &cred).expect("save");
        let loaded = CredentialStore::get(&id).expect("get").expect("some");
        assert_eq!(loaded.base_url, cred.base_url);
        assert_eq!(loaded.cookies, cred.cookies);
        assert_eq!(loaded.extra_headers, cred.extra_headers);
        assert!(CredentialStore::is_present(&id));
        CredentialStore::clear(&id).expect("clear");
        assert!(!CredentialStore::is_present(&id));
    }
}
