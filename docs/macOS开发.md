# MollyCloud macOS 开发与打包

macOS 版保留账户、订阅、用量、充值与订单、API 密钥、Molly 助手、透明桌宠、CC Switch、生图工作台、Skill 管理器和功能插件。按用户要求排除 NetSpeed Dynamic：没有灵动岛入口、窗口、原生插件或 Windows 辅助 EXE。Windows 仍通过独立平台配置保留原功能。

## 开发环境

- macOS 13 或更高版本。
- Xcode / Command Line Tools：`xcode-select --install`。
- Node.js 22.12 或更高版本、npm、Rust stable 和 Git。
- Apple Silicon 默认构建 `aarch64-apple-darwin`；Intel 默认构建 `x86_64-apple-darwin`。

首次下载 npm / Cargo 依赖需要联网。Mac 使用系统 WKWebView，无需安装 WebView2。

```sh
cd app
npm ci
npm run dev:desktop
```

开发服务位于 `http://localhost:24320`。`dev:desktop` 会复用此项目已运行的 Vite，冲突时停止启动并报告端口；不会停止其他服务。内嵌工作台首次运行时自动安装与构建各自前端依赖。

仅预览前端运行 `npm run dev`。Vite 通过 Tauri 目标平台或当前主机选择构建入口；跨平台前端预览可显式设置 `MOLLY_TARGET_PLATFORM=windows`。浏览器演示与原生功能验证独立，预览不读真实密钥或操作桌宠。

## 验证与打包

```sh
cd app
npm test
npm run build
npm run package:macos
```

`package:macos` 在 Mac 上调用 Tauri，生成当前架构的 `.app` 与 `.dmg`，并自动执行前端构建及安装资源检查。标准产物位于：

```text
app/src-tauri/target/release/bundle/macos/MollyCloud.app
app/src-tauri/target/release/bundle/dmg/MollyCloud_0.2.0_aarch64.dmg
```

Intel 文件名中的架构为 `x64`。指定 `--target` 后产物位于对应的 `target/<目标>/release/bundle/`。

通用架构需要同时安装两个 Rust 目标，再传入 Tauri 的 universal 目标：

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run package:macos -- --target universal-apple-darwin
```

本机架构构建不等同于 Intel 或 universal 的发行验收，后两者应在对应机器确认运行。透明 Petra 窗口开启 Tauri 的 `macOSPrivateApi`；这套构建面向直接分发 `.app` / `.dmg`，不面向 Mac App Store。

开发服务运行时可执行：

```sh
npm run check:design
npm run check:design -- --dark
```

检查脚本使用临时 Chrome / Edge 用户目录，覆盖登录、六项 Mac 控制台入口与子页，以及 960×640、1180×760、1440×900。截图和报告位于 `artifacts/design-review/`。通过 `MOLLY_BROWSER_PATH` 可指定浏览器可执行文件。灵动岛专项仅在 Windows 运行。

## 安装与平台行为

打开 `.dmg`，将 MollyCloud 拖入 Applications。开发构建也可直接运行生成的 `.app`。默认采用 ad-hoc 本地签名，封装后可通过 `codesign --verify --deep --strict` 检查。尚未进行 Developer ID 签名和 Apple 公证的本机构建可能被 macOS 拦截；确认来自自己的构建后，可在 Finder 中打开，按系统提示允许运行。正式向用户分发前应按 [Tauri macOS 签名指南](https://v2.tauri.app/distribute/sign/macos/) 配置开发者签名与公证；`APPLE_SIGNING_IDENTITY` 环境变量可覆盖本地签名身份。

| Windows 功能 | macOS 实现 |
| --- | --- |
| 系统托盘、窗口显示 / 隐藏 | 菜单栏状态菜单；保留控制台关闭策略、桌宠显隐与退出 |
| 控制台窗口外观 | 原生圆角与系统阴影、红黄绿按钮；44px Overlay 标题栏，侧栏切换按钮位于绿色按钮右侧 |
| 开机自启动 | 用户登录后的 LaunchAgent；在 MollyCloud 设置中启用 |
| 登录令牌 | macOS 钥匙串；不保存登录密码 |
| 助手与语音密钥 DPAPI | 钥匙串保存本机随机密钥，AES-GCM 加密私有配置 |
| 启动本机应用 | 扫描 Applications 并通过 macOS `open` 启动应用 |
| CMD 查询命令 | 直接执行受限 macOS 查询程序，无 shell 解释、管道或重定向 |
| 音量、通知、定时关机 | macOS 系统接口；通知或关机授权由系统处理 |
| 桌宠音频互动 | ScreenCaptureKit；默认关闭，用户开启后授权，不保存或上传音频 |
| 灵动岛及辅助 EXE | 不包含 |

`Option+P` 是菜单栏菜单的显示 / 隐藏加速键，不是系统全局快捷键。重新打开应用会唤起已运行的控制台。自启动最小化仅适用于带专用参数的登录启动，手动打开始终显示控制台。

应用私有数据与插件保存在 `~/Library/Application Support/cn.mollycloud.client/`。CC Switch 供应商库、设置与备份继续使用 MollyCloud 私有目录；工具配置、MCP、Skills、会话作用于用户明确选择的实际本机工具。API 密钥导入只入库，点击启用后才写工具配置。内置导入 URL 使用 `mollycloud://ccswitch/import`，不会注册独立版 `ccswitch://`。

