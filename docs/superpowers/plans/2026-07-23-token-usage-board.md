# Token Usage Board 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把已搭好的 Tauri v2 + TypeScript 骨架推进为一个可用的、常驻 Windows 托盘的 MiMo Token Plan 用量展示板，并为后续接入其他厂商预留扩展点。

**Architecture:** 方案 A（API 重放）。登录时内嵌 webview 捕获控制台内部用量 API（URL/headers/Cookie），之后 Rust `reqwest` 周期重放；桌面托盘 + 悬停面板渲染 Rust 缓存的统一 `UsageData`。凭证用 keyring 跨平台安全存储。核心 Rust 100% 复用，桌面端托盘/悬停、移动端 App/Widget 仅 UI 层分形态。

**Tech Stack:** Tauri v2 (Rust)、TypeScript + Vite、reqwest、keyring（跨平台安全存储）。深色玻璃质感 UI（backdrop-filter）。

## Global Constraints

- 平台：跨平台架构（核心 Rust 100% 复用）；本期交付 Windows 桌面端，macOS/Linux 与 iOS/Android 为后续阶段。Rust `rust-version = "1.90"`（本地 1.95 可用）。
- 许可证：MIT；仓库名 `token-usage-board`。
- 依赖版本（已锁定在 Cargo.toml）：tauri `2`、tauri-plugin-shell `2`、reqwest `0.12`、tokio `1`、serde `1`、thiserror `2`、async-trait `0.1`、chrono `0.4`、anyhow `1`、keyring `3`（按平台启用 `windows-native`/`apple-native`/`linux-native-sync-persistent`）。
- 统一数据模型字段名固定：`tier / total_credits / used_credits / expire_at / fetched_at`（unix 秒）。
- Provider 接口：`fn id(&self)->&'static str; fn display_name(&self)->&'static str; async fn fetch_usage(&self,&Credential)->Result<UsageData,ProviderError>`。
- 凭证存储：keyring 跨平台安全存储，key = `token-usage-board/<provider_id>`，序列化为 JSON 存入系统凭证库（不再用文件落盘）。
- UI 视觉：深色玻璃质感（backdrop-filter blur + 半透明 + 高光边 + 蓝紫渐变 glow），跨平台共享同一 `styles.css`。
- 每个任务结束都要 `cargo test`（后端）或 `npx tsc --noEmit`（前端）保持绿色，并频繁提交。

---

## 现状（已完成，作为基线）

- 骨架：`src-tauri/{main.rs,lib.rs,state.rs,refresher.rs,tray.rs,credential.rs,provider/{mod.rs,mimo.rs}}`、前端 `src/{index.html,main.ts,styles.css}`、配置 `{package.json,tsconfig.json,vite.config.ts,src-tauri/tauri.conf.json}`、图标、LICENSE、README、设计文档、实施计划。
- 跨平台凭证存储：`credential.rs` 已用 keyring（Windows 凭证管理器实装，macOS/Linux 由 feature 自动接入）。
- 深色玻璃 UI：`styles.css` 已重写为 glassmorphism；`main.ts` 已适配新结构 + usage-updated 事件监听 + 刷新按钮 spinner。
- 验证：`cargo test` 4/4 通过（mimo fixture 解析 / 别名 / 缺失报错 / keyring 往返）；`npx tsc --noEmit` 通过。
- 基线提交：`1cb1467`（含 `38c755b` 骨架、`7a97a06` 计划）。

---

### Task 1: 修正托盘图标 tooltip 动态反映用量

**Files:**
- Modify: `src-tauri/src/tray.rs`（新增 `update_tray_tooltip(app:&AppHandle, tip:&str)`）
- Modify: `src-tauri/src/refresher.rs`（刷新后调用）
- Modify: `src-tauri/src/state.rs`（暴露 `tooltip_text()`）

