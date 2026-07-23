use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub endpoint: String,
    pub cookies: Vec<(String, String)>,
    pub extra_headers: Vec<(String, String)>,
    pub obtained_at: i64,
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("io error: {0}")]
    Io(String),
    #[error("protect/unprotect failed: {0}")]
    Crypto(String),
    #[error("serde error: {0}")]
    Serde(String),
}

fn store_path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("token-usage-board")
}

fn file_for(provider_id: &str) -> PathBuf {
    store_path().join(format!("credential-{provider_id}.bin"))
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Result<Vec<u8>, CredentialError> {
    use std::ptr;
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    unsafe {
        CryptProtectData(&mut input, None, None, None, None, 0, &mut output)
            .map_err(|e| CredentialError::Crypto(e.to_string()))?;
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let result = slice.to_vec();
        let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            output.pbData as *mut _,
        )));
        Ok(result)
    }
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Result<Vec<u8>, CredentialError> {
    use std::ptr;
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    unsafe {
        CryptUnprotectData(&mut input, None, None, None, None, 0, &mut output)
            .map_err(|e| CredentialError::Crypto(e.to_string()))?;
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let result = slice.to_vec();
        let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            output.pbData as *mut _,
        )));
        Ok(result)
    }
}

#[cfg(not(windows))]
fn protect(data: &[u8]) -> Result<Vec<u8>, CredentialError> {
    Ok(data.to_vec())
}

#[cfg(not(windows))]
fn unprotect(data: &[u8]) -> Result<Vec<u8>, CredentialError> {
    Ok(data.to_vec())
}

pub struct CredentialStore;

impl CredentialStore {
    pub fn save(provider_id: &str, cred: &Credential) -> Result<(), CredentialError> {
        std::fs::create_dir_all(store_path()).map_err(|e| CredentialError::Io(e.to_string()))?;
        let json =
            serde_json::to_vec(cred).map_err(|e| CredentialError::Serde(e.to_string()))?;
        let blob = protect(&json)?;
        std::fs::write(file_for(provider_id), blob).map_err(|e| CredentialError::Io(e.to_string()))
    }

    pub fn get(provider_id: &str) -> Result<Option<Credential>, CredentialError> {
        let path = file_for(provider_id);
        if !path.exists() {
            return Ok(None);
        }
        let blob = std::fs::read(&path).map_err(|e| CredentialError::Io(e.to_string()))?;
        let json = unprotect(&blob)?;
        let cred =
            serde_json::from_slice(&json).map_err(|e| CredentialError::Serde(e.to_string()))?;
        Ok(Some(cred))
    }

    pub fn clear(provider_id: &str) -> Result<(), CredentialError> {
        let path = file_for(provider_id);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| CredentialError::Io(e.to_string()))?;
        }
        Ok(())
    }

    pub fn is_present(provider_id: &str) -> bool {
        file_for(provider_id).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let id = "mimo_test";
        let cred = Credential {
            endpoint: "https://example.com/api/usage".into(),
            cookies: vec![("session".into(), "abc123".into())],
            extra_headers: vec![("x-req".into(), "1".into())],
            obtained_at: 1_800_000_000,
        };
        CredentialStore::save(id, &cred).expect("save");
        let loaded = CredentialStore::get(id).expect("get").expect("some");
        assert_eq!(loaded.endpoint, cred.endpoint);
        assert_eq!(loaded.cookies, cred.cookies);
        assert_eq!(loaded.extra_headers, cred.extra_headers);
        assert!(CredentialStore::is_present(id));
        CredentialStore::clear(id).expect("clear");
        assert!(!CredentialStore::is_present(id));
    }
}
