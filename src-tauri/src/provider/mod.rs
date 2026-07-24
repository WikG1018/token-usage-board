pub mod mimo;

use crate::credential::Credential;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 近 N 月每月用量，按时间正序，元素为 (月标签 "YYYY-MM", 当月 token 总量)。
pub type MonthlySeries = Vec<(String, u64)>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageData {
    pub tier: String,
    pub total_credits: u64,
    pub used_credits: u64,
    pub expire_at: i64,
    pub fetched_at: i64,
    /// 年度累计 token 用量（原始 token 计数，可选）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year_used: Option<u64>,
    /// 本月累计 token 用量（原始 token 计数，可选）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month_used: Option<u64>,
    /// 近 N 月每月 token 用量（可选，元素为 ("YYYY-MM", tokens)，正序）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly_usage: Option<MonthlySeries>,
}

impl UsageData {
    pub fn remaining(&self) -> u64 {
        self.total_credits.saturating_sub(self.used_credits)
    }

    pub fn percent_used(&self) -> f64 {
        if self.total_credits == 0 {
            return 0.0;
        }
        (self.used_credits as f64 / self.total_credits as f64) * 100.0
    }

    pub fn days_left(&self, now: i64) -> i64 {
        ((self.expire_at - now) as f64 / 86400.0).ceil().max(0.0) as i64
    }
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("not authenticated / credential expired")]
    Unauthorized,
    #[error("network error: {0}")]
    Network(String),
    #[error("failed to parse response: {0}")]
    Parse(String),
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    async fn fetch_usage(&self, cred: &Credential) -> Result<UsageData, ProviderError>;
}
