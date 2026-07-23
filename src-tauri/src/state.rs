use crate::credential::{Credential, CredentialStore};
use crate::provider::mimo::MiMoProvider;
use crate::provider::{Provider, ProviderError, UsageData};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    LoggedOut,
    Fresh,
    Stale,
    Expired,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageState {
    pub status: Status,
    pub data: Option<UsageData>,
    pub message: Option<String>,
}

impl Default for UsageState {
    fn default() -> Self {
        Self {
            status: Status::LoggedOut,
            data: None,
            message: None,
        }
    }
}

struct Inner {
    state: UsageState,
    consecutive_failures: u32,
}

pub struct AppState {
    inner: RwLock<Inner>,
    provider: Arc<dyn Provider>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                state: UsageState::default(),
                consecutive_failures: 0,
            }),
            provider: Arc::new(MiMoProvider::new()),
        }
    }

    pub async fn snapshot(&self) -> UsageState {
        self.inner.read().await.state.clone()
    }

    fn load_credential(&self) -> Option<Credential> {
        match CredentialStore::get(self.provider.id()) {
            Ok(Some(c)) => Some(c),
            _ => None,
        }
    }

    pub async fn refresh_now(&self) -> anyhow::Result<()> {
        let cred = self
            .load_credential()
            .ok_or_else(|| anyhow::anyhow!("no credential"))?;

        match self.provider.fetch_usage(&cred).await {
            Ok(data) => {
                let mut g = self.inner.write().await;
                g.state = UsageState {
                    status: Status::Fresh,
                    data: Some(data),
                    message: None,
                };
                g.consecutive_failures = 0;
            }
            Err(ProviderError::Unauthorized) => {
                let mut g = self.inner.write().await;
                let prev = g.state.data.clone();
                g.state = UsageState {
                    status: Status::Expired,
                    data: prev,
                    message: Some("登录已过期，请重新连接".into()),
                };
            }
            Err(e) => {
                let mut g = self.inner.write().await;
                g.consecutive_failures += 1;
                let had_data = g.state.data.is_some();
                g.state = UsageState {
                    status: if had_data { Status::Stale } else { Status::Error },
                    data: g.state.data.clone(),
                    message: Some(e.to_string()),
                };
            }
        }
        Ok(())
    }

    pub async fn logout(&self) -> anyhow::Result<()> {
        CredentialStore::clear(self.provider.id())?;
        let mut g = self.inner.write().await;
        g.state = UsageState::default();
        g.consecutive_failures = 0;
        Ok(())
    }

    pub async fn on_credential_captured(&self, cred: Credential) -> anyhow::Result<()> {
        CredentialStore::save(self.provider.id(), &cred)?;
        self.refresh_now().await
    }

    pub async fn backoff_secs(&self) -> u64 {
        let fails = self.inner.read().await.consecutive_failures;
        match fails {
            0 => 600,
            1 => 60,
            2 => 300,
            _ => 900,
        }
    }
}
