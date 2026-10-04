# MollyCloud Desktop

MollyCloud 桌面客户端，将账户控制台、Molly AI 助手、透明桌宠和开发工具工作台整合到一个应用中。仓库包含 Windows 与 macOS 平台实现；macOS 版按平台边界移除了 NetSpeed Dynamic 灵动岛。

[Mac 安装包](https://github.com/ititrica/mollycloud-desktop/releases/tag/v0.2.0-macos) · [Windows 安装包](https://github.com/ititrica/mollycloud-desktop/releases/tag/v0.2.0) · [Windows 更新同步记录](./docs/Windows更新记录.md) · [更新日志](./CHANGELOG.md) · [开源项目与许可证](./docs/开源项目与许可证.md) · [反馈问题](https://github.com/ititrica/mollycloud-desktop/issues)

## 开发与同步约定

**macOS 是先行开发版。** 新功能和共享界面先在 `codex/macos-port` 开发、验证和发布，Windows 按 [持续更新记录](./docs/Windows更新记录.md) 与 [完整功能交接](./docs/Windows功能同步交接.md) 在可运行的 Windows 基线上逐项升级。每次推送源码或上传安装包同步更新文档，Release 也提供两份 Markdown 附件。Windows 同步完成情况以 Windows 实际编译和运行记录为准。

`master` / Windows `v0.2.0` 保留原 Windows 基线；`codex/macos-port` / `v0.2.0-macos` 是当前 Mac 参考。两个平台同为 0.2.0 不表示功能已同步。用户反馈不可用的 `codex/windows-parity` 分支不作为 Windows 升级基础。

## 下载与安装

当前 Mac 源码版本：**v0.2.0**。Mac 安装包适用于 **macOS 13+、Apple Silicon（M 系列芯片）**；Intel / universal 尚未提供经实机验证的安装包。Windows 下载仍为原 Windows 0.2.0 发行。

| 安装包 | 用途 |
| --- | --- |
| [MollyCloud_0.2.0_aarch64.dmg](https://github.com/ititrica/mollycloud-desktop/releases/download/v0.2.0-macos/MollyCloud_0.2.0_aarch64.dmg) | Mac Apple Silicon，含关闭控制台后菜单栏常驻修复 |
| [MollyCloud_0.2.0_x64-setup.exe](https://github.com/ititrica/mollycloud-desktop/releases/download/v0.2.0/MollyCloud_0.2.0_x64-setup.exe) | 推荐，NSIS 安装器 |
| [MollyCloud_0.2.0_x64_en-US.msi](https://github.com/ititrica/mollycloud-desktop/releases/download/v0.2.0/MollyCloud_0.2.0_x64_en-US.msi) | MSI 安装包，适合集中部署 |

Windows 下载后运行安装器，需要 Microsoft Edge WebView2 Runtime；如果系统缺少该运行时，按安装器提示完成安装。macOS 将 `.dmg` 中的应用拖入 Applications，使用系统 WKWebView。首次使用可通过 Windows 系统托盘或 macOS 菜单栏打开控制台并登录 MollyCloud。

发行页提供 `SHA256SUMS.txt`。可在 PowerShell 中核对下载文件：

```powershell
Get-FileHash .\MollyCloud_0.2.0_x64-setup.exe -Algorithm SHA256
```

Windows 0.2.0 安装包未进行代码签名；Mac 使用 ad-hoc 签名，尚未进行 Developer ID 签名与公证。此次按要求覆盖 Mac 0.2.0；已有 Mac 0.2.0 用户需手动下载安装，相同版本号不会触发自动更新。历史发行见 [Releases](https://github.com/ititrica/mollycloud-desktop/releases)。

## 功能

| 模块 | 功能 |
| --- | --- |
| 账户概览 | 查询余额、订阅额度、到期时间和用量；订阅卡片支持重置与自动续杯 |
| 充值与订单 | 余额充值、购买订阅、付款方式与费用预估、订单详情及待支付取消；付款在独立窗口中完成 |
| API 密钥 | 创建、复制、删除、分组切换、配置额度与有效期，查看实际累计 / 今日消费，导入内置 CC Switch |
| 提示词与 MCP | Mac 提供独立导航页面，复用内置 CC Switch 并随其更新 |
| Molly 助手 | “对话 / 设置”横向菜单，自定义 OpenAI 兼容对话端点和模型、在线朗读、本地账户查询与主动提醒 |
| 透明桌宠 | Molly 模型、拖动和互动、对话气泡、显示 / 隐藏、托盘与启动设置 |
| 灵动岛（仅 Windows） | 网速、流量、系统资源、时间、媒体与通知展示，随主应用启停；macOS 不编译或打包此模块 |
| CC Switch | 内置供应商、CLI 配置、代理、MCP、Skills 和会话管理，支持 Claude Code、Codex、Gemini CLI 等工具 |
| 生图工作台 | 生成和编辑图片、流式预览、作品历史、自定义 API 端点与手动保存密钥 |
| Skill 管理器 | 浏览、导入、安装、整理、启用与部署本机 Agent Skills |
| 功能插件 | 管理 CC Switch、生图工作台和 Skill 管理器资源包，支持安装、卸载、兼容更新与恢复上一版本 |

控制台支持浅色、深色和跟随系统，公共导航与分类切换使用统一过渡动画。

### 0.2.0 更新重点

- 生图默认配置：OpenAI 兼容接口、Images API、`gpt-image-2.5`、开启流式传输、1 张中间步骤图像。已有个人配置继续保留。
- 修复大图片流式响应末尾截断，确认全部分块完整交付后才结束传输。
- 在线朗读支持 MiMo V2.5 TTS，预置冰糖音色与低延迟流式播放；也支持自定义 OpenAI `/audio/speech` 服务。文字与首段有效音频同步开始。
- 余额、订阅额度与到期查询通过固定接口和本地规则完成，不消耗对话模型 token。低余额、低额度和即将到期提醒按条件去重。
- 优化助手、密钥、订阅和充值布局，修复密钥累计 / 今日消费统计。

完整改动（含此前本地版本的功能）见 [CHANGELOG.md](./CHANGELOG.md)。

## 使用配置

**对话与语音**：打开“Molly助手 → 设置”配置。语音朗读默认关闭，需要填写独立 TTS 密钥，试听后开启并保存。MiMo 默认模型为 `mimo-v2.5-tts`；自定义语音模式可设置端点、模型和音色。账户查询本身不调用对话模型；开启朗读后，语音合成仍会产生所选 TTS 服务的用量。详见 [在线 TTS 接入方案](./docs/在线TTS接入方案.md)。

**生图**：首次打开工作台后手动填写 API Key。默认 Base URL 为 `https://mollycloud.cn/v1`；端点、模型和流式参数均可修改。工作台不会自动读取 MollyCloud 账户 API 密钥。

**API 密钥 / CC Switch**：Mac 在“API 密钥”中统一管理账户密钥与供应商，自动同步只入私有库，点击“启用 / 应用配置”后才写本机工具；完整编辑器保存当前已启用供应商沿用 CC Switch 的即时应用行为。Codex 支持逐密钥“原生 / 映射”和 MollyCloud 1M 上下文默认值，Official 保留官方登录。OpenCode 等累加供应商工具由用户手动选择添加。提示词和 MCP 使用独立导航，共用 CC Switch 插件；Skill 管理器紧跟 API 密钥。Windows 的移植要求见同步文档。

**更新**：GitHub Releases 提供源码与安装包。Windows 更新检查使用 `https://desktop.veriolink.com/latest.json`；macOS 使用对应架构的 `latest-macos-aarch64.json` / `latest-macos-x86_64.json`，仅接受匹配架构的 DMG。GitHub Release 发布与这些更新源独立，发布到 GitHub 不会自动更新 R2。详见 [桌面端更新发布](./docs/桌面端更新发布.md) 和 [macOS 本地归档](./docs/macOS开发.md#本地发行归档)。

## 开发与构建

### 环境要求

- macOS：macOS 13+、Xcode Command Line Tools、Node.js 22.12+、Rust stable；Apple Silicon 与 Intel 分别使用对应目标。详见 [macOS 开发](./docs/macOS开发.md)。
- Windows 10/11 x64 与 WebView2 Runtime。
- Node.js **22.12 或更高版本**，建议使用受支持的 LTS 版本。
- Rust stable；Windows 使用 MSVC 工具链，macOS 使用 Apple 工具链。
- Windows 需要 Visual Studio Build Tools 的“使用 C++ 的桌面开发”工作负载与 Windows SDK。
- Git；首次安装 npm / Cargo 依赖及获取安装器工具时需要网络。

### Windows 获取源码并启动

以下默认分支为原 Windows 基线。同步 Mac 功能前先确认该版本在 Windows 可运行，再按 [交接文档](./docs/Windows功能同步交接.md) 分阶段升级。

```powershell
git clone https://github.com/ititrica/mollycloud-desktop.git
cd mollycloud-desktop\app
npm.cmd ci
npm.cmd run dev:desktop
```

`dev:desktop` 启动 Tauri 开发应用，前端位于 `http://localhost:24320`。如果该端口运行的是当前项目的 Vite，会复用现有服务；端口被其他应用占用时会提示冲突。

首次启动会分别安装并构建 CC Switch、生图工作台和 Skill 管理器的前端。只预览前端可运行 `npm.cmd run dev`；浏览器预览中的模拟数据不代表原生接口已完成验证。

### Windows 验证与打包

在 `app` 目录执行：

```powershell
npm.cmd test
npm.cmd run build
npm.cmd run tauri -- build
```

`tauri build` 会自动执行前端生产构建和安装资源检查，产物在：

```text
app/src-tauri/target/release/bundle/nsis/
app/src-tauri/target/release/bundle/msi/
```

UI 开发服务运行时，执行 `npm run check:design`。当前 Mac 检查覆盖登录、七个控制台入口及子页、960×640 / 1180×760 / 1440×900；Windows 功能同步后需覆盖八项入口（含灵动岛）。截图输出到 `artifacts/design-review/`。设计规范见 [MASTER.md](./design-system/molly-desktop/MASTER.md)。回归测试使用临时用户目录和模拟凭据。macOS 的构建和安装命令见 [macOS 开发](./docs/macOS开发.md)。

模型导入脚本也在 `app` 目录执行：

```powershell
npm.cmd run import:model -- --source "C:\path\to\model" --manifest "model.model3.json" --core "C:\path\to\live2dcubismcore.min.js" --preview "C:\path\to\preview.png"
```

模型图片、PSD、Live2D 模型和 Cubism Core 各有独立授权边界，见下方许可证说明。

## 架构与仓库结构

```text
app/
  src/                    Vue 控制台、Petra 助手和公共组件
  src-tauri/              Rust 原生接口、窗口、托盘、凭据和工具桥接
  public/models/          桌宠模型资源
  plugins/                功能插件定义
  third_party/Petra/      Petra 上游源码快照
  THIRD_PARTY_NOTICES.md   第三方归属与许可证文本
vendor/
  cc-switch/              React 前端、Rust 后端和嵌入适配
  gpt-image-playground/    隔离生图工作台及桌面流式传输适配
  skills-manager/         Skill 管理器前端、后端及嵌入适配
  netspeed-dynamic/        灵动岛及固定辅助组件
design-system/           控制台公共设计规范
docs/                    集成、开发、发布和开源归属文档
```

应用由 Vue 3 / TypeScript / Naive UI 控制台、独立透明 Petra 窗口、Windows 专用灵动岛窗口，以及 Tauri 2 / Rust 原生层组成。内嵌 React 工具独立构建，样式与宿主分离；生图工作台通过受限消息桥接通信，保持 sandbox 来源隔离。

### 数据与权限边界

- 账户令牌留在原生层；Mac 的自动登录资料、刷新令牌及应用管理的密钥使用私有目录中的 AES-GCM 加密存储，不再调用钥匙串。原 Windows 基线使用凭据管理器 / DPAPI，升级需保留旧数据并按同步文档实现同样的保存体验。对话与 TTS 使用独立配置。
- 生图 API Key 由用户手动填写，工作台使用独立存储，不共享账户 Cookie、令牌或通用 Tauri IPC。
- CC Switch 的供应商库、设备设置、备份及代理归属保持私有；用户明确操作的工具配置、MCP、Skills、会话和环境变量管理作用于本机真实工具。
- 支付验证使用订单专属隔离 WebView（Windows WebView2 / macOS WKWebView），会话退出时清理；金额、可购状态与支付结果以服务端确认为准。
- 在线对话、生图、TTS 会向用户配置的服务发送相应内容；账户查询及提醒使用本地规则访问 MollyCloud 账户接口。

## 使用的开源项目

感谢以下项目的作者与贡献者。版本和提交对应仓库保留的上游快照，MollyCloud 的嵌入版本含本地适配。

| 项目 | 用途 | 固定上游版本 / 许可证 |
| --- | --- | --- |
| [Petra](https://github.com/Wumiu/Petra) | 桌宠行为、交互与助手架构 | 0.2.3 · `9b4af14` · MIT |
| [Anime2.5DRig](https://github.com/852wa/Anime2.5DRig) | PSD 装配与 2.5D 运行时，随 Petra 引入 | Petra 快照内置版本 · MIT |
| [CC Switch](https://github.com/farion1231/cc-switch) | 开发工具供应商、配置与代理管理 | 3.20.4 · `43e1d99` · MIT |
| [GPT Image Playground](https://github.com/CookSleep/gpt_image_playground) | 生图与图片编辑工作台 | 0.7.12 · `da4fda8` · MIT |
| [Skills Manager](https://github.com/xingkongliang/skills-manager) | Agent Skills 管理 | 1.40.0 · `6ae02e3` · MIT |
| [NetSpeed Dynamic](https://github.com/GEORGEWWWU/NetSpeed-Dynamic) | Windows 专用灵动岛、网速、媒体与通知 | 2.4.6 · `422931f` · MIT |
| [Sub2API](https://github.com/Wei-Shaw/sub2api) | 供应商品牌图标组件的适配来源 | `458b92a` · LGPL-3.0 |
| [Tauri](https://github.com/tauri-apps/tauri) / [Wry](https://github.com/tauri-apps/wry) | 原生外壳、窗口、WebView 与 IPC | Tauri 2 / Wry 0.55 · MIT OR Apache-2.0 |
| [Vue](https://github.com/vuejs/core) / [Pinia](https://github.com/vuejs/pinia) / [Naive UI](https://github.com/tusen-ai/naive-ui) | 控制台组件与状态管理 | MIT |
| [React](https://github.com/facebook/react) | 内嵌工具前端 | MIT |
| [PixiJS](https://github.com/pixijs/pixijs) / [pixi-live2d-display](https://github.com/guansss/pixi-live2d-display) / [ag-psd](https://github.com/Agamnentzar/ag-psd) | 模型渲染与 PSD 读取 | MIT |
| [Vite](https://github.com/vitejs/vite) / [TypeScript](https://github.com/microsoft/TypeScript) / [Vitest](https://github.com/vitest-dev/vitest) | 构建、类型检查与测试 | MIT / Apache-2.0 / MIT |

更多直接依赖、锁定版本、Rust 依赖、原始许可证路径和本地适配说明见 [开源项目与许可证](./docs/开源项目与许可证.md) 与 [第三方声明](./app/THIRD_PARTY_NOTICES.md)。上游版权归原作者所有，不代表上游项目对 MollyCloud 的官方背书。

## 许可证

MollyCloud 自身代码目前未声明独立开源许可证；源码公开不等于授予任意使用、修改或再分发许可。第三方开源组件按各自许可证授权，品牌、模型素材及服务端接口不因此获得额外授权。

Live2D Cubism Core 是 **Live2D 专有运行时**，并非 MIT 开源组件，适用其文件头引用的 [Live2D 许可协议](https://www.live2d.com/eula/live2d-proprietary-software-license-agreement_en.html)。模型和美术资源按各自授权使用。

## 相关文档与反馈

- [CC Switch 内置方案](./docs/CCSwitch内置方案.md)
- [macOS 开发与验证](./docs/macOS开发.md)
- [Windows 持续更新同步记录](./docs/Windows更新记录.md)
- [Windows 完整功能同步交接与验收](./docs/Windows功能同步交接.md)
- [生图工作台集成方案](./docs/GPTImagePlayground集成方案.md)
- [Skill 管理器集成方案](./docs/Skill管理器集成方案.md)
- [灵动岛内置方案](./docs/灵动岛内置方案.md)
- [功能插件方案](./docs/功能插件方案.md)
- [充值与密钥管理](./docs/客户端充值与密钥分组.md)
- [项目开发交接](./docs/项目开发交接.md)

请在 [Issues](https://github.com/ititrica/mollycloud-desktop/issues) 中附上应用版本、操作系统版本与架构（Windows / macOS，x64 / Apple Silicon）、复现步骤和相关截图；提交前移除 API Key、登录令牌和私人账户信息。
