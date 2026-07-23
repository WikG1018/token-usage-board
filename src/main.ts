import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { listen } from "@tauri-apps/api/event";

interface UsageData {
  tier: string;
  total_credits: number;
  used_credits: number;
  expire_at: number; // unix seconds
  fetched_at: number; // unix seconds
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

      <div class="actions">
        <button class="btn" id="btn-refresh"><span class="spinner"></span><span class="label">刷新</span></button>
        <button class="btn" id="btn-official"><span class="label">官方控制台</span></button>
      </div>
      ${expiredNote}${staleNote}
      ${state.message ? `<div class="status-line error">${state.message}</div>` : ""}
    </div>
  `;
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
