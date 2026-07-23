# Token Usage Board — 设计文档

日期：2026-07-23（2026-07-23 修订：新增深色玻璃 UI 与全平台架构）
仓库名：token-usage-board
许可证：MIT
目标平台：跨平台 —— 桌面（Windows/macOS/Linux 常驻系统托盘 + 悬停面板）与移动端（iOS/Android App 主界面 + 桌面小组件）。核心逻辑（取数/凭证/状态机）100% 复用，仅 UI 层与平台交互分形态实现。本期首交付 Windows，但架构与凭证存储按跨平台设计。

## 1. 背景与目标

构建一个跨平台常驻工具，在桌面系统托盘 / 移动端桌面小组件显示大模型「Token Plan」订阅套餐的用量信息；桌面端鼠标悬停托盘时展开详情面板，移动端打开 App 看详情、桌面小组件看概览。首个接入厂商为 **小米 Xiaomi MiMo Token Plan**，架构需支持后续低成本接入其他厂商。

### 1.1 数据来源（关键约束）

小米 MiMo Token Plan 是小米大模型的订阅套餐（Lite / Standard / Pro / Max 四档，按 Credits 点数计费）。**官方未公开「查询套餐用量/剩余 Credits」的编程接口**，官方仅支持登录 Web 控制台查看：

- 控制台：https://platform.xiaomimimo.com/console/plan-manage
- Token Plan API Key 格式 `tp-xxxxx`，Base URL `https://token-plan-cn.xiaomimimo.com/v1`（OpenAI 兼容，仅用于模型调用，不含套餐额度查询）

因此采用**逆向 Web 控制台内部 API** 的方式取数：登录态下复现控制台前端调用的内部用量查询接口。

### 1.2 成功标准

- 应用启动后常驻系统托盘，资源占用低（登录后不持有 webview）。
- 托盘 tooltip 实时反映剩余用量 / 到期天数。
- 悬停展开面板展示：套餐档位、Credits 总额/已用/剩余 + 进度条、有效期 + 剩余天数、最近刷新时间 + 手动刷新按钮、跳转官方网页按钮。
- 登录一次后可长期自动轮询；凭证过期有清晰提示并可一键重新登录。
- 内部 API 改版或网络异常时不崩溃，有可读的错误状态。

## 2. 总体架构（方案 A：API 重放）

登录时通过内嵌 webview 抓取控制台前端调用的内部 API（URL / 请求头 / Cookie），之后关闭 webview，由 Rust 端 `reqwest` 按周期重放该 API。托盘弹窗只渲染 Rust 缓存的数据。

```
┌─────────────────────────────────────────────────────┐
│  UI 层 (Web 前端，Tauri webview)                      │
│  ┌──────────────┐  ┌──────────────────────────────┐ │
│  │ 托盘图标渲染  │  │  悬停详情面板 (HTML/CSS/TS)   │ │
│  │ (Rust 调系统) │  │  档位/进度/有效期/刷新/跳转   │ │
│  └──────────────┘  └──────────────────────────────┘ │
└───────────────▲───────────────────▲─────────────────┘
                │ Tauri IPC (invoke / event)
┌───────────────┴───────────────────┴─────────────────┐
│  应用层 (Rust)                                        │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ │
│  │ Provider     │ │ Credential   │ │ Refresher    │ │
│  │ Trait(扩展点) │ │ Store(凭证)  │ │ (定时轮询)    │ │
│  └──────┬───────┘ └──────────────┘ └──────┬───────┘ │
│         │ impl                            │持有     │
│  ┌──────▼──────────────────────────────────▼──────┐ │
│  │       MiMo Provider (逆向内部 API)              │ │
│  │  fetch_usage() -> UsageData                     │ │
│  └─────────────────────────────────────────────────┘ │
└───────────────▲─────────────────────────────────────┘
                │ reqwest (带 Cookie)
┌───────────────┴─────────────────────────────────────┐
│  数据源层                                            │
│  登录态获取: 内嵌 webview 加载控制台页,注入 JS 钩取    │
│  用量查询: Rust reqwest 重放内部 API                  │
└─────────────────────────────────────────────────────┘
```

## 3. 核心组件

### 3.1 Provider Trait（扩展点）

统一接口，UI 层只依赖统一数据模型，不感知厂商。新增厂商 = 新增一个 `impl Provider`。

```rust
pub trait Provider {
    fn id(&self) -> &'static str;                 // "mimo"
    fn display_name(&self) -> &'static str;       // "Xiaomi MiMo"
    async fn fetch_usage(&self, cred: &Credential) -> Result<UsageData, ProviderError>;
}
```

### 3.2 UsageData（统一数据模型）

