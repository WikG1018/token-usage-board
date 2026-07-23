# Token Usage Board — 设计文档

日期：2026-07-23
仓库名：token-usage-board
许可证：MIT
目标平台：Windows（常驻任务栏 / 系统托盘）

## 1. 背景与目标

构建一个 Windows 桌面常驻工具，在系统托盘显示大模型「Token Plan」订阅套餐的用量信息；鼠标悬停时展开详情面板。首个接入厂商为 **小米 Xiaomi MiMo Token Plan**，架构需支持后续低成本接入其他厂商。

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

- `Credential { cookies: Vec<(String,String)>, extra_headers: Vec<(String,String)>, obtained_at: i64 }`
- 使用 Windows DPAPI（`windows` crate，`CryptProtectData`）加密后落盘到 `%APPDATA%/token-usage-board/credentials.bin`，避免明文存储。
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
- **CredentialStore**：测试 DPAPI 加密落盘 / 读取往返、is_present。
- **状态机**：测试状态流转（未登录→登录→过期→重新登录）。
- **UI**：手动验证为主（托盘、悬停、面板数据展示）。

## 7. 技术选型与项目结构

- 技术栈：Tauri v2（Rust 后端）+ TypeScript + Vite（前端）。
- 开源：MIT 许可证；仓库名 `token-usage-board`。

```
token-usage-board/
├── src/                    # 前端 (TS + Vite): 悬停面板、登录页 UI
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── main.rs
│   │   ├── provider/       # trait + 统一模型
│   │   │   ├── mod.rs      # Provider trait, UsageData
│   │   │   └── mimo.rs     # MiMoProvider
│   │   ├── credential.rs   # CredentialStore (DPAPI)
│   │   ├── refresher.rs    # 轮询器
│   │   └── tray.rs         # 托盘图标与菜单
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
