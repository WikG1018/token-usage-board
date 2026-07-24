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

// ===== /api/v1/tokenPlan/detail =====
#[derive(Debug, Deserialize)]
struct DetailResp {
    #[serde(default)]
    data: Option<DetailData>,
}
#[derive(Debug, Deserialize)]
struct DetailData {
    #[serde(default, alias = "planName")]
    plan_name: Option<String>,
    /// "2026-07-26 23:59:59"（北京时间）
    #[serde(default, alias = "currentPeriodEnd", alias = "expire_time")]
    current_period_end: Option<String>,
}

// ===== /api/v1/tokenPlan/usage =====
#[derive(Debug, Deserialize)]
struct UsageResp {
    #[serde(default)]
    data: Option<UsageData_>,
}
#[derive(Debug, Deserialize)]
struct UsageData_ {
    #[serde(default)]
    usage: Option<Bucket>,
    #[serde(default, alias = "monthUsage")]
    month_usage: Option<Bucket>,
}
#[derive(Debug, Deserialize)]
struct Bucket {
    #[serde(default)]
    items: Vec<Item>,
}
#[derive(Debug, Deserialize)]
struct Item {
    #[serde(default)]
    used: Option<u64>,
    #[serde(default)]
    limit: Option<u64>,
}

// ===== /api/v1/usage/token-plan/list =====
#[derive(Debug, Deserialize)]
struct ListResp {
    #[serde(default)]
    data: Option<Vec<ListEntry>>,
}
#[derive(Debug, Deserialize)]
struct ListEntry {
    /// "2026-07"
    #[serde(default)]
    date: Option<String>,
    #[serde(default, alias = "totalToken", alias = "total_token")]
    total_token: Option<u64>,
}

/// 解析北京时间字符串 "2026-07-26 23:59:59" 为 unix 秒（按 UTC+8）。
fn parse_beijing_datetime(s: &str) -> Option<i64> {
    use chrono::NaiveDateTime;
    let dt = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(dt.and_utc().timestamp() - 8 * 3600)
}

/// 从 list 接口返回体解析：年度总和、当月用量、近 6 月序列。
/// `current_month` 形如 "2026-07"。
fn parse_yearly(
    body: &str,
    current_year: i32,
    current_month: &str,
) -> Option<(u64, u64, Vec<(String, u64)>)> {
    let resp: ListResp = serde_json::from_str(body).ok()?;
    let entries = resp.data?;
    if entries.is_empty() {
        return None;
    }

    // 按月聚合
    use std::collections::BTreeMap;
    let mut by_month: BTreeMap<String, u64> = BTreeMap::new();
    for e in entries {
        let Some(date) = e.date.as_deref() else {
            continue;
        };
        let Some(tokens) = e.total_token else {
            continue;
        };
        *by_month.entry(date.to_string()).or_insert(0) += tokens;
    }

    // 年度总和：仅当年
    let year_prefix = format!("{current_year}-");
    let year_used: u64 = by_month
        .iter()
        .filter(|(m, _)| m.starts_with(&year_prefix))
        .map(|(_, v)| *v)
        .sum();

    // 当月用量
    let month_used = by_month.get(current_month).copied().unwrap_or(0);

    // 近 6 月序列（正序）
    let monthly_usage: Vec<(String, u64)> = by_month.into_iter().rev().take(6).collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    Some((year_used, month_used, monthly_usage))
}

