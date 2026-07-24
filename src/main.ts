import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { listen } from "@tauri-apps/api/event";

interface UsageData {
  tier: string;
  total_credits: number;
  used_credits: number;
  expire_at: number; // unix seconds
  fetched_at: number; // unix seconds
  year_used?: number | null;
  month_used?: number | null;
  monthly_usage?: Array<[string, number]> | null; // ["YYYY-MM", tokens]
}

interface UsageState {
  status: "logged_out" | "fresh" | "stale" | "expired" | "error";
  data: UsageData | null;
  message: string | null;
}

const OFFICIAL_URL = "https://platform.xiaomimimo.com/console/plan-manage";

function remaining(d: UsageData): number {
  return Math.max(0, d.total_credits - d.used_credits);
}

function percentUsed(d: UsageData): number {
  if (d.total_credits <= 0) return 0;
  return Math.min(100, (d.used_credits / d.total_credits) * 100);
}

function daysLeft(d: UsageData): number {
  const now = Date.now() / 1000;
  return Math.max(0, Math.ceil((d.expire_at - now) / 86400));
}

function fmtNumber(n: number): string {
  return n.toLocaleString("en-US");
}

/**
 * 自适应单位的 token 数量格式化：
 * ≥1亿 → X.XX 亿；≥1千万 → X.X 千万；≥1百万 → X.X 百万；否则原数字。
 */
function fmtTokens(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0";
  const yi = 1e8;
  const qianWan = 1e7;
  const baiWan = 1e6;
  if (n >= yi) return `${(n / yi).toFixed(2)} 亿`;
  if (n >= qianWan) return `${(n / qianWan).toFixed(2)} 千万`;
  if (n >= baiWan) return `${(n / baiWan).toFixed(2)} 百万`;
  return fmtNumber(n);
}

