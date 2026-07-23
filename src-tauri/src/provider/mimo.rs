use super::{Provider, ProviderError, UsageData};
use crate::credential::Credential;
use async_trait::async_trait;
use serde::Deserialize;

pub struct MiMoProvider;

impl MiMoProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MiMoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    data: Option<ApiData>,
}

#[derive(Debug, Deserialize)]
struct ApiData {
    #[serde(default, alias = "plan_name", alias = "tier")]
    plan: Option<String>,
    #[serde(default, alias = "total_credits", alias = "total")]
    total_credits: Option<u64>,
    #[serde(default, alias = "used_credits", alias = "used")]
    used_credits: Option<u64>,
    #[serde(default, alias = "expire_time", alias = "expires_at", alias = "end_time")]
    expire_at: Option<i64>,
}

pub fn parse_usage(body: &str, fetched_at: i64) -> Result<UsageData, ProviderError> {
    let resp: ApiResponse =
        serde_json::from_str(body).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let d = resp
        .data
        .ok_or_else(|| ProviderError::Parse("missing data field".into()))?;

    let tier = d
        .plan
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProviderError::Parse("missing tier".into()))?;
    let total = d
        .total_credits
        .ok_or_else(|| ProviderError::Parse("missing total_credits".into()))?;
    let used = d
        .used_credits
        .ok_or_else(|| ProviderError::Parse("missing used_credits".into()))?;
    let expire = d
        .expire_at
        .ok_or_else(|| ProviderError::Parse("missing expire_at".into()))?;

    Ok(UsageData {
        tier,
        total_credits: total,
        used_credits: used,
        expire_at: expire,
        fetched_at,
    })
}

#[async_trait]
impl Provider for MiMoProvider {
    fn id(&self) -> &'static str {
        "mimo"
    }

    fn display_name(&self) -> &'static str {
        "Xiaomi MiMo"
    }

    async fn fetch_usage(&self, cred: &Credential) -> Result<UsageData, ProviderError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let mut req = client.get(&cred.endpoint);
        for (k, v) in &cred.extra_headers {
            req = req.header(k, v);
        }
        let cookie_header = cred
            .cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        if !cookie_header.is_empty() {
            req = req.header(reqwest::header::COOKIE, cookie_header);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::Unauthorized);
        }
        if !status.is_success() {
            return Err(ProviderError::Network(format!("HTTP {status}")));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let now = chrono::Utc::now().timestamp();
        parse_usage(&body, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/mimo_usage.json");

    #[test]
    fn parses_fixture() {
        let now = 1_800_000_000;
        let u = parse_usage(FIXTURE, now).expect("should parse");
        assert_eq!(u.tier, "Standard");
        assert_eq!(u.total_credits, 1_000_000);
        assert_eq!(u.used_credits, 250_000);
        assert_eq!(u.remaining(), 750_000);
        assert!((u.percent_used() - 25.0).abs() < 0.001);
        assert_eq!(u.fetched_at, now);
        assert!(u.expire_at > now || u.expire_at <= now);
    }

    #[test]
    fn missing_data_errors() {
        let body = r#"{"code":0}"#;
        assert!(parse_usage(body, 0).is_err());
    }

    #[test]
    fn supports_field_aliases() {
        let body = r#"{"data":{"plan_name":"Pro","total":100,"used":10,"end_time":1800000000}}"#;
        let u = parse_usage(body, 0).expect("aliases should parse");
        assert_eq!(u.tier, "Pro");
        assert_eq!(u.total_credits, 100);
        assert_eq!(u.used_credits, 10);
        assert_eq!(u.expire_at, 1_800_000_000);
    }
}