**Interfaces:**
- Consumes: `AppState::snapshot() -> UsageState`、`UsageData::{remaining,percent_used,days_left}`。
- Produces: `pub fn update_tray_tooltip(app:&AppHandle, tip:&str)`；`AppState::tooltip_text(&self)->String`。

- [ ] **Step 1: 写状态→tooltip 文案的纯函数 + 单元测试**

在 `src-tauri/src/state.rs` 末尾加：

```rust
pub fn tooltip_for(state: &UsageState) -> String {
    match (state.status, &state.data) {
        (Status::LoggedOut, _) => "Token Usage Board · 未连接".into(),
        (_, Some(d)) => {
            let now = chrono::Utc::now().timestamp();
            format!(
                "MiMo · 剩 {:.0}% · 到期 {} 天",
                100.0 - d.percent_used(),
                d.days_left(now)
            )
        }
        (_, None) => "Token Usage Board · 数据获取失败".into(),
    }
}
```

测试（同文件 `#[cfg(test)] mod tooltip_tests`）：

```rust
#[test]
fn tooltip_shows_percent_and_days() {
    let now = chrono::Utc::now().timestamp();
    let s = UsageState {
        status: Status::Fresh,
        data: Some(UsageData {
            tier: "Standard".into(),
            total_credits: 1000,
            used_credits: 250,
            expire_at: now + 10 * 86400,
            fetched_at: now,
        }),
        message: None,
    };
    let t = tooltip_for(&s);
    assert!(t.contains("剩 75%"), "got: {t}");
    assert!(t.contains("到期 10 天"), "got: {t}");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test tooltip_shows_percent_and_days`
Expected: FAIL（`tooltip_for` 未定义）

- [ ] **Step 3: 实现 `update_tray_tooltip` 并接线**

`tray.rs` 增加：

```rust
pub fn update_tray_tooltip(app: &AppHandle, tip: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tip));
    }
}
```

`refresher.rs` 在 `refresh_now` 后：

```rust
let snap = state.snapshot().await;
crate::tray::update_tray_tooltip(&app, &crate::state::tooltip_for(&snap));
let _ = app.emit("usage-updated", snap);
```

- [ ] **Step 4: 运行测试 + 编译**

Run: `cargo test tooltip` 与 `cargo check`
Expected: PASS + 无错误

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/refresher.rs src-tauri/src/state.rs
git commit -m "feat(tray): dynamic tooltip with remaining % and days left"
```

---

### Task 2: 托盘悬停自动显示/隐藏面板

**Files:**
- Modify: `src-tauri/src/tray.rs`（`setup_tray` 增加 hover 处理）
- Modify: `src-tauri/src/lib.rs`（panel blur 自动隐藏）

**Interfaces:**
- Consumes: 已存在的 `panel` 窗口（tauri.conf.json）、`toggle_panel`。
- Produces: `fn position_and_show_panel(app:&AppHandle)`；`fn hide_panel(app:&AppHandle)`。

- [ ] **Step 1: 实现定位并显示面板（托盘图标右下角）**

```rust
fn position_and_show_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn hide_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
}
```

- [ ] **Step 2: 接入托盘 hover 与菜单「打开面板」**

在 `setup_tray` 的 `TrayIconBuilder` 上 `.on_tray_icon_event` 处理 `TrayIconEvent::Enter` 显示面板；菜单 `show` 改为 `position_and_show_panel(app)`。

- [ ] **Step 3: panel 失焦自动隐藏（lib.rs on_window_event 增加 `Focused(false)`）**

```rust
tauri::WindowEvent::Focused(false) if window.label() == "panel" => {
    let _ = window.hide();
}
```

- [ ] **Step 4: 编译 + 手动验证**

Run: `cargo check`
Expected: 通过；`npm run tauri dev` 后悬停托盘图标弹出面板、移开/失焦隐藏。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/lib.rs
git commit -m "feat(panel): show on tray hover, hide on blur"
```

---

### Task 3: 前端订阅 usage-updated 事件实时刷新

