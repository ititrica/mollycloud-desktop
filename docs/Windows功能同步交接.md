# MollyCloud：将可用的 Windows 版本更新到当前 macOS 功能的 AI 交接文档

最后更新：2026-10-04。本文是持续维护的完整功能目标与验收清单；逐次变化、Windows 同步状态和发布规则见 [Windows 更新记录](./Windows更新记录.md)。macOS 作为先行开发版，Windows 从可用源码逐项升级。

## 1. 交给 Windows AI 的任务

> 请以当前可正常运行的 MollyCloud Windows 源码为基础，按照本文逐项实现功能，使共享业务和视觉效果与当前 macOS 版本一致，保留 Windows 灵动岛和原有 Windows 系统能力。先确认旧版本可以在本机编译、启动，再分阶段实现、测试和提交。参考 macOS 的 Vue / React 界面、业务规则与测试，Windows 原生窗口、系统 API、WebView2、凭据和工具路径按 Windows 实现。不要直接合并已失败的 `codex/windows-parity` 分支，也不要整套替换 Cargo、Tauri 配置或原生启动代码。保留本地已有修改。交付时逐项报告 Windows 上的编译和运行结果，未验证的功能明确标记。

参考仓库：[ititrica/mollycloud-desktop](https://github.com/ititrica/mollycloud-desktop)。

| 用途 | 源码基准 |
| --- | --- |
| 原 Windows 0.2.0 基线 | `6e8d8db84de8235b01d077e95f5d87723f522e6b`，`master` / `v0.2.0` |
| 当前 macOS 发行参考 | `v0.2.0-macos`，2026-10-04 安装包常驻修复批次；持续开发分支 `codex/macos-port` |
| 本文完整功能梳理基准 | `3b22b903449de1635d2f1fb6b77f0f01e30f3191`，并追加下文第 8 节的安装包常驻修复 |
| 已失败的同步尝试 | `codex/windows-parity`；用户反馈 Windows 上完全无法使用，失败原因尚未定位 |

若用户的可用 Windows 工作区比上述基线更新，优先保留它，以本文补齐缺失功能。**本文的目标行为来自当前 Mac；失败同步分支的 Windows 适配不能作为已经验证的实现。**

下文源码路径均相对于仓库根目录；Windows 基线里可能尚无这些文件。可在另一个目录只读查看 [当前 Mac 发行源码](https://github.com/ititrica/mollycloud-desktop/tree/v0.2.0-macos) 或 [持续开发源码](https://github.com/ititrica/mollycloud-desktop/tree/codex/macos-port)，按模块移植所需逻辑。同版本覆盖会移动 Mac tag，Windows 开始同步时须记录实际完整提交 ID，后续对照逐次更新记录检查新增变化。

## 2. 修改前必须了解的项目边界

- 先读 Windows 工作区的 `AGENTS.md` 和 Mac 参考中的 `design-system/molly-desktop/MASTER.md`。颜色、字体、圆角、公共导航与外框复用 `app/src/design-tokens.css`、`app/src/theme.ts`、`app/src/styles.css`。
- CC Switch 私有供应商库、设备设置、备份保存在 MollyCloud 应用数据目录，代理使用独立端口；不覆盖独立版 CC Switch 的数据库或设置。用户主动启用供应商、管理 MCP / 提示词 / Skills 时，作用于实际本机工具配置。
- API 密钥同步、配置准备和模式切换只写私有供应商库；“启用 / 应用配置”才写本机工具。完整供应商编辑器保存已启用供应商的行为见第 4 节。
- 生图工作台保留 sandbox 来源隔离和专用消息桥。密钥由用户手动填写并持久保存，不能自动读取 MollyCloud 账户密钥，不能加 `allow-same-origin` 或通用 Tauri IPC。
- Petra 桌宠使用独立透明窗口；控制台的背景、圆角裁切、最小尺寸、滚动条和主题样式不能全局套到桌宠。
- 测试使用临时用户目录、模拟账户与密钥。不要为测试重启用户正在运行的桌宠，或覆盖真实 CLI / 独立 CC Switch 数据。

参考文档：`docs/CCSwitch内置方案.md`、`docs/GPTImagePlayground集成方案.md`、`docs/功能插件方案.md`。历史记录中的菜单数量、凭据方案和版本号可能已过时；当前目标以下文及固定 Mac 提交为准。

## 3. 导航、窗口与公共界面

### 3.1 菜单调整

Windows 顶级菜单目标顺序：

1. 概览
2. API 密钥
3. Skill 管理器
4. 提示词管理
5. MCP 管理
6. Molly助手
7. 灵动岛（NetSpeed Dynamic，Windows 保留）
8. 生图工作台

“概览”继续包含账户概览 / 订阅 / 用量 / 充值。Mac 不包含灵动岛，因此有七项；Windows 不能照搬七项菜单删除灵动岛。

API 密钥与 CC Switch 已合并，只保留一个名为“API 密钥”的入口。概览中点击密钥数量也进入这个页面。原独立 API 密钥页不再作为另一套入口出现。

提示词与 MCP 是独立菜单界面，但内部复用同一个 CC Switch 插件与 iframe，切换 `providers` / `prompts` / `mcp` 视图。切换菜单不重载 iframe、不丢失编辑内容。它们随 CC Switch 插件一起更新，不拆成独立上游或另一套后端。

内嵌 CC Switch 顶部移除左上角品牌文字，以及 Skills、提示词管理、MCP 管理的快捷入口，重新安排顶部按钮间距。保留独立的 Molly Skill 管理器，位置紧跟 API 密钥；不以 CC Switch 内的隐藏 Skills 入口替代它。

提示词与 MCP 页面删除这整段说明：

> 本机工具配置 · 导入只保存供应商；点击启用后，配置将用于本机工具。切换后请重新打开对应 CLI。

保留 API 密钥页面中必要的启用语义和现有功能。

参考：`app/src/App.vue`、`app/src/components/CcSwitchPanel.vue`、`vendor/cc-switch/frontend/src/App.tsx`、`vendor/cc-switch/frontend/src/embedded/bridge.ts`、`core.ts`。

### 3.2 横向菜单、滚动与阴影

- Molly助手“对话 / 设置”等横向菜单移除整条底部分割线，隐藏横向滚动条；保留选中项的荧光绿下划线。
- 公共横向菜单统一复用 `.molly-subnav` 与导航动画，保留窄窗口中的横向滚动、触控板及键盘操作。
- 充值页面保留纵向滚动，但不显示滚动条；不要用 `overflow: hidden` 使下方内容不可达。
- 公共工作区增加轻微阴影：浅色 `-3px 0 12px rgba(35, 40, 28, 0.045)`；深色 `-3px 0 12px rgba(0, 0, 0, 0.14)`。使用共享 `--shadow-workspace`。
- 保留工作区两个上角 16px、下角直角和右侧 6px 外框留白；各菜单的公共侧栏、顶部栏和背景保持一致。

参考：`app/src/subnav.css`、`scrollbars.css`、`styles.css`、`design-tokens.css`。

### 3.3 原生窗口差异与白底图标

Mac 已改为系统装饰窗口、原生圆角和阴影、真实红黄绿关闭/最小化/缩放按钮。Mac 顶部栏为 44px，侧栏收起按钮在绿色按钮右侧（左距 92px、上距 6px）。

Windows 保留自身标题栏习惯：当前设计规范是 32px 顶部栏，右侧最小化 / 最大化或还原 / 关闭，左侧收起菜单按钮。不复制 Mac 的红黄绿按钮、AppKit 或 Overlay 坐标。若调整 Windows 外框圆角和阴影，使用 Windows 支持的实现，并实测普通、最大化、贴靠和 Windows 10/11 的兼容行为。任何关闭按钮都必须进入同一个关闭策略。

应用图标已将原品牌图形置于纯白底。Mac 图形源为 `app/src-tauri/icons/macos/source.svg`，生成的 PNG / ICNS 在同目录。Windows 需要从该图形生成实际 EXE / 任务栏 / 托盘 / 安装器使用的 ICO / PNG，并核对 Tauri 与 NSIS 引用；单改网页 favicon 不算完成。

## 4. API 密钥与内嵌 CC Switch 合并

### 4.1 账户密钥管理

供应商卡片下方展示 MollyCloud 账户密钥，支持：

- 自动读取登录账户的密钥及其状态、分组和可用工具类型。
- 创建、复制、删除密钥，更换分组；保留原有创建参数和账户管理能力。
- 按名称或分组搜索、手动刷新、定时刷新。
- 显示分组倍率、累计实际消费、今日实际消费及存在时的额度用量。
- 复制 API 端点；根据状态与分组兼容性启用对应配置。

原来的“配置模型”文字按钮改成铅笔编辑图标，打开 **完整 CC Switch 供应商编辑器**，包括名称、端点、鉴权、模型、模型映射和高级配置，不能只弹出模型下拉框。未准备配置的密钥可在点击编辑时创建私有草稿。编辑完成及时刷新密钥行；即使供应商 ID 被编辑，仍能按账户 ID 与密钥 ID 找回归属。

账户列表和跨 iframe 导航消息使用脱敏密钥。复制操作由原生后台按密钥 ID 获取并写剪贴板；不要把完整密钥放进 URL、导航消息或日志。完整供应商编辑器仍沿用 CC Switch 自身的配置读取权限。

同步须完整读取分页，并验证登录账户未变。分页不完整、网络失败或同步期间注销 / 换号，不能把临时空列表当作删除依据。仅清理对应账户的私有供应商记录；不自动清空正在使用的本机工具文件，当前密钥被删除时提示用户选择其他配置。

参考：`app/src-tauri/src/molly_keys.rs`、`vendor/cc-switch/backend/src/molly_keys.rs`、`provider.rs`、`embedded.rs`；前端 `components/providers/MollyKeyDirectory.tsx`、`ProviderList.tsx`、`EditProviderDialog.tsx`；宿主 `KeyGroupDialog.vue`、`KeyGroupPicker.vue`、`CcSwitchImportDialog.vue`。

### 4.2 Official 与自动准备

Codex、Claude Code / Desktop 等有官方登录的工具，在首次没有已有配置时提供空白 Official 并默认选中；Official 不填 Molly 密钥、不读取或复制官方登录凭据。当前 Mac 也为 Gemini 与 Grok 提供相应 Official。

已存在的供应商选择、模型和高级配置继续保留，重复刷新不能强制切回 Official。即使账户密钥网络请求失败，本地 Official 仍应可用。

兼容当前工具、状态有效的 MollyCloud 密钥自动读取分组模型，准备为私有供应商选项；模型请求失败可手动编辑。**自动准备不代表自动启用。**

OpenCode 等支持累加多个供应商的工具，不自动创建 Official、不自动把全部账户密钥加入本机配置、不自动选择默认模型。用户点击“选择并添加”，选择密钥与模型后确认添加；移除只处理选择的供应商，保留其他供应商和默认模型。当前 Mac 同类处理还包括 OpenClaw、Hermes、Pi、MiniMax。

### 4.3 启用和写入边界

自动同步、创建草稿、切换原生 / 映射、模型自动刷新，都只更新 Molly 的私有供应商库。

用户点击“启用”、已启用密钥的“应用配置”，或确认“选择模型并启用 / 选择并添加”，才通过 CC Switch 写实际 CLI 文件。配置准备后显示“待应用”，不能被旧的本机配置回填覆盖；若应用期间后台准备了更新配置，不能把新版本错误标记为已应用。

完整供应商编辑器保存 **当前已启用的供应商** 时，沿用上游 CC Switch 的即时应用行为；不要为了统一按钮描述破坏这一既有能力。

保留本机工具路径设置、代理归属、原始文件备份和外部修改保护。不能改写用户原有 Codex `auth.json` 登录策略，也不能仅凭该文件存在判断当前供应商已启用。

## 5. Codex 的 1M 上下文与每密钥模式

### 5.1 原生模式

原来已有的 Codex 配置称为“原生”，首次默认采用此模式。MollyCloud Codex 供应商初始化时默认补齐：

```toml
model_context_window = 1050000
model_auto_compact_token_limit = 900000
```

首次初始化保留用户显式设置的上下文值；初始化后，在原生模式中手动关闭 1M 或修改配置，不应被每分钟同步重新开启。Official 和其他来源的供应商不强制套用 Molly 默认值。

### 5.2 映射模式

每个 **Codex 账户密钥** 独立显示“原生 / 映射”切换开关，并持久化该密钥的选择。此开关目前针对 Codex，不要求所有其他工具都有同一模式。

开启映射时，原生后台使用该密钥请求 `https://mollycloud.cn/v1/models`，读取该密钥当前分组实际可用的模型。自动刷新周期为 60 秒，用户手动刷新也重新拉取。

仅将实际返回 ID 与以下白名单的 **精确交集** 写入映射，按白名单顺序排列并去重；不能使用前缀、模糊匹配、别名或直接写入全部白名单：

```text
gpt-5.6-luna
gpt-5.6-terra
gpt-5.6-sol
gpt-6-luna
gpt-6-sol
gpt-6-astra
gpt-6-astra-fast
gpt-6.1-sol
gpt-6.1-sol-fast
```

私有供应商的 `settingsConfig.modelCatalog.models` 中每个模型使用如下字段；示例模型也必须由服务实际返回才能加入：

```json
{
  "model": "gpt-6.1-sol",
  "displayName": "gpt-6.1-sol",
  "contextWindow": 1050000,
  "reasoningLevels": ["low", "medium", "high", "xhigh", "max", "ultra"],
  "defaultReasoningLevel": "high"
}
```

- 当前默认模型仍在交集内则保留，否则选交集首项。
- 拉取失败或交集为空：保留已有配置、显示原因，不写空映射，不伪造模型；错误状态不能无提示启用待处理映射。
- 分组改变后按新分组重新发现，避免继续把旧分组模型当作可用模型。
- 切回原生恢复切换前的默认模型、上下文覆盖、原始映射目录和目录指针；保留鉴权、端点、TOML 注释及其他高级设置。
- 映射准备仍只入私有库；点击启用 / 应用后，由 CC Switch 的模型目录序列化逻辑写本机 Codex 配置与目录文件，不把上述 UI JSON 直接当作 CLI 文件格式。

参考：`vendor/cc-switch/backend/src/molly_keys.rs`、`codex_config.rs`、`provider.rs`、`commands/provider.rs`、`services/provider/mod.rs`；`app/src-tauri/src/molly_keys.rs`；前端 `MollyKeyDirectory.tsx`、`forms/CodexConfigSections.tsx`、`types.ts`。

## 6. 自动登录与应用内保存

Mac 当前已取消反复调用钥匙串，自动登录邮箱、密码、刷新令牌、助手配置及手动 API 密钥改为应用私有目录内的加密文件。Skill Git 凭据也使用私有存储；Mac 不再用系统 Git 凭据助手或 `security find-generic-password` 查询钥匙串。

应同步的行为：

1. 勾选“自动登录”并成功完成登录后，保存邮箱、密码与刷新令牌；未成功或仍在两步验证时不提前持久化密码。
2. 启动优先恢复刷新令牌；仅确定登录过期时尝试保存的邮箱 / 密码，网络故障不能当作过期而反复登录。
3. 需要两步验证时由用户重新完成验证，不绕过验证。
4. 未勾选自动登录或注销，清理保存的登录资料与会话。Access Token 保持原生内存状态。
5. 控制台和桌宠共享令牌刷新，串行处理旋转刷新令牌及注销，防止并发造成旧会话恢复。
6. 保存失败、文件损坏或读取失败时给出具体错误，不假装保存成功。

Windows 需要保持同样的“应用内保存、没有重复授权弹窗”体验。可复用原 Windows DPAPI 能力实现私有加密文件；不能照搬 Mac 的钥匙文件权限或 POSIX 代码。保留已有 Windows DPAPI 的桌宠 / TTS 配置兼容性，明确处理旧凭据管理器数据的读取或迁移；若确实需要重新登录 / 保存，须在升级说明中写明，不能静默丢失配置。

CC Switch 官方用量读取在 Mac 已改为只读取工具的凭据文件；Official 本身仍为空白，不复制官方凭据。生图工作台继续只保存用户手动填写的密钥。

参考：`app/src-tauri/src/state.rs`、`commands.rs`、`credential_store.rs`、`assistant_config.rs`、`vendor/local-secrets/src/lib.rs`；`vendor/skills-manager/backend/src/core/git_credentials.rs`、`git_backup.rs`、`git_fetcher.rs`；`vendor/cc-switch/backend/src/services/subscription.rs`。

## 7. 桌宠、悬浮球与 Codex 任务进度

### 7.1 登录后创建桌宠

- 控制台启动 / 登录 / 两步验证期间不创建 Petra 窗口；进入已登录工作区时才创建并显示。
- 注销销毁桌宠窗口，停止音频捕获、拖动、平滑移动等临时任务；保留模型、语音、外观等已保存偏好。
- 重新登录恢复保存的设置，不重复创建多个窗口。
- 用户手动隐藏后，刷新账户或切换菜单不能擅自重新显示。托盘和快捷键在已登录时可重新唤出；未登录时提示先登录。
- 懒加载桌宠须先注册余额监听，再发 `pet-ready` 请求控制台补发已缓存余额，避免控制台请求过早完成导致小球空白。

参考：`app/src-tauri/src/pet_window.rs`、`console_settings.rs`、`lib.rs`；`app/src/App.vue`、`app/src/petra/main.ts`。

### 7.2 全屏拖动与层级

Mac 已将边界从可用工作区扩大到完整屏幕，按 **实际模型边界** 限制拖动；大型透明窗口的空白可以超出屏幕，不能拿整个透明窗口夹紧导致角色到不了边缘。保留固定高度贴边拖动、平滑移动和鼠标穿透。

目标层级是普通应用窗口之上、系统 Dock / 菜单栏之下。Windows 对应为桌宠置顶，但不能遮住任务栏或系统菜单；边界包含任务栏占据的区域，角色可以拖到屏幕底边。

Windows 必须使用 Win32 的屏幕坐标和层级实现，校验多屏负坐标、125% / 150% / 200% 缩放、跨显示器 DPI、任务栏四边位置和自动隐藏。不要复制 `screen_macos.rs` 的 AppKit 层级数字、窗口约束放宽或坐标换算。

参考行为：`app/src-tauri/src/screen_macos.rs`。Windows 实现应基于可用 Windows 代码中的屏幕 / 拖动模块，并核对前端传入模型边界的单位。

### 7.3 悬浮外观

- 悬浮球、展开圆形面板和任务胶囊外侧不能出现半透明矩形容器。
- Mac 删除对应 `backdrop-filter` 模糊以避免透明原生窗口的矩形绘制；Windows 用实际 WebView2 检查白色、深色和复杂桌面背景。
- 胶囊的两处光斑已去除。**当前 Mac CSS 直接隐藏胶囊状态的 `.balance-pill__border-light`**，不要按旧设计记录重新加入两道胶囊高光；空闲小球的原有圆形光效可保留。
- 透明桌宠本体、正常圆形 / 胶囊表面、文本、余额、点击及拖动能力保留。

参考：`app/src/petra/style.css`、`app/src/components/PetraAssistant.vue`。

### 7.4 Codex 任务读取修复

原文件通知之外，增加每两秒的会话元数据检查；只对长度 / 修改时间 / 文件身份有变化的文件增量读取，避免不断重读全部日志。通知初始化失败时保留定时同步，不能直接结束监听线程。

保留 Windows 的 `%USERPROFILE%\.codex\sessions`、`CODEX_HOME`、CC Switch 当前 Codex 实际目录和用户附加目录发现逻辑。Mac 新增 `HOME` 发现仅属于平台适配；Windows 不照搬 Mac 目录。

解析器兼容桌面版日志中 `summary` 为字符串等其他类型的记录：忽略不支持的摘要内容，但保留任务生命周期字段，不能导致整个事件被丢弃。补充兼容已知桌面元数据事件。

保留原有运行、思考、命令、等待输入、完成、失败 / 停止及多任务选择规则。后续真实任务进度会替换等待状态；完成胶囊保留 20 秒后回空闲，旧事件与重复快照不延长保留时间。

任务状态只使用允许的生命周期和公开摘要标题；不把原始推理、消息正文、命令参数、工具输出、认证密钥写入前端状态或日志。任务读取不需要调用付费 API，不应依赖启动 App Server 或修改 Codex 配置。

参考：`app/src-tauri/src/codex_activity/{discovery,mod,parser,watcher}.rs`；任务展示继续复用原有 `app/src/petra/assistant/` 和余额胶囊逻辑。旧 `docs/Codex任务监听与界面调整.md` 中“不轮询”“胶囊双光斑”等记录须结合本节的新行为理解。

## 8. 关闭窗口后后台继续运行

Mac 已修复：设置为“最小化到菜单栏”时，点击关闭仅隐藏控制台，不因最后一个窗口关闭而退出整个进程；登录页和注销后的控制台也遵守保存的关闭偏好。明确退出仍正常停止服务、代理与窗口。

2026-10-04 追加了**安装包自带的常驻声明**：`Info.macos.plist` 使用 `LSUIElement=true`；打开控制台切为 `Regular`，收起到菜单栏切为 `Accessory`。这解决本机自动退出工具将无窗口应用结束的问题，不依赖用户设置例外或另行授予后台权限。Mac 菜单栏不可用时拒绝收起，策略切换失败时恢复窗口；没有篡改原生关闭按钮或禁止明确退出。

可供 Windows 复用的变化是：在创建窗口前加载已保存的关闭偏好，各种打开入口统一显示 / 取消最小化 / 聚焦，以及补充隐藏失败和退出生命周期日志。`LSUIElement`、AppKit 激活策略、Dock 行为和 Mac 专用回归脚本不能搬到 Windows。Windows 应保留自己的托盘机制，缺少恢复入口或隐藏失败时保持可操作窗口，不能让用户失去控制台。

Windows 对应要求：设置为“最小化到托盘”后，关闭按钮隐藏控制台，托盘、桌宠及后台仍可用；托盘能重复恢复窗口。保存的策略在登录前、两步验证、已登录和注销后均应有效。设置为“退出”或点击明确退出时才结束进程并完成清理。

注意 Mac 源码中的 `cfg!(target_os = "macos")` 和 `ExitRequested` 保护是平台分支；单纯复制文件不能证明 Windows 已获得同一行为。需在 Windows 实测 Tauri `CloseRequested` / `ExitRequested` 与自身托盘生命周期。

参考：`app/src-tauri/src/console_settings.rs`、`lib.rs`、`examples/console_settings_smoke.rs`；控制台标题栏和设置弹窗也须走同一原生命令。

## 9. API 密钥、提示词、MCP、生图工作台的主题统一

- 将 CC Switch 主要按钮、选中项、焦点与蓝色交互强调改为 Molly 荧光绿；主绿色为 `#b5ff36`，绿色按钮文字为 `#233600`，保留错误 / 警告 / 品牌等语义色。
- API 密钥、提示词、MCP 与生图工作台共用工作区、表面、边框、文字、字体、圆角和阴影 token，深色同步中性炭灰与绿色，不各自维护一套蓝色主题。
- `embedded-theme.css` 只导入对应内嵌文档，不能污染宿主或透明桌宠；`embeddedTheme.ts` 从共享 token 生成 Tailwind / 组件所需 HSL 值。
- 普通输入、textarea、可编辑区的插入光标：浅色黑色 `#000000`，深色白色 `#ffffff`。CodeMirror 需修改绘制的光标边线，并避免出现两个光标。这里指文字插入光标，不是鼠标指针。
- 主题选择继续由 Molly 设置管理，跟随系统 / 浅色 / 深色实时传播。切换不能刷新 iframe 或丢失输入、编辑器、选择和当前工具视图。
- 生图只做主题、字体、标题控件等适配；保留 GPT Image Playground 的原版布局、模型 / 端点 / 流式功能、手动密钥及来源隔离。

参考：`app/src/embedded-theme.css`、`embeddedTheme.ts`、`console-inputs.css`、`design-tokens.css`；`vendor/cc-switch/frontend/src/components/theme-provider.tsx`、`ui/button.tsx`、`embedded/embedded.css`、`tailwind.config.cjs`；`vendor/gpt-image-playground/src/main.tsx`、`components/Header.tsx`、`tailwind.config.js`。

## 10. 版本与检查更新

当前 Mac 最终版本为 **0.2.0**，以 `app/package.json`、Cargo / Tauri 版本为准。历史文档提到的 0.2.1 / 0.2.2 是此前开发与发行记录，不能据此把目标版本改成 0.2.2。

现方案按平台分开更新源：

| 平台 | 检查地址 | 安装文件 |
| --- | --- | --- |
| Windows x64 | `https://desktop.veriolink.com/latest.json` | `MollyCloud_<version>_x64-setup.exe` |
| macOS Apple Silicon | `https://desktop.veriolink.com/latest-macos-aarch64.json` | `MollyCloud_<version>_aarch64.dmg` |
| macOS Intel | `https://desktop.veriolink.com/latest-macos-x86_64.json` | `MollyCloud_<version>_x64.dmg` |

保留 Windows 清单及 EXE 路由，不能用 Mac DMG 覆盖 Windows 的 `latest.json`。清单包含 `version`、`downloadUrl`、`sha256`、`sizeBytes`、可选 `notes`；校验值和大小必须根据最终安装包生成。

更新检查匹配平台、架构、文件名版本和受信任发布域名；失败不阻止应用启动，系统代理失效可直连重试。当前逻辑只在远端版本 **高于** 本地时提示下载，由用户发起，不自动执行安装程序。若继续使用 0.2.0 覆盖发行，已安装 0.2.0 不会收到同版本更新提示；是否提升版本应单独按用户的发行要求确定。

GitHub 源码 / Release 与更新域名存储独立，上传 GitHub 不会自动更新该域名上的清单。Mac 发行附件中的清单保留客户端信任的更新域名，作为该域名的部署文件；未同步安装包和清单到更新域名时不能声称应用内更新已可用。Windows 功能移植默认只完成源码和验证；发布或覆盖 Windows Release 按用户授权执行。

参考：`app/src-tauri/src/update.rs`、`app/scripts/create-release-manifest.mjs`、`publish-desktop-release.mjs`、`docs/桌面端更新发布.md`；Mac 的 `package-macos.mjs` / `prepare-macos-release.mjs` 仅作发行设计参考。

## 11. Mac 移植中完成、Windows 应保留原实现的部分

Mac 移植还完成了系统音频、系统音量 / 静音、应用发现与启动、只读命令、关机、通知、回收站、自启动、系统代理、支付 WebView、插件资源 / 自定义来源和打包的适配。这些能力在旧 Windows 版已有实现，功能同步时保留并回归，不因引入共享业务而删除。

| Mac 的实现变化 | Windows 要求 |
| --- | --- |
| 系统音频用 ScreenCaptureKit / Swift；默认关闭，开启时请求权限 | 保留 Windows 原有音频实现和保存的开关；不编译 Swift 或导入 ScreenCaptureKit |
| AppKit 应用发现 / 启动、macOS 只读命令、macOS 回收站及通知 | 保留 Windows 路径、快捷方式、cmd 查询白名单、回收站和通知 |
| 登录启动使用 LaunchAgent | 保留 Windows 自启动实现 |
| macOS 系统代理读取 | 保留 Windows WinINET 和环境代理行为 |
| 支付适配 WKWebView 临时会话及弹窗 | 保留 Windows WebView2 的独立支付窗口、临时用户目录、Access Token 边界、弹窗会话和关闭清理；充值滚动改动不能破坏付款 |
| 生图原生来源 `molly-image://localhost` | 保留 Windows 的 `http://molly-image.localhost` 等已有受控来源；不照搬 WKWebView 或解除 sandbox |
| Mac 专用 `mollycloud://ccswitch/import` 导入入口 | 外部导入保留预览确认；遵循项目协议所有权要求，不占用独立版 `ccswitch://`；修改时只处理 Molly 自己的关联 |
| Mac 不编译 / 打包 NetSpeed Dynamic | Windows 保留窗口、导航、IPC、能力配置、辅助 EXE 资源及其校验 |
| Cargo 目标依赖、Tauri 分平台配置和 Mac 图标 | 逐项核对 Windows 依赖、build.rs、capabilities、NSIS / MSI 和图标；不整体复制 Mac 配置 |

助手账户信息由本机只读接口返回，不猜测余额或索要密钥；系统命令提示词按目标平台选择，Windows 继续使用 Windows 查询语法。模型 / 桌宠 / 对话 / TTS / 余额 / 订阅 / 充值 / 插件安装与更新等已有功能均应保留。

## 12. 建议的 Windows 实施顺序

1. **确认基线**：保存当前修改，记录可用 Windows 提交、工具链和启动日志，编译并启动旧版本。先解决本机基线问题再同步。
2. **导航与主题**：统一 API 入口、独立提示词 / MCP、Skill 顺序、横向菜单、充值滚动、输入光标及内嵌主题；在真实 WebView2 中检查。
3. **密钥业务**：账户分页同步、Official、完整编辑器、手动添加、CRUD / 分组 / 消费；先证明同步不修改真实工具配置。
4. **Codex 模式**：1M 默认值、每密钥模式、精确交集、恢复原生、待应用与明确写入；用模拟服务和临时工具目录验证。
5. **生命周期与桌宠**：自动登录私有存储、登录后创建、注销销毁、余额补发、托盘常驻、任务读取、透明外观、全屏边界与层级。
6. **目标平台回归**：Windows 原生编译 / 启动、所有菜单、插件、灵动岛、工具配置、支付窗口、图标和升级兼容。再整理源码交付及真实验证结果。

每阶段单独提交并记录验证；出现白屏、无法登录或原生启动失败时就地定位，不能继续叠加改动后仅凭前端构建宣告完成。

## 13. Windows 完成验收清单

所有测试使用临时用户目录、模拟密钥、模拟模型 API 和隔离 CLI 文件。测试记录说明 Windows 版本、缩放比例、构建提交、通过项与未验证项。

- [ ] MSVC 原生构建成功；实际 Windows 应用可启动、登录、刷新、注销与再次登录，无白屏或崩溃。
- [ ] 未登录 / 两步验证无桌宠；登录后只有一个桌宠；手动隐藏后刷新不重现；重新登录恢复偏好。
- [ ] 自动登录重启有效，刷新令牌过期回退符合规则；两步验证不被跳过；取消记住 / 注销清除保存资料；旧数据兼容明确。
- [ ] “最小化到托盘”在各登录阶段关闭后不退后台，托盘重复恢复有效；“退出”和明确退出正常结束并清理代理。
- [ ] Windows 八项菜单顺序正确；提示词 / MCP 独立展示，工具栏无被要求移除的品牌及三个快捷入口，说明文案已删除。
- [ ] 账户密钥创建、复制、删除、分组、搜索、刷新、消费正常；脱敏正确，分页失败 / 换号不误删。
- [ ] 新用户 Official 默认，旧用户选择保留；网络失败不阻止本地 Official；不复制官方登录凭据。
- [ ] OpenCode 手动选择并添加，保留原供应商及默认模型；其他累加工具遵守同类边界。
- [ ] 铅笔打开完整供应商编辑器，保存及 ID 修改后归属和页面刷新正确。
- [ ] Codex 首次 1M 默认生效，原生显式覆盖 / 手动关闭不会被同步改回。
- [ ] 每个 Codex 密钥模式独立持久化；映射精确交集、顺序、去重、1050000 和六种思考等级正确。
- [ ] 模型失败 / 空交集不清空旧配置；换分组刷新正确；切回原生恢复原模型 / 目录 / 指针 / 上下文及高级设置。
- [ ] 同步和模式准备仅入库；明确启用 / 应用后才写隔离 CLI；待应用配置不被回填或并发错误清除。
- [ ] 独立 CC Switch 数据与无关工具配置不变；端口冲突提示；代理恢复保留外部修改。
- [ ] 桌宠实际角色到四个屏幕边缘、含任务栏区域；负坐标多屏、混合 DPI、固定高度、穿透和移动有效。
- [ ] 桌宠处于普通窗口上方，任务栏 / 系统菜单仍能覆盖；自动隐藏及多屏任务栏实测。
- [ ] 球 / 胶囊 / 展开圆面板无矩形容器；胶囊无两处光斑；保留点击、余额和拖动。
- [ ] Codex 本机目录发现、通知和两秒补偿有效；模拟遗漏通知、字符串 summary、等待 / 继续 / 完成，显示正确且无敏感正文泄漏。
- [ ] 浅色黑色、深色白色插入光标，包括 CodeMirror；主题切换不丢编辑状态。
- [ ] 充值可滚到末尾但无滚动条；横向菜单无底线或滚动条，选中下划线和键盘操作仍有效。
- [ ] 登录、八项菜单、概览子页及弹窗在 960×640、1180×760、1440×900 深浅两种主题下检查，无溢出、隐藏按钮或公共框架差异。
- [ ] 生图手动密钥可保存，sandbox 与专用桥保持隔离；CC Switch / 提示词 / MCP 共用插件更新和状态。
- [ ] 灵动岛及其原生辅助组件可用；Windows 音频、TTS、通知、应用启动、自启动等旧功能无回归。
- [ ] 独立支付 WebView 可打开 / 关闭，主控制台继续响应；模拟会话 / 弹窗 / 重载清理正常。模拟测试不等同于真实付款验证。
- [ ] EXE / 任务栏 / 托盘图标为新白底图标，窗口普通 / 最大化 / 贴靠行为正常。
- [ ] 更新源仍选 Windows EXE，清单校验与版本比较正确；不把 Mac 产物或模拟文件发布给 Windows 用户。

在 Windows 的 `app` 目录按实际可用脚本运行基础检查：

```powershell
npm.cmd ci
npm.cmd test
npm.cmd run build
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib --test session_persistence
npm.cmd run tauri -- build
```

完成 UI 后，运行开发服务，再执行已有设计检查：

```powershell
npm.cmd run check:design
npm.cmd run check:design -- --dark
npm.cmd run check:design -- --appearance
npm.cmd run check:design -- --netspeed
```

专项参数和隔离原生示例仅在当前 Windows 实现已支持时使用，缺少时补充对应验证，不从失败同步分支直接搬取。构建脚本应根据 Windows 实际环境选择前端、原生依赖和资源；Mac 上强制构建 Windows 前端、浏览器模拟和共享 Rust 测试均不能代替 Windows 原生编译 / WebView2 运行。

## 14. Mac 改动索引与最终交付要求

| Mac 提交 | 主要改动 |
| --- | --- |
| `1a4d422` | macOS 移植、移除 Mac NetSpeed、平台服务 / 构建 / 更新适配 |
| `5da4a6f` | 原生 Mac 窗口按钮、圆角、菜单底线 / 滚动条、工作区阴影 |
| `69d5aaa` | 桌宠拖动、透明外观、Codex 读取、登录后启动与注销生命周期 |
| `453bf58` | 账户密钥与 CC Switch 统一、Official、自动准备与手动启用 |
| `947d5a5` | 完整供应商编辑、Codex 1M、原生 / 映射及白名单目录 |
| `05c0141` | 应用内凭据、API 菜单命名、独立提示词 / MCP、绿主题、充值滚动 |
| `c6b288f` | 最终主应用与发行源版本恢复为 0.2.0 |
| `3b22b90` | Mac 后台关闭修复、白底图标、Skill 顺序、桌宠层级、光标及四个工具主题统一 |
| `v0.2.0-macos`，2026-10-04 常驻修复批次 | 安装包 `LSUIElement`、前台 / 菜单栏激活策略、提前读取偏好、统一唤起入口、真实原生关闭与退出监督回归；具体发行提交在 Release 说明中记录 |

交付一份 Windows 实际改动清单与验证记录，注明基线、最终提交、未验证事项和升级数据兼容情况。默认先交付源码；安装包、版本提升、上传 GitHub Release 和更新清单发布按用户后续要求执行。**必须把“源码已修改”“Windows 编译通过”“Windows 实机功能通过”分别说明，不能把任何一项当作另外两项的证明。**

## 15. 持续同步记录

Windows 每轮升级先查看 [Windows 更新记录](./Windows更新记录.md)，选择尚未同步的批次；完成后回填 Windows 提交、系统 / 工具链、编译结果、实机结果与剩余问题。Mac 每次推送或上传也同时维护该记录及本文中的目标行为。历史记录与完整目标结合使用，不能仅按两端版本号判断一致，也不能将 Mac 通过的测试勾选为 Windows 已通过。
