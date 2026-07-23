# token-usage-board

一个常驻 Windows 系统托盘的 Token Plan 用量展示板。鼠标悬停托盘图标时展开详情面板。首个接入的厂商是 **小米 Xiaomi MiMo Token Plan**，架构上预留了 Provider 扩展点，便于后续接入其他厂商。

## 功能

- 常驻系统托盘，低资源占用
- 托盘 tooltip 实时显示剩余用量 / 到期天数
- 悬停展开面板，展示：
  - 套餐档位（Lite / Standard / Pro / Max）
  - Credits 总额 / 已用 / 剩余 + 进度条
  - 套餐有效期 + 剩余天数（临近到期高亮）
  - 最近刷新时间 + 手动刷新按钮
  - 跳转官方控制台按钮
- 登录一次后自动定时轮询；凭证过期可一键重新登录
- 网络异常 / 接口变更时有可读的错误状态，不崩溃

## 数据来源说明

小米 MiMo Token Plan **没有公开的用量查询 API**，官方仅支持在 Web 控制台查看。本工具通过逆向控制台前端调用的内部接口获取用量：登录时在内嵌 webview 捕获内部 API 的请求信息，之后由 Rust 端定期重放该请求。凭证使用 Windows DPAPI 加密后本地存储。

## 技术栈

- 后端：Rust + [Tauri v2](https://tauri.app)
- 前端：TypeScript + Vite（无框架）

## 开发

前置要求：Node.js 18+、Rust 1.90+。

```bash
npm install
npm run tauri dev
```

## 构建

```bash
npm run tauri build
```

## 许可证

[MIT](./LICENSE)
