# Windows 更新同步记录（Mac 先行开发）

最后更新：2026-10-04（北京时间）。本文长期保留，每次 Mac 源码推送或安装包上传同步更新；历史批次不删除。

## 使用方式与当前状态

macOS 是先行开发版。Windows 开发者或 AI 先读本文的新增批次，再对照 [完整功能交接与验收清单](./Windows功能同步交接.md) 在可运行的 Windows 源码上实现。Mac 的业务和共享视觉是功能参考；系统窗口、托盘、WebView2、凭据、音频、路径、安装器按 Windows 实现。

| 项目 | 当前基准 |
| --- | --- |
| Mac 持续开发源码 | [`codex/macos-port`](https://github.com/ititrica/mollycloud-desktop/tree/codex/macos-port) |
| Mac 当前发行 | [0.2.0 / `v0.2.0-macos`](https://github.com/ititrica/mollycloud-desktop/releases/tag/v0.2.0-macos)，Apple Silicon、macOS 13+ |
| 当前发行批次 | `MAC-20261004-02`：安装包常驻修复；版本号仍为 0.2.0 |
| Windows 原可运行基线 | `master` / `v0.2.0`，提交 `6e8d8db84de8235b01d077e95f5d87723f522e6b`；开始修改前仍需在目标机器确认可运行 |
| Windows 同步状态 | **待 Windows 实际编译和实机验证**；尚未收到通过记录 |
| 已失败的 Windows 尝试 | `codex/windows-parity`，用户反馈完全无法使用，原因未定位；不直接合并或作为验证依据 |

两个平台的 0.2.0 功能并不相同。记录功能同步时使用批次 ID、Mac 实际提交、Windows 实际提交及测试结果，不能只比较版本号。同版本覆盖会移动 Mac tag；每次开始移植必须记录当时完整提交 ID，已发布的具体提交也在 GitHub Release 说明及 `release-info.json` 中保存。

Windows 保留 NetSpeed Dynamic 灵动岛、Windows 自启动、系统音频、托盘、通知、代理、实际 CLI 路径和 WebView2。Mac 的 `LSUIElement`、AppKit、Swift / ScreenCaptureKit、LaunchAgent、ICNS 和 WKWebView 不能直接搬入 Windows。跨平台共享源码不等于 Windows 原生构建已通过。

## MAC-20261004-02：安装包常驻修复与持续维护规则

- Mac 版本：**0.2.0**，覆盖原 Mac 0.2.0 发行。
- 源码：[`v0.2.0-macos`](https://github.com/ititrica/mollycloud-desktop/tree/v0.2.0-macos)，前一批次功能参考提交为 `3b22b903449de1635d2f1fb6b77f0f01e30f3191`；发行说明记录本批次最终提交。
- 安装包：[MollyCloud_0.2.0_aarch64.dmg](https://github.com/ititrica/mollycloud-desktop/releases/download/v0.2.0-macos/MollyCloud_0.2.0_aarch64.dmg)。
- SHA-256：`8540ec3e67f593a4f0dec03e5989a2e5779333a50f5ec8328aff3b44f4a24266`；大小：26,961,098 字节。
- Windows 状态：**待移植 / 待验证**；本轮没有修改或编译 Windows 分支。

### Mac 的行为变化

此前只隐藏控制台仍可能被“关闭即退出”工具结束进程；用户机器的本地例外不能保护其他安装者。现在通过安装包中的 `LSUIElement=true` 声明菜单栏应用，打开控制台采用 `Regular`，收起采用 `Accessory`。点击红色关闭按钮后，菜单栏、桌宠当前显隐状态与后台继续保留；重新打开恢复 Dock 图标和前台控制台。无需另行申请后台运行权限，也不依赖自动退出工具中的本机例外。

偏好加载移到创建控制台之前，登录、两步验证、已登录和注销阶段均遵守保存的关闭设置。菜单栏、Dock 重开、导入链接和桌宠的控制台入口统一唤起；菜单栏不可用时保持窗口，激活策略切换失败时恢复窗口。增加隐藏失败、退出请求和最终清理日志。明确退出、Cmd+Q、退出模式及重启仍遵循正常退出，不替换系统关闭按钮，也不拦截用户明确退出。

### Windows 本轮应同步什么

| 变化 | Windows 移植要求 | 源码参考 |
| --- | --- | --- |
| 创建窗口前读取关闭偏好 | 在 Windows 实际初始化顺序中提前加载；登录前、两步验证、登录后和注销后均有效 | [`lib.rs`](../app/src-tauri/src/lib.rs)、[`console_settings.rs`](../app/src-tauri/src/console_settings.rs) |
| 统一控制台打开入口 | 托盘、桌宠、单实例重开和导入入口统一显示、取消最小化、聚焦；保留 Windows 逻辑 | 同上 `show_console` |
| 关闭后保留恢复入口 | “最小化到托盘”只隐藏窗口，托盘和桌宠继续可用；隐藏失败或无法恢复时保持可操作窗口 | 同上关闭策略；Mac 的 `tray_by_id` 保护只在 Mac 分支中实现，Windows 需自行适配 |
| 正常退出与清理 | 明确退出结束进程，清理 CC Switch 代理和临时资源；不能为常驻而禁止退出 | `handle_run_event`、`handle_window_event` |
| 回归不能仅凭退出码 | 监督测试同时检查最终完成标记，防止进程提前退出却返回 0 被误判成功 | [`console_settings_smoke.rs`](../app/src-tauri/examples/console_settings_smoke.rs)、[`verify-background-close-macos.mjs`](../app/scripts/verify-background-close-macos.mjs)，测试方法可参考，脚本本身仅适用于 Mac |
| `LSUIElement` 和激活策略 | **Mac 专用，不移植**；Windows 使用自身系统托盘与窗口机制 | [`Info.macos.plist`](../app/src-tauri/Info.macos.plist) |
| Mac 先行、同步文档随发布维护 | Windows 每轮按批次实施并回填验证结果；两份 Markdown 与 Mac 源码 / 安装包一起交付 | [`AGENTS.md`](../AGENTS.md)、[发布说明](./桌面端更新发布.md) |

### 本轮验证与 Windows 验收

Mac 已通过 110 项 Rust 单元测试、生产构建以及 6 种真实 AppKit 回归：已登录无桌宠、登录页无桌宠、隐藏桌宠、可见桌宠、菜单栏明确退出、原生明确退出。临时实例使用新的包标识，不在自动退出工具例外中；检查真实原生红色按钮、两次收起与恢复、窗口 / 菜单栏 / 主循环仍存在、桌宠显隐保持及明确退出。测试使用临时设置与模拟窗口，不重启用户桌宠。

DMG 完成校验、只读挂载、应用签名、常驻声明、导入 URL、已构建应用与挂载二进制一致性及系统库依赖检查。验证摘要随发行提供 `validation.json`，本地日志在 `artifacts/background-close-package-20261004/`；未将测试日志或用户设置提交到仓库。

Windows 要独立证明：原生窗口 / 自定义关闭按钮均进入同一策略，各登录阶段连续收起和恢复不退出，托盘和可见 / 隐藏桌宠状态不变，“退出”及明确退出正常清理。测试在临时用户目录执行，填写实际 Windows 系统、MSVC / WebView2、提交和通过项。Mac 的上述通过结果不能直接替 Windows 勾选。

本轮 GitHub 发行替换 Mac 安装包、更新清单、校验和与说明，并提供两份 Windows 文档。Windows `v0.2.0` 及 `latest.json` 独立。`latest-macos-aarch64.json` 仍使用客户端信任的 `desktop.veriolink.com` 下载域名，是更新域名的部署文件；GitHub 上传不代表该域名已部署。本轮没有上传 R2，已安装 Mac 0.2.0 用户需要手动下载覆盖安装。

## MAC-20261004-01：Windows 首次功能追平清单

本批次汇总 Mac 从原 Windows 基线到 `3b22b903449de1635d2f1fb6b77f0f01e30f3191` 的已实现功能。详细参数、异常处理与 26 项验收见 [完整功能交接](./Windows功能同步交接.md)。各项当前均为**待 Windows 实际编译和实机验证**。

| 功能领域 | Windows 要追平的目标 | Mac 提交 |
| --- | --- | --- |
| 平台能力 | 保留账户、订单、助手、支付、插件和本机工具；Windows 继续包含灵动岛，原生服务按 Windows 保留 | `1a4d422` |
| 窗口 / 菜单 / 视觉 | 横向菜单无整条分割线或可见滚动条，保持选中下划线；工作区轻阴影；Windows 标题栏按自身布局适配 | `5da4a6f` |
| 桌宠生命周期 | 登录后创建，注销销毁；手动隐藏不被刷新覆盖；余额监听先注册并补发已有数据 | `69d5aaa` |
| 桌宠外观与进度 | 浮球 / 胶囊无半透明矩形，胶囊无两处静态光斑；Codex 增量读取、两秒补偿、兼容字符串 summary | `69d5aaa` |
| API 与 CC Switch 合并 | 唯一 API 密钥入口，账户 CRUD / 分组 / 消费，自动准备私有配置，明确启用才写工具 | `453bf58` |
| Official / 多供应商 | 有官方登录的工具首次空白 Official；保留已有选择；OpenCode 等由用户手动选择添加 | `453bf58` |
| 供应商编辑 | 铅笔图标打开完整 CC Switch 编辑器，保留端点 / 鉴权 / 模型 / 映射 / 高级配置和密钥归属 | `947d5a5` |
| Codex 原生 / 映射 | 每密钥独立开关；Molly 初始 1M；分组模型与九项白名单精确交集、1050000 上下文、六档思考等级 | `947d5a5` |
| 自动登录 / 凭据 | 应用内保存邮箱、密码及刷新令牌；恢复令牌优先，确定过期才回退；注销清理，保留两步验证 | `05c0141` |
| 独立导航 / 充值 | API 密钥命名、独立提示词和 MCP，共用 CC Switch 更新；隐藏内部三个入口；充值可滚动无滚动条 | `05c0141` |
| 最终版本 | Mac 主应用及发行清单恢复为 0.2.0；平台清单分开，不能覆盖 Windows 更新源 | `c6b288f` |
| 白底图标 / 菜单顺序 | 白底品牌图标须生成 Windows ICO / PNG 并应用 EXE / 任务栏 / 托盘；Skill 紧跟 API 密钥 | `3b22b90` |
| 全屏拖动 / 系统层级 | 实际角色可达完整屏幕边缘及任务栏区域，桌宠高于普通窗口、低于任务栏 / 系统菜单 | `3b22b90` |
| 光标 / 工具主题 | 浅色黑色、深色白色文字光标；API / 提示词 / MCP / 生图统一荧光绿与共享 token；切主题保留编辑状态 | `3b22b90` |
| 文案 / 关闭 | 提示词及 MCP 移除“本机工具配置”横条；关闭到托盘在登录前后有效；并继续同步后一批次的常驻修复 | `3b22b90` + `MAC-20261004-02` |

完整目标包括映射失败 / 空交集保留旧配置、原生配置恢复、准备与应用版本防竞态、完整账户分页及换号保护、两端数据隔离、主题保活、sandbox 与手动生图密钥。不能只按表中的短标题简化实现。

## Windows 实施与回填规则

1. 保存现有工作，记录当前可用 Windows 提交并确认能编译 / 启动；不直接合并失败分支，不整套替换 Mac Cargo / Tauri / 原生启动代码。
2. 记录选定 Mac 完整提交及本文批次，按完整交接中的阶段逐项实现、测试、提交。保留 Windows 灵动岛和已保存配置。
3. 用临时账户和 CLI 目录证明自动同步只入库、明确启用才写工具；不会改动独立 CC Switch 数据或真实用户密钥。
4. 在 Windows 编译安装器并用真实 WebView2 检查各菜单、关闭 / 托盘、登录、支付窗口、桌宠、DPI / 多屏、主题与升级数据兼容。模拟付款不代表真实付款通过。
5. 在下面表格及对应批次添加实际记录。只修改源码可标“已实现，待编译”；编译通过可标“编译通过，待实机”；所有必需实机项通过后才标“完成”。未知项保持待验证，写明原因。

| Windows 同步批次 | Mac 参考提交 | Windows 基线 → 最终提交 | 环境 / 编译 | 实机结果 / 剩余问题 |
| --- | --- | --- | --- | --- |
| `MAC-20261004-01` | `3b22b903449de1635d2f1fb6b77f0f01e30f3191` | 待填写 | 待验证 | 待验证 |
| `MAC-20261004-02` | 见该批次 Release 的完整提交 | 待填写 | 待验证 | 待验证 |

## 每次上传的维护要求

Mac 每次推送源码或上传安装包前，添加新的批次，更新本文“当前状态”和完整交接中的目标行为；每条说明须包含具体变化、源码分支 / tag、Windows 移植要求、Mac 验证和 Windows 状态。仅改 Mac 平台时写明无需移植的部分，以及 Windows 仍需检查的共享行为；不能跳过记录。

同一发布提交包含两份最新文档，Release tag 指向该源码。Release 说明链接本分支上的最新记录，并上传 `Windows更新记录.md` 和 `Windows功能同步交接.md` 作为当次快照。GitHub 上传后核对远端源码、tag、DMG 哈希、清单和文档附件一致；更新服务器的部署状态另行记录。

新版本保留历史；用户明确要求同版本覆盖时，先备份旧 Mac tag、Release 元数据与附件，新增批次并记录新包哈希，注明同版本不会自动提示升级。Windows 发行、EXE 和 `latest.json` 保持独立。

未来批次沿用以下格式，新增在旧批次之前：

```markdown
## MAC-YYYYMMDD-NN：本次变化

- Mac 版本 / 日期 / 分支 / tag：
- 源码参考提交（不能自引时使用对应发行 tag，由 Release 记录最终提交）：
- 变化及受影响模块：
- Windows 应实现的行为与源码路径：
- Mac 专用部分 / Windows 应保留的实现：
- 数据兼容与更新源影响：
- Mac 实际验证 / 未验证项：
- Windows 同步状态 / 提交 / 验证：
- 发布附件及更新域名部署状态：
```
