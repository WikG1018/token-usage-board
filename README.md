# token-usage-board

一个常驻系统托盘的 **Token Plan 用量展示板**。鼠标悬停托盘图标即展开详情面板，深色玻璃质感 UI，低资源占用。首个接入厂商为 **小米 Xiaomi MiMo Token Plan**，架构上预留了 Provider 扩展点，便于后续接入其他厂商。

跨平台架构已就绪：核心 Rust 逻辑 100% 复用，桌面端（Windows / macOS / Linux）共享同一套托盘 + 悬停面板形态，移动端（iOS / Android）为后续阶段。

## 功能亮点

- **常驻托盘**：低资源占用，托盘 tooltip 实时显示剩余用量百分比与到期天数
- **悬停展开**：鼠标移入托盘图标，玻璃质感面板自动从屏幕右下角托盘区上方弹出，失焦自动隐藏
- **深色玻璃 UI**：`backdrop-filter` blur + 半透明 + 高光描边 + 蓝紫渐变 glow，现代高级感
- **状态机驱动**：未连接 / 数据新鲜 / 数据过期 / 凭证过期 / 网络异常，每种状态都有可读的界面与提示
- **安全凭证存储**：跨平台 keyring（Windows 凭证管理器 / macOS Keychain / Linux Secret Service），不落盘明文
- **智能轮询**：成功时 10 分钟一刷，失败时退避（1 分钟 → 5 分钟 → 15 分钟），不狂打接口
- **一键断开**：托盘菜单「断开连接」清除本地凭证并回到未连接态

面板内容：

- 套餐档位（Lite / Standard / Pro / Max）
- Credits 总额 / 已用 / 剩余 + 进度条
- 套餐有效期 + 剩余天数（临近到期高亮）
- 最近刷新时间 + 手动刷新按钮（带 spinner）
- 跳转官方控制台按钮

## 数据来源说明

小米 MiMo Token Plan **没有公开的用量查询 API**，官方仅支持在 Web 控制台查看。本工具通过逆向控制台前端调用的内部接口获取用量：

1. 首次使用时，点击托盘菜单「重新登录」打开内嵌 webview 加载官方控制台
2. 注入的捕获脚本拦截 `fetch`，匹配用量相关 URL（`usage|plan|credit|quota`）时，回传 endpoint、请求头与 `document.cookie`
3. Rust 端将凭证序列化为 JSON 存入系统 keyring
4. 之后由 Rust `reqwest` 周期重放该请求，解析返回的 JSON 为统一 `UsageData`

> 凭证仅本地存储，不会上传到任何第三方服务器。本工具不修改任何官方接口数据，仅读取用量信息。

## 技术栈

| 层 | 技术 |
| --- | --- |
| 后端 | Rust + [Tauri v2](https://tauri.app) |
| 前端 | TypeScript + Vite（无框架） |
| 网络 | reqwest（含 cookies feature） |
| 凭证存储 | [keyring](https://crates.io/crates/keyring) v3（按平台启用原生后端） |
| 异步运行时 | tokio |

## 开发

前置要求：Node.js 18+、Rust 1.90+（本地 1.95 可用）。

```bash
npm install
npm run tauri dev
```

开发模式启动后，托盘图标出现在系统托盘。首次使用右键 → 「重新登录」，在弹出的 webview 中登录小米 MiMo 控制台，登录成功并触发一次用量请求后窗口自动关闭，面板开始显示数据。

## 构建

```bash
npm run tauri build
```

产物位于 `src-tauri/target/release/bundle/`（Windows 为 `.msi` / `.exe`）。

## 项目结构

```
src/                      # 前端
  index.html              # 面板结构
  main.ts                 # 状态渲染 + 事件监听 + 刷新
  styles.css              # 深色玻璃质感样式（跨平台共享）
src-tauri/
  src/
    lib.rs                # Tauri 入口：命令注册 + 窗口事件
    tray.rs               # 托盘、菜单、悬停面板定位、登录窗口、捕获脚本
    state.rs              # AppState 状态机 + tooltip 文案
    refresher.rs          # 周期刷新 + 退避 + 事件广播
    credential.rs         # keyring 凭证存储
    provider/
      mod.rs              # Provider trait + 统一 UsageData
      mimo.rs             # 小米 MiMo 实现
  tests/fixtures/         # 真实接口返回样本（脱敏）
docs/superpowers/         # 设计文档与实施计划
```

## 扩展其他厂商

实现 `Provider` trait 即可接入新厂商：

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;           // 如 "mimo"
    fn display_name(&self) -> &'static str; // 如 "Xiaomi MiMo"
    async fn fetch_usage(&self, cred: &Credential) -> Result<UsageData, ProviderError>;
}
```

`UsageData` 字段固定：`tier / total_credits / used_credits / expire_at / fetched_at`（unix 秒）。在 `AppState::new()` 中替换 provider 实例即可，凭证按 `token-usage-board/<provider_id>` 自动隔离存储。

## 路线图

- [x] Windows 桌面端核心闭环（托盘 + 悬停面板 + 玻璃 UI + keyring + 轮询）
- [x] 跨平台凭证存储架构（keyring feature 自动按平台接入）
- [ ] macOS / Linux 桌面端验证（需对应平台环境）
- [ ] 移动端 App 主界面（iOS / Android，复用核心 + 玻璃 UI，全屏卡片布局）
- [ ] 移动端桌面小组件（iOS WidgetKit / Android AppWidget，极简概览）

## 贡献

欢迎 Issue 与 PR。请确保 `cargo test`（后端）与 `npx tsc --noEmit`（前端）保持绿色。

## 许可证

[MIT](./LICENSE)
