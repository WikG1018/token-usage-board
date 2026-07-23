import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";

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

function renderLoggedOut(message: string | null): string {
  return `
    <div class="panel">
      <div class="panel-header">
        <div class="brand"><span class="dot" style="background: var(--muted)"></span>Token Usage Board</div>
      </div>
      <div class="empty">
        <p>尚未连接 Xiaomi MiMo Token Plan</p>
        <button class="btn primary" id="btn-login">连接 MiMo</button>
      </div>
      ${message ? `<div class="status-line error">${message}</div>` : ""}
    </div>
  `;
}

function renderUsage(state: UsageState): string {
  const d = state.data!;
  const pct = percentUsed(d);
  const left = daysLeft(d);
  const expireClass = left <= 3 ? "danger" : left <= 7 ? "warn" : "";
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
          <span>已用 ${fmtNumber(d.used_credits)}</span>
          <span>剩余 ${fmtNumber(remaining(d))}</span>
        </div>
        <div class="progress-bar">
          <div class="progress-fill ${progressClass(d)}" style="width:${pct}%"></div>
        </div>
      </div>

      <div class="meta">
        <div class="meta-row"><span>总额度</span><strong>${fmtNumber(d.total_credits)} Credits</strong></div>
        <div class="meta-row"><span>已用比例</span><strong>${pct.toFixed(1)}%</strong></div>
        <div class="meta-row"><span>到期时间</span><strong class="${expireClass}">${fmtTime(d.expire_at)}</strong></div>
        <div class="meta-row"><span>剩余天数</span><strong class="${expireClass}">${left} 天</strong></div>
        <div class="meta-row"><span>最近刷新</span><strong>${fmtAgo(d.fetched_at)}</strong></div>
      </div>

      <div class="actions">
        <button class="btn" id="btn-refresh">刷新</button>
        <button class="btn" id="btn-official">官方控制台</button>
      </div>
      ${expiredNote}${staleNote}
      ${state.message ? `<div class="status-line error">${state.message}</div>` : ""}
    </div>
  `;
}

async function refresh(): Promise<void> {
  const app = document.querySelector<HTMLDivElement>("#app")!;
  try {
    const state = await invoke<UsageState>("get_usage_state");
    if (state.status === "logged_out" || !state.data) {
      app.innerHTML = renderLoggedOut(state.message);
    } else {
      app.innerHTML = renderUsage(state);
    }
  } catch (e) {
    app.innerHTML = renderLoggedOut(String(e));
  }
  bindEvents();
}

async function manualRefresh(): Promise<void> {
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

window.addEventListener("DOMContentLoaded", () => {
  void refresh();
});