生图工作台在 Mac 使用独立 `molly-image://localhost` 来源，保留 sandbox 与 MessageChannel，不授予同源权限或通用 Tauri IPC。生图 API 密钥仍由用户手动填写并持久保存，不能自动导入账户密钥。支付窗口保持与账户控制台隔离，付款结果以服务端状态为准。

## 本地发行归档

本地归档命令只复制安装包、计算 SHA-256 和生成更新清单，不上传或发布：

```sh
npm run release:macos -- \
  --installer src-tauri/target/release/bundle/dmg/MollyCloud_0.2.0_aarch64.dmg \
  --out ../output/release/macos-aarch64
```

输出包含 DMG、`SHA256SUMS.txt` 与 `latest-macos-aarch64.json`。Intel 构建生成 `latest-macos-x86_64.json`；universal DMG 同时生成两个架构清单。版本必须与前端 / Tauri 一致。

Mac 更新分别读取 `https://desktop.veriolink.com/latest-macos-aarch64.json` 与 `latest-macos-x86_64.json`，只接受匹配架构或 universal 的 DMG。原 Windows `latest.json` 与 EXE 更新源保持独立。此源码改造不会自动部署这些清单。

配置依据：[Tauri 平台配置](https://v2.tauri.app/reference/config/#platform-specific-configuration)、[macOS App Bundle](https://v2.tauri.app/distribute/macos-application-bundle/)、[macOS 配置项](https://v2.tauri.app/reference/config/#macconfig)。

## 本机验证记录（2026-10-03）

本次在 Apple Silicon Mac 上完成以下检查；测试使用临时用户目录、模拟凭据和本机 mock 服务，不操作真实付款、用户工具凭据或已有桌宠。

- 104 项 MollyCloud 原生单元测试通过。
- 86 项前端测试、6 项 Node 脚本测试和 CC Switch 16 项嵌入前端测试通过；完整前端生产构建、public 179 / dist 192 文件安装资源检查通过。
- 深浅色各 84 组 UI 回归通过，覆盖登录、六项 Mac 入口与子页和三种窗口尺寸，失败与运行时异常均为空；截图已查看。
- WKWebView 中 CC Switch 原生 smoke 的 23 项检查及初始化失败分支通过；Skill 管理器 35 项原生流程通过；控制台登录页关闭退出、已登录关闭隐藏和退出行为通过。
- 独立 WKWebView 支付原生验证通过：会话隔离、独立非模态窗口、弹窗共享临时存储且关闭处理器独立、主题与重载、原生及 JS 关闭后的资源清理。未进行实际付款。
- 桌宠窗口 25 项原生检查通过，包括透明、非手动缩放、程序调整大小、工作区、鼠标坐标、穿透、置顶及平滑移动。本机显示器缩放为 1；Retina、混合 DPI 和多屏仍需实机验收。
- 生图工作台 15 项 WKWebView 检查通过：实际 Mac 自定义来源、opaque iframe、拒绝通用账户 IPC、MessageChannel 握手、手动 mock 密钥、完整 3 MB 响应与尾部、默认账户密钥不导入、切页保活。全部请求仅访问本机 mock。
- 最终 ARM `.app` / `.dmg` 已成功生成，应用 ad-hoc 签名、DMG 校验和、只读挂载后的应用签名均通过；签名后的应用可启动。依赖仅来自系统库，不需要 Homebrew。未执行 Developer ID 签名、公证或网络发布。

安装包归档位于 `output/release/macos-aarch64/`，包含 `.app`、`.dmg`、SHA-256 和本地更新清单。原生验证日志位于 `artifacts/macos-port/`，界面回归报告位于 `artifacts/design-review/`。

真实 MollyCloud 登录、付费 API、付款、TTS 音色、系统音频授权、Intel / universal 运行与签名发行仍需各自环境验收。UI mock 和单元测试不代表这些实际服务已经验证。

### 2026-10-03 原生窗口与菜单外观调整

控制台启用原生窗口装饰及 Overlay 标题栏，以系统圆角、阴影与红黄绿按钮替代 Mac 的 HTML 窗口控制。菜单栏切换按钮与原生按钮垂直居中，并位于绿色按钮右侧。公共工作区增加低对比度阴影；共享横向菜单移除底部分割线和可见滚动条，保留选中下划线、触控板和键盘滚动。

深浅色各 84 组完整界面检查通过，覆盖登录、六项 Mac 导航及全部子页、960×640 / 1180×760 / 1440×900；导航动画专项通过。已查看助手、概览和登录截图。隔离原生窗口使用实际系统关闭按钮，确认按钮位置、Overlay 与阴影，以及已登录隐藏 / 退出、登录页退出和偏好持久化。证据在 `artifacts/window-chrome-20261003/`。验证使用临时窗口与模拟数据，保留 `/Applications/MollyCloud.app` 中正在运行的用户桌宠。


2026-10-04 桌宠修正：控制台进入登录后的工作区时才创建 Petra 窗口，登录及两步验证期间不加载；注销停止窗口、捕获与移动线程的任务，重新登录恢复本地设置。手动隐藏不会被控制台刷新覆盖。悬浮入口和详情取消背景模糊，任务胶囊取消两处光斑。macOS 拖动按实际角色边界限制，允许透明留白探出屏幕；窗口原生约束仅对登记的 Petra 实例放开，控制台仍遵循系统约束。Codex 文件通知之外每两秒检查近期会话的修改元数据，仅增量读取变化内容，通知不可用时仍继续同步；兼容桌面版异形 summary 字段，不读取账户密钥。

107 项 Rust、86 项前端和 6 项脚本测试通过；30 项原生窗口检查包含四角拖动、尺寸、穿透、置顶、平滑移动及控制台约束隔离；11 项启动检查包含登录前无窗口、登录后创建、手动隐藏、注销与重新登录；原生关闭策略回归通过。桌宠 UI 的 13 组记录无失败或运行时错误，包含账户详情、胶囊宽度/暗背景、Codex 步骤/等待/完成恢复和设置保存失败。截图已检查，证据在 `artifacts/pet-fixes-20261004/`。测试未重启已安装的桌宠；本机仍为 1× 显示器，多屏与 Retina 的实机边界沿用上述验收限制。