**Files:**
- Modify: `src/main.ts`（监听 Tauri event）

**Interfaces:**
- Consumes: 后端 `app.emit("usage-updated", UsageState)`（Task 1 已发）。
- Produces: 前端 `listenUsageEvents()`。

- [ ] **Step 1: 引入 listen 并订阅**

```ts
import { listen } from "@tauri-apps/api/event";

async function listenUsageEvents(): Promise<void> {
  await listen<UsageState>("usage-updated", (e) => {
    renderFromState(e.payload);
  });
}
```

把 `refresh()` 的渲染逻辑抽成 `renderFromState(state: UsageState)`，`DOMContentLoaded` 时同时 `void refresh()` 与 `void listenUsageEvents()`。

- [ ] **Step 2: 类型检查**

Run: `npx tsc --noEmit`
Expected: EXIT=0

- [ ] **Step 3: Commit**

```bash
git add src/main.ts
git commit -m "feat(panel): live-update via usage-updated event"
```

---

### Task 4: 完善登录捕获流程（捕获 Cookie + 必要头）

**Files:**
- Modify: `src-tauri/src/tray.rs`（CAPTURE_SCRIPT 与 `credential_candidate`）
- Test: `src-tauri/src/credential.rs`（已有 roundtrip 可复用）

**Interfaces:**
- Consumes: `Credential{endpoint,cookies,extra_headers,obtained_at}`、`AppState::on_credential_captured`。
- Produces: `credential_candidate(app, endpoint, headers, cookies)` 命令签名（新增 `cookies: Vec<(String,String)>`）。

- [ ] **Step 1: 前端脚本同时回传 Cookie**

`CAPTURE_SCRIPT` 增加读取 `document.cookie` 并随 `credential_candidate` 一起回传（按 `; ` 拆成键值对）。

- [ ] **Step 2: 扩展 `credential_candidate` 接收 cookies 并落盘**

```rust
#[tauri::command]
pub async fn credential_candidate(
    app: tauri::AppHandle,
    endpoint: String,
    headers: serde_json::Value,
    cookies: Vec<(String, String)>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let cred = Credential {
        endpoint,
        cookies,
        extra_headers: serde_json::from_value::<Vec<(String, String)>>(headers).unwrap_or_default(),
        obtained_at: chrono::Utc::now().timestamp(),
    };
    state.on_credential_captured(cred).await.map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("login") { let _ = w.close(); }
    Ok(())
}
```

- [ ] **Step 3: 编译 + 类型检查**

Run: `cargo check` 与 `npx tsc --noEmit`
Expected: 均通过

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tray.rs src/main.ts
git commit -m "feat(login): capture cookies+headers from console webview and persist via DPAPI"
```

---

### Task 5: 托盘菜单补「退出前确认 + 登出」与设置项占位

**Files:**
- Modify: `src-tauri/src/tray.rs`（菜单加 logout）
- Modify: `src-tauri/src/lib.rs`（注册已有 `logout` 命令已在）

**Interfaces:**
- Consumes: `AppState::logout()`。
- Produces: 菜单项 `logout` 触发后端 logout 并隐藏面板。

- [ ] **Step 1: 菜单新增「断开连接」调用 logout**

```rust
"logout" => {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _ = state.logout().await;
    });
    hide_panel(app);
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check`
Expected: 通过

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tray.rs
git commit -m "feat(tray): add disconnect (logout) menu item"
```

---

### Task 6（后续阶段）: macOS/Linux 桌面端验证

**前置**：Task 1–5 完成且 Windows 端稳定。

**Files:**
- Verify: `src-tauri/src/tray.rs` 已用 `cfg(desktop)`，托盘在 macOS/Linux 应可用
- Verify: keyring `apple-native`/`linux-native-sync-persistent` feature 已配
- Modify（按需）: `src-tauri/tauri.conf.json` 图标（macOS `.icns` 已生成）、`Info.plist` 等

- [ ] **Step 1: macOS 构建验证**

