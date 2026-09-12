# MollyCloud Desktop

MollyCloud 的 Windows 桌面客户端。它把账户控制台、透明 Live2D 桌宠、Molly AI 助手、开发工具供应商管理和图片工作台组合为一个本地应用。

当前版本：[v0.1.1](https://github.com/ititrica/mollycloud-desktop/releases/tag/v0.1.1)。可从 Release 下载 [NSIS 安装器](https://github.com/ititrica/mollycloud-desktop/releases/latest) 或 MSI 安装包。

## 用途

- 登录 MollyCloud 账户，查看余额、累计消费、订阅、API 密钥状态和 Token 用量。
- 在桌面上运行可拖动、可显示或隐藏的 Live2D Molly；系统托盘可快速唤出控制台。
- 使用 Molly 助手对话，并在控制台设置桌宠显示、模型、助手和开机自启。
- 在内置 CC Switch 中管理供应商，并在用户点击“启用”后配置本机的 Claude Code、Codex、Gemini CLI 等工具。
- 在独立隔离的生图工作台中，使用手动保存的 API Key 进行图片生成、编辑和历史管理。

充值和订阅购买当前仍跳转至 MollyCloud 网页端，桌面端不会自动创建订单或扣款。

## 架构

```text
MollyCloud Desktop (Tauri 2 / Windows)
|
|-- Console window
|   |-- Vue 3 + TypeScript + Naive UI
|   |-- Account dashboard: overview, subscriptions, keys, usage
|   |-- Molly assistant controls and desktop settings
|   `-- Embedded tool area
|       |-- CC Switch (React application)
|       `-- GPT Image Playground (sandboxed application)
|
|-- Petra window
|   |-- Transparent, borderless, always-on-top Live2D desktop companion
|   `-- Assistant chat, model behavior, interaction and context menu
|
`-- Native Rust layer
    |-- Tauri commands, window/tray lifecycle and autostart
    |-- MollyCloud HTTPS API client and credential storage
    |-- Live2D/Petra integration and account-safe assistant tools
    |-- CC Switch integration, local tool configuration and proxy ownership
    `-- Isolated image-workbench storage and request transport
```

控制台和桌宠是同一 Tauri 应用的两个窗口。控制台负责账户数据与工具入口；Petra 窗口保持透明独立布局，不继承控制台的背景或滚动规则。

### 模块边界

| 模块 | 职责 | 数据与权限边界 |
| --- | --- | --- |
| MollyCloud 控制台 | 账户概览、订阅、用量、掩码 API 密钥、桌宠开关 | Access Token 仅在原生内存中使用；Refresh Token 存放在 Windows 凭据管理器；账户读取通过 HTTPS API 完成。 |
| Live2D / Petra | 桌宠渲染、交互、助手对话、模型和行为设置 | 独立透明窗口；桌宠助手不能直接取得账户密钥或账户令牌。 |
| 内置 CC Switch | 供应商库、工具配置、备份和本地代理 | 数据库与设备设置放在 MollyCloud 私有目录；只有用户在界面中明确启用供应商时，才会写入实际 CLI 的配置目录。不会唤起或覆盖独立 CC Switch 的数据。 |
| 生图工作台 | 图像请求、编辑、作品历史、手动 API 配置 | 在 sandbox 内运行，不能调用通用 Tauri IPC；不会读取 MollyCloud 账户 API Key，密钥由用户手动填写并在当前设备保存。 |

## 仓库结构

```text
app/
  src/                    Vue 控制台、Petra 适配和共享组件
  src-tauri/              Rust 原生层、窗口、托盘、账户与工具桥接
  public/models/          打包的 Live2D 模型资源
  third_party/Petra/      Petra 固定版本的上游源码快照
  THIRD_PARTY_NOTICES.md  发行包随附的第三方归属与许可证文本
vendor/
  cc-switch/              内置 CC Switch 前端与后端适配
  gpt-image-playground/   内置生图工作台适配
design-system/            MollyCloud 控制台设计规范
docs/                     CC Switch 与生图工作台的集成、测试边界
```

## 开发

### 环境

- Windows 10/11
- Node.js 20 或更高版本
- Rust stable 与 Windows MSVC Build Tools
- WebView2 Runtime

### 启动

```powershell
cd app
npm install
npm run dev:desktop
```

首次构建时，`build:ccswitch` 与 `build:image-workbench` 会自动安装并构建各自内嵌模块的前端依赖。

若要导入自己的 Live2D 资源，在仓库根目录执行：

```powershell
npm install
npm run import:model -- --source "C:\path\to\model" --core "C:\path\to\live2dcubismcore.min.js" --preview "C:\path\to\preview.png"
```

模型素材、Live2D Cubism SDK/Core 和相关运行时是否可以重新分发，取决于各自许可；发布前须自行确认。

### 验证与打包

```powershell
cd app
npm test
npm run check:design
npm run tauri build
```

设计检查覆盖登录页和七个控制台页面的 960x640、1180x760、1440x900 视口。安装产物位于 `app/src-tauri/target/release/bundle/`，其中 NSIS 安装器位于 `nsis/`。

## 使用的开源项目

以下项目被直接集成或构成桌面端的主要运行基础。固定版本、版权声明与完整许可证文本见 [app/THIRD_PARTY_NOTICES.md](./app/THIRD_PARTY_NOTICES.md)。

| 项目 | 用途 | 上游版本 / 许可证 |
| --- | --- | --- |
| [Tauri](https://tauri.app/) | Windows 原生应用外壳、窗口、托盘与 IPC 能力 | Tauri 2，MIT / Apache-2.0 |
| [Vue](https://vuejs.org/) + [Vite](https://vite.dev/) + [TypeScript](https://www.typescriptlang.org/) | MollyCloud 控制台与 Petra 适配前端 | 按上游许可证 |
| [Naive UI](https://www.naiveui.com/) | 控制台通用 UI 组件 | MIT |
| [PixiJS](https://pixijs.com/) + [pixi-live2d-display](https://github.com/guansss/pixi-live2d-display) | Live2D 模型渲染 | MIT |
| [Petra](https://github.com/Wumiu/Petra) | 桌宠行为、助手架构与交互能力的适配来源 | 0.2.3，提交 `9b4af14`，MIT |
| [CC Switch](https://github.com/farion1231/cc-switch) | 内置供应商和 CLI 配置管理器 | 3.20.3，提交 `d695a2d`，MIT |
| [GPT Image Playground](https://github.com/CookSleep/gpt_image_playground) | 内置图片生成与编辑工作台 | 0.7.12，提交 `da4fda8`，MIT |

上游项目的版权仍归各自作者所有。MollyCloud 对内置模块进行了宿主、存储隔离和桌面端适配，并不代表与上游项目存在官方隶属、背书或发布关系。

## 安全与隐私说明

- MollyCloud 账户凭据不交给 WebView 或模型上下文；控制台只展示原生层处理后的掩码密钥状态。
- 图片工作台不自动导入账户 API Key，也不共享控制台登录 Cookie 或账户令牌。
- CC Switch 对真实工具配置的写入始终来自用户明确操作。不同配置管理器同时管理同一个 CLI 时，最后一次启用的供应商会生效。
- 测试应使用临时用户目录和模拟凭据；不要让自动化测试修改真实 CLI 配置或运行中的桌宠。

## 许可证与归属

本仓库目前没有为 MollyCloud 自身代码声明单独的开源许可证。除非另有书面许可，请勿将 MollyCloud 品牌、服务端接口、模型素材或本项目代码视为已按开源许可证授权。

随源代码和安装包分发的第三方组件遵循其各自许可证。请在重新分发、二次开发或商业使用前阅读 [第三方声明](./app/THIRD_PARTY_NOTICES.md) 以及每个上游项目的许可证。
