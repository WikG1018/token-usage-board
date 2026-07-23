pub mod mimo;

use crate::credential::Credential;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 近 N 日每日用量，按时间正序，元素为 (日期戳 00:00 UTC 秒, 当日用量)。
pub type DailySeries = Vec<(i64, u64)>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageData {
    pub tier: String,
    pub total_credits: u64,
    pub used_credits: u64,
    pub expire_at: i64,
    pub fetched_at: i64,
    /// 年度累计用量（可选，接口未返回则为 None）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year_used: Option<u64>,
    /// 本月累计用量（可选）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month_used: Option<u64>,
    /// 近 N 日每日用量（可选，元素为 (日期 00:00 UTC 秒, 当日用量)，正序）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_usage: Option<DailySeries>,
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