Run: `cargo check --target aarch64-apple-darwin`（需 macOS 环境）
Expected: 通过；托盘/面板/凭证存储在 macOS 上行为一致（Keychain）。

- [ ] **Step 2: Linux 构建验证**

Run: `cargo check --target x86_64-unknown-linux-gnu`（需 Linux 环境 + libsecret）
Expected: 通过；凭证走 Secret Service。

- [ ] **Step 3: 视觉一致性核对**

确认 `backdrop-filter` 在 WKWebView（macOS）/WebKitGTK（Linux）下生效；若 WebKitGTK 旧版不支持，降级为不透明深色底（媒体查询或 `@supports`）。

- [ ] **Step 4: Commit（如有适配改动）**

```bash
git add -A
git commit -m "build: verify macOS/Linux desktop (keyring + tray + glass UI)"
```

---

### Task 7（后续阶段）: 移动端 App 主界面（iOS/Android）

**前置**：桌面端三平台稳定。

**Files:**
- Create: `src/mobile.ts`（移动端主界面入口，复用 `styles.css` 玻璃质感，全屏卡片布局）
- Create: `src/mobile.html`
- Modify: `src-tauri/src/lib.rs`（`#[cfg_attr(mobile, tauri::mobile_entry_point)]` 已就绪，补移动端 setup，去掉托盘）
- Modify: `src-tauri/tauri.conf.json`（移动端窗口配置）
- Modify: `vite.config.ts`（按平台选入口）

**Interfaces:**
- Consumes: 共享核心层所有 `invoke` 命令（`get_usage_state`/`refresh_now`/`open_login_window`/`logout`/`credential_candidate`）。
- Produces: 移动端主界面，无托盘；登录改为 App 内 webview；UI 复用深色玻璃质感，全屏适配。

- [ ] **Step 1: 写移动端入口 `src/mobile.html` + `src/mobile.ts`**

复用 `renderFromState` 逻辑，面板改为全屏卡片（去掉固定 340px 宽，改为 `max-width: 480px; margin: auto; padding-top: env(safe-area-inset-top)`）。

- [ ] **Step 2: `lib.rs` 区分桌面/移动 setup**

```rust
.setup(|app| {
    #[cfg(desktop)]
    {
        tray::setup_tray(app.handle())?;
    }
    refresher::spawn_refresher(app.handle().clone());
    Ok(())
})
```

- [ ] **Step 3: 类型检查 + 编译**

Run: `npx tsc --noEmit` 与（需移动端工具链）`cargo tauri android init` / `cargo tauri ios init`
Expected: 通过

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(mobile): add iOS/Android app entry reusing core + glass UI"
```

---

### Task 8（后续阶段，可选）: 移动端桌面小组件

**前置**：Task 7 完成。

**说明**：iOS WidgetKit / Android AppWidget 需各自原生实现，展示极简概览（剩余 % + 到期天数）。数据由 App 通过 App Group / SharedPreferences 共享给小组件。范围较大，单独立 spec 推进，本计划仅记录入口。

---

## 自审记录

- **Spec 覆盖**：设计文档 §4 数据流（登录/轮询/状态机）→ Task 2/4/1；§5 错误处理 → Task 1 tooltip + 已有 state.rs；§3.6 UI（悬停/手动刷新/事件）→ Task 2/3；§6 测试 → 各 Task TDD + 基线测试；§7.1 跨平台架构 → Task 6/7/8；§7.2 深色玻璃 UI → 已在基线实装。剩余「设置中调轮询间隔」列为后续可选项，未纳入本期（YAGNI）。
- **占位符**：无 TBD/TODO；每步含可运行代码与命令（Task 6/7/8 为后续阶段，含明确前置与验证命令）。
- **类型一致性**：`tooltip_for`、`update_tray_tooltip`、`credential_candidate(endpoint,headers,cookies)`、`renderFromState` 在产出与消费处签名一致。