function fmtTime(unixSec: number): string {
  const d = new Date(unixSec * 1000);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function fmtAgo(unixSec: number): string {
  const diff = Math.max(0, Math.floor(Date.now() / 1000 - unixSec));
  if (diff < 60) return `${diff} 秒前`;
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

function fmtMonthLabel(ym: string): string {
  // "2026-07" → "7月"
  const parts = ym.split("-");
  if (parts.length !== 2) return ym;
  return `${parseInt(parts[1], 10)}月`;
}

function progressClass(d: UsageData): string {
  const p = percentUsed(d);
  if (p >= 90) return "danger";
  if (p >= 70) return "warn";
  return "";
}

function expireClass(left: number): string {
  if (left <= 3) return "danger";
  if (left <= 7) return "warn";
  return "";
}

function renderLoggedOut(message: string | null): string {
  return `
    <div class="panel">
      <div class="panel-header">
        <div class="brand"><span class="dot off"></span>Token Usage Board</div>
      </div>
      <div class="empty">
        <div class="empty-icon">◎</div>
        <p>尚未连接 Xiaomi MiMo Token Plan<br/>连接后即可在托盘查看用量</p>
        <button class="btn primary" id="btn-login"><span class="label">连接 MiMo</span></button>
      </div>
      ${message ? `<div class="status-line error">${message}</div>` : ""}
    </div>
  `;
}

function renderUsage(state: UsageState): string {
  const d = state.data!;
  const pct = percentUsed(d);
  const left = daysLeft(d);
  const expCls = expireClass(left);
  const staleNote =
    state.status === "stale"
      ? `<div class="status-line error">数据可能已过期（${fmtAgo(d.fetched_at)}更新）</div>`
      : "";
  const expiredNote =
    state.status === "expired"
      ? `<div class="status-line error">登录已过期，请重新连接</div>`
      : "";

  // 年/月用量区块：仅当任一字段存在时渲染
  const hasYearMonth = d.year_used != null || d.month_used != null;
  const yearMonthBlock = hasYearMonth
    ? `
      <div class="ym-block">
        ${d.year_used != null ? `
          <div class="ym-cell">
            <span class="ym-label">年使用</span>
            <span class="ym-value">${fmtTokens(d.year_used)}</span>
          </div>` : ""}
        ${d.month_used != null ? `
          <div class="ym-cell">
            <span class="ym-label">月使用</span>
            <span class="ym-value">${fmtTokens(d.month_used)}</span>
          </div>` : ""}
      </div>`
    : "";

  // 近6月柱状图：仅当 monthly_usage 存在且非空时渲染
  const monthlyBlock = renderMonthlyChart(d.monthly_usage);

  return `
    <div class="panel">
      <div class="panel-header">
        <div class="brand"><span class="dot"></span>Xiaomi MiMo</div>
        <div class="tier">${d.tier}</div>
      </div>

      <div class="progress-wrap">
        <div class="progress-labels">
          <span>已用 <span class="num">${fmtNumber(d.used_credits)}</span></span>
          <span>剩余 <span class="num">${fmtNumber(remaining(d))}</span></span>
        </div>
        <div class="progress-bar">
          <div class="progress-fill ${progressClass(d)}" style="width:${pct}%"></div>
        </div>
      </div>

      ${yearMonthBlock}

      <div class="meta">
        <div class="meta-cell">
          <span class="meta-label">总额度</span>
          <span class="meta-value">${fmtNumber(d.total_credits)}</span>
        </div>
        <div class="meta-cell">
          <span class="meta-label">已用比例</span>
          <span class="meta-value">${pct.toFixed(1)}%</span>
        </div>
        <div class="meta-cell">
          <span class="meta-label">到期时间</span>
          <span class="meta-value ${expCls}">${fmtTime(d.expire_at)}</span>
        </div>
        <div class="meta-cell">
          <span class="meta-label">剩余天数</span>
          <span class="meta-value ${expCls}">${left} 天</span>
        </div>
      </div>

      <div class="meta" style="grid-template-columns: 1fr;">
        <div class="meta-cell">
          <span class="meta-label">最近刷新</span>
          <span class="meta-value" style="font-weight:500;color:var(--muted);">${fmtAgo(d.fetched_at)}</span>
        </div>
      </div>

      ${monthlyBlock}

      <div class="actions">
        <button class="btn" id="btn-refresh"><span class="spinner"></span><span class="label">刷新</span></button>
        <button class="btn" id="btn-official"><span class="label">官方控制台</span></button>
      </div>
      ${expiredNote}${staleNote}
      ${state.message ? `<div class="status-line error">${state.message}</div>` : ""}
    </div>
  `;
}

/**
 * 渲染近6月柱状图。仅当 monthly 非空时返回 HTML，否则返回空串（不渲染该区块）。
 * 柱高按当月用量 / max(全部用量) 计算；max 为 0 时所有柱子给极小高度避免全 0。
 */
function renderMonthlyChart(monthly: UsageData["monthly_usage"]): string {
  if (!monthly || monthly.length === 0) return "";
  const entries = monthly.slice(-6); // 取最后6个，保证"近6月"
  if (entries.length === 0) return "";
  const max = Math.max(1, ...entries.map((e) => e[1]));
  const bars = entries
    .map(([ym, used]) => {
      const h = Math.max(4, Math.round((used / max) * 100));
      return `
        <div class="bar-col">
          <div class="bar-track">
            <div class="bar" style="height:${h}%">
              <span class="bar-value">${fmtTokens(used)}</span>
            </div>
          </div>
          <span class="bar-day">${fmtMonthLabel(ym)}</span>
        </div>`;
    })
    .join("");
  return `
    <div class="daily-chart">
      <div class="chart-title">近 ${entries.length} 月用量</div>
      <div class="chart-bars">${bars}</div>
    </div>`;
}

function renderFromState(state: UsageState): void {
  const app = document.querySelector<HTMLDivElement>("#app")!;
  if (state.status === "logged_out" || !state.data) {
    app.innerHTML = renderLoggedOut(state.message);
  } else {
    app.innerHTML = renderUsage(state);
  }
  bindEvents();
}

async function refresh(): Promise<void> {
  try {
    const state = await invoke<UsageState>("get_usage_state");
    renderFromState(state);
  } catch (e) {
    renderFromState({ status: "logged_out", data: null, message: String(e) });
  }
}

async function manualRefresh(): Promise<void> {
  const btn = document.querySelector<HTMLButtonElement>("#btn-refresh");
  btn?.classList.add("loading");
  try {
    await invoke("refresh_now");
  } catch (e) {
    console.error(e);
  }
  await refresh();
}

async function connect(): Promise<void> {
  try {
    await invoke("open_login_window");
  } catch (e) {
    console.error(e);
  }
}

function bindEvents(): void {
  document.querySelector("#btn-refresh")?.addEventListener("click", manualRefresh);
  document.querySelector("#btn-login")?.addEventListener("click", connect);
  document.querySelector("#btn-official")?.addEventListener("click", () => {
    void open(OFFICIAL_URL);
  });
}

async function listenUsageEvents(): Promise<void> {
  await listen<UsageState>("usage-updated", (e) => {
    renderFromState(e.payload);
  });
}

window.addEventListener("DOMContentLoaded", () => {
  void refresh();
  void listenUsageEvents();
});