```rust
pub struct UsageData {
    pub tier: String,            // 套餐档位, e.g. "Standard"
    pub total_credits: u64,      // 总 Credits
    pub used_credits: u64,       // 已用 Credits
    pub expire_at: i64,          // 套餐到期时间 (unix 秒)
    pub fetched_at: i64,         // 本次数据抓取时间 (unix 秒)
}
impl UsageData { pub fn remaining(&self) -> u64; pub fn percent_used(&self) -> f64; pub fn days_left(&self) -> i64; }
```

### 3.3 Credential / CredentialStore

- `Credential { endpoint: String, cookies: Vec<(String,String)>, extra_headers: Vec<(String,String)>, obtained_at: i64 }`
- 使用跨平台安全存储：`keyring` crate（Windows 凭证管理器 / macOS Keychain / Linux Secret Service）。序列化为 JSON 后存入 keyring 条目，key = `token-usage-board/<provider_id>`，避免明文落盘。移动端通过 Tauri 平台层桥接系统 Keystore/Keychain。
- 接口：`get(provider_id) / save / clear / is_present`。

### 3.4 MiMoProvider

- 登录阶段：从 webview 捕获控制台内部用量查询 API 的 `endpoint`、必要 headers、Cookie。
- 查询阶段：`reqwest` 重放该请求，解析返回 JSON 为 `UsageData`。
- 解析逻辑对字段缺失宽容（详见错误处理），并对真实响应做 fixture 单元测试。

### 3.5 Refresher（轮询器）

- 默认轮询间隔 10 分钟（设置中可调）。
- 成功：更新内存缓存 + 通知 UI（Tauri event）。
- 401/403：标记凭证过期，停止轮询，提示重新登录。
- 网络失败：指数退避（1m → 5m → 15m 封顶），保留上次缓存。

### 3.6 UI 层

- 托盘图标：Rust 调 Win32 NotifyIcon；tooltip 显示「剩 X% · 到期 Y 天」；右键菜单（打开面板 / 重新登录 / 设置 / 退出）。
- 悬停详情面板：无边框、置顶、透明背景 webview 窗口；鼠标进入托盘区显示，离开延时隐藏；展示档位 / 进度条 / 有效期 / 刷新时间 / 手动刷新按钮 / 跳转官方网页按钮。
- 登录窗口：内嵌 webview，加载 MiMo 登录页。

## 4. 数据流与状态机

### 4.1 登录流程（一次性）

```
托盘右键「登录/连接 MiMo」→ 弹出 webview 加载 platform.xiaomimimo.com 登录页
→ 用户登录成功跳转到控制台 → 注入 JS 钩住 fetch/XHR 捕获内部用量 API(URL/headers/cookie)
→ 回传 Rust → CredentialStore(DPAPI) 加密落盘 → 关闭 webview → 立即触发首次 fetch
```

### 4.2 轮询流程（常驻）

```
Refresher(默认10min) → MiMoProvider.fetch_usage(credential)
→ 成功: 更新缓存 → 托盘 tooltip「剩 X% / 到期 Y 天」→ 面板可看详情
→ 401/403: 标记凭证过期 → 托盘黄标 + 「登录过期，点击重新登录」
→ 网络失败: 指数退避 → 显示上次缓存 + 「数据可能过期」
```

### 4.3 状态机

```
[未登录] --登录成功--> [已登录·数据新鲜]
[已登录·数据新鲜] --fetch成功(刷新)--> [已登录·数据新鲜]
[已登录·数据新鲜] --fetch失败(网络)--> [已登录·数据过期] --恢复--> [已登录·数据新鲜]
[已登录·数据新鲜/过期] --401/403--> [凭证过期] --重新登录--> [已登录·数据新鲜]
```

## 5. 错误处理

| 场景 | 处理 |
|---|---|
| 未登录 | 托盘灰显，tooltip「未连接」，面板显示「连接 MiMo」按钮 |
| Cookie 过期(401/403) | 停止轮询，托盘黄标，面板提示重新登录，保留最后一次成功数据 |
| 网络失败 | 指数退避，面板显示上次数据 + 「N 分钟前更新，可能已过期」 |
| 内部 API 改版(字段缺失) | 解析失败不崩溃，记录原始响应便于调试，面板提示「接口可能已变更」 |
| 凭证落盘失败 | 内存暂存本次会话，提示用户，不阻塞使用 |

## 6. 测试策略

- **Provider 解析（重点）**：用预先录制的真实内部 API 响应 JSON 作 fixture 做单元测试，断言解析出正确 tier / total / used / expire_at。不联网即可验证解析逻辑；改版时更新 fixture。
- **CredentialStore**：测试 keyring 跨平台安全存储的落盘 / 读取往返、is_present。
- **状态机**：测试状态流转（未登录→登录→过期→重新登录）。
- **UI**：手动验证为主（托盘、悬停、面板数据展示）。