/// 合并 detail + usage 两个 GET 接口的解析（核心字段，必需）。
fn parse_core(detail_body: &str, usage_body: &str, fetched_at: i64) -> Result<UsageData, ProviderError> {
    let detail: DetailResp = serde_json::from_str(detail_body)
        .map_err(|e| ProviderError::Parse(format!("detail: {e}")))?;
    let d = detail
        .data
        .ok_or_else(|| ProviderError::Parse("detail: missing data".into()))?;
    let tier = d
        .plan_name
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProviderError::Parse("detail: missing planName".into()))?;
    let expire_at = d
        .current_period_end
        .as_deref()
        .and_then(parse_beijing_datetime)
        .ok_or_else(|| ProviderError::Parse("detail: missing/invalid currentPeriodEnd".into()))?;

    let usage: UsageResp = serde_json::from_str(usage_body)
        .map_err(|e| ProviderError::Parse(format!("usage: {e}")))?;
    let u = usage
        .data
        .ok_or_else(|| ProviderError::Parse("usage: missing data".into()))?;
    let plan_bucket = u
        .usage
        .ok_or_else(|| ProviderError::Parse("usage: missing usage bucket".into()))?;
    let plan_item = plan_bucket
        .items
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::Parse("usage: empty usage items".into()))?;
    let total_credits = plan_item
        .limit
        .ok_or_else(|| ProviderError::Parse("usage: missing limit".into()))?;
    let used_credits = plan_item
        .used
        .ok_or_else(|| ProviderError::Parse("usage: missing used".into()))?;

    Ok(UsageData {
        tier,
        total_credits,
        used_credits,
        expire_at,
        fetched_at,
        year_used: None,
        month_used: None,
        monthly_usage: None,
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

        let base = cred.base_url.trim_end_matches('/');
        let detail_url = format!("{base}/api/v1/tokenPlan/detail");
        let usage_url = format!("{base}/api/v1/tokenPlan/usage");
        let list_url = format!("{base}/api/v1/usage/token-plan/list");

        // 构造带 cookie + headers 的请求
        let cookie_header = cred
            .cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        let build_req = |method: reqwest::Method, url: &str| -> reqwest::RequestBuilder {
            let mut r = client.request(method, url);
            if !cookie_header.is_empty() {
                r = r.header(reqwest::header::COOKIE, &cookie_header);
            }
            for (k, v) in &cred.extra_headers {
                r = r.header(k, v);
            }
            r
        };

        // 并行拉取 detail + usage（核心，必需）+ list（可选，失败不阻断）
        let now_chrono = chrono::Utc::now();
        let now_ts = now_chrono.timestamp();
        let year = now_chrono.format("%Y").to_string().parse::<i32>().unwrap_or(0);
        let current_month = now_chrono.format("%Y-%m").to_string();
        let list_body_json = serde_json::json!({ "year": year });

        let detail_fut = build_req(reqwest::Method::GET, &detail_url).send();
        let usage_fut = build_req(reqwest::Method::GET, &usage_url).send();
        let list_fut = build_req(reqwest::Method::POST, &list_url)
            .header("content-type", "application/json")
            .json(&list_body_json)
            .send();

        let (detail_resp, usage_resp, list_resp) =
            tokio::join!(detail_fut, usage_fut, list_fut);

        // 检查核心接口认证状态
        let check_auth = |status: reqwest::StatusCode| -> Result<(), ProviderError> {
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                Err(ProviderError::Unauthorized)
            } else if !status.is_success() {
                Err(ProviderError::Network(format!("HTTP {status}")))
            } else {
                Ok(())
            }
        };

        let detail_status = detail_resp.as_ref().map(|r| r.status()).unwrap_or_else(|_| reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        check_auth(detail_status)?;
        let usage_status = usage_resp.as_ref().map(|r| r.status()).unwrap_or_else(|_| reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        check_auth(usage_status)?;

        let detail_body = detail_resp
            .map_err(|e| ProviderError::Network(e.to_string()))?
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let usage_body = usage_resp
            .map_err(|e| ProviderError::Network(e.to_string()))?
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let mut data = parse_core(&detail_body, &usage_body, now_ts)?;

        // list 接口可选：失败则 year/month/monthly 留空
        if let Ok(resp) = list_resp {
            if resp.status().is_success() {
                if let Ok(body) = resp.text().await {
                    if let Some((year_used, month_used, monthly_usage)) =
                        parse_yearly(&body, year, &current_month)
                    {
                        data.year_used = Some(year_used);
                        data.month_used = Some(month_used);
                        if !monthly_usage.is_empty() {
                            data.monthly_usage = Some(monthly_usage);
                        }
                    }
                }
            }
        }

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DETAIL_FIXTURE: &str = include_str!("../../tests/fixtures/mimo_detail.json");
    const USAGE_FIXTURE: &str = include_str!("../../tests/fixtures/mimo_usage.json");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/mimo_list.json");

    #[test]
    fn parses_detail_fixture() {
        let r: DetailResp = serde_json::from_str(DETAIL_FIXTURE).unwrap();
        let d = r.data.expect("data");
        assert_eq!(d.plan_name.as_deref(), Some("Max"));
        assert_eq!(
            d.current_period_end.as_deref(),
            Some("2026-07-26 23:59:59")
        );
    }

    #[test]
    fn parses_usage_fixture() {
        let r: UsageResp = serde_json::from_str(USAGE_FIXTURE).unwrap();
        let d = r.data.expect("data");
        let item = d.usage.expect("usage bucket").items.into_iter().next().unwrap();
        assert_eq!(item.limit, Some(82_000_000_000));
        assert_eq!(item.used, Some(6_387_380_709));
        let month_item = d.month_usage.expect("month bucket").items.into_iter().next().unwrap();
        assert_eq!(month_item.used, Some(6_387_380_709));
    }

    #[test]
    fn parses_list_fixture() {
        let (year_used, month_used, monthly) =
            parse_yearly(LIST_FIXTURE, 2026, "2026-07").expect("should parse");
        // 2026 年所有月份总和
        assert!(year_used > 0);
        // 当月 2026-07
        assert_eq!(month_used, 15_353_049 + 219_383_356);
        // 近 6 月序列
        assert!(!monthly.is_empty());
        // 正序
        let months: Vec<&str> = monthly.iter().map(|(m, _)| m.as_str()).collect();
        assert!(months.windows(2).all(|w| w[0] <= w[1]));
        // 最后一个应为最近月
        assert_eq!(monthly.last().unwrap().0, "2026-07");
    }

    #[test]
    fn parse_core_combines_detail_and_usage() {
        let now = 1_800_000_000;
        let u = parse_core(DETAIL_FIXTURE, USAGE_FIXTURE, now).expect("should parse");
        assert_eq!(u.tier, "Max");
        assert_eq!(u.total_credits, 82_000_000_000);
        assert_eq!(u.used_credits, 6_387_380_709);
        assert_eq!(u.fetched_at, now);
        // "2026-07-26 23:59:59" 北京时间 (UTC+8) → 2026-07-26 15:59:59 UTC
        assert_eq!(u.expire_at, 1_785_081_599);
        // 可选字段未填充（list 未传）
        assert_eq!(u.year_used, None);
        assert_eq!(u.month_used, None);
        assert_eq!(u.monthly_usage, None);
    }

    #[test]
    fn parse_beijing_datetime_works() {
        // 2026-07-26 23:59:59 北京 (UTC+8) = 2026-07-26 15:59:59 UTC
        let ts = parse_beijing_datetime("2026-07-26 23:59:59").unwrap();
        assert_eq!(ts, 1_785_081_599);
    }

    #[test]
    fn missing_data_errors() {
        assert!(parse_core(r#"{"code":0}"#, r#"{"code":0}"#, 0).is_err());
    }
}