## 7. 技术选型与项目结构

- 技术栈：Tauri v2（Rust 后端，桌面端）+ TypeScript + Vite（前端）；移动端复用同一 Tauri 核心，前端按平台分入口。
- 凭证存储：跨平台安全存储 —— 用 `keyring` crate（封装 Windows 凭证管理器 / macOS Keychain / Linux Secret Service），移动端通过 Tauri 平台层桥接 Keystore/Keychain。**不再使用 Windows DPAPI 直调**，`credential.rs` 改为调用 `keyring`，接口不变。
- 开源：MIT 许可证；仓库名 `token-usage-board`。

### 7.1 跨平台架构

```
┌──────────────────────────────────────────────────────┐
│  平台 UI 层（分形态）                                  │
│  ┌─────────────────────┐  ┌────────────────────────┐ │
│  │ 桌面: 托盘+悬停面板  │  │ 移动: App主界面+Widget  │ │
│  │ (Windows/mac/Linux) │  │ (iOS/Android)          │ │
│  └──────────┬──────────┘  └───────────┬────────────┘ │
└─────────────┼─────────────────────────┼──────────────┘
              │  Tauri IPC (invoke/event)│
┌─────────────┼─────────────────────────┼──────────────┐
│  共享核心层 (Rust, 100% 复用)          │              │
│  Provider trait + MiMoProvider / Refresher / 状态机   │
│  CredentialStore ── keyring (各平台原生安全存储)       │
└──────────────────────────────────────────────────────┘
```

桌面端交互：系统托盘图标 + 鼠标悬停弹玻璃面板 + 右键菜单。
移动端交互：App 主界面（同款玻璃面板 UI，全屏适配）+ 桌面小组件（极简概览，原生 WidgetKit/AppWidget，后续阶段）。

### 7.2 UI 视觉语言 — 深色玻璃质感（Glassmorphism）

- **玻璃层**：`rgba(20,22,30,0.55)` 深色半透明 + `backdrop-filter: blur(24px) saturate(1.4)`，透出壁纸/下层窗口的朦胧感。
- **描边与高光**：1px `rgba(255,255,255,0.08)` 描边；顶部一道 `rgba(255,255,255,0.18)` 高光边模拟玻璃受光。
- **配色点缀**：主色蓝紫渐变 `#6a8dff→#8a6aff`（进度条/状态点 + 外发光）；成功青绿、警告琥珀、危险红，均带柔和 glow。
- **字体层级**：标题大字距小号 caps，次要信息低对比灰，关键数字等宽突出，制造呼吸感。
- **圆角阴影**：16px 圆角 + 多层柔和投影（`0 12px 40px rgba(0,0,0,.5)` + 内阴影）。
- **微交互**：进度条 0.4s 缓动；按钮 hover 玻璃提亮 + `translateY(-1px)`；刷新按钮转圈。
- **跨平台一致**：纯 CSS 实现，桌面 webview（WebView2/WKWebView）与移动端 webview 均原生支持 `backdrop-filter`，视觉一致。移动端面板改为全屏卡片布局。

### 7.3 项目结构

```
token-usage-board/
├── src/                    # 前端 (TS + Vite)
│   ├── main.ts             # 桌面悬停面板入口
│   ├── mobile.ts           # 移动端主界面入口（后续阶段）
│   ├── styles.css          # 深色玻璃质感样式（跨平台共享）
│   └── index.html
├── src-tauri/              # Rust 后端（共享核心）
│   ├── src/
│   │   ├── main.rs         # 桌面入口
│   │   ├── lib.rs          # 移动端入口 + 共享 setup
│   │   ├── provider/       # trait + 统一模型
│   │   │   ├── mod.rs      # Provider trait, UsageData
│   │   │   └── mimo.rs     # MiMoProvider
│   │   ├── credential.rs   # CredentialStore (keyring, 跨平台)
│   │   ├── refresher.rs    # 轮询器
│   │   └── tray.rs         # 桌面托盘 (cfg(desktop))
│   ├── tests/fixtures/     # 录制的真实 API 响应 JSON
│   └── tauri.conf.json
├── LICENSE                 # MIT
└── README.md
```

## 8. 范围（YAGNI）

本期不做：
- 多厂商实装（仅预留 Provider 扩展点，不实装第二家）。
- 用量历史曲线 / 统计图表。
- 浏览器自动化抓取兜底（方案 C），待 API 失效真实发生时再评估。
- 自动续费、套餐升级等操作（只读展示）。
- **移动端实装**：架构与凭证存储按跨平台设计，但本期只交付 Windows 桌面端；macOS/Linux 与 iOS/Android 实装列为后续阶段，待桌面端稳定后再逐平台推进。
