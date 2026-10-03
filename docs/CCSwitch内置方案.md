# CC Switch 内置方案

更新：2026-10-04。内置 CC Switch 管理本机工具实际配置。早期“仅 Molly 私有 CLI 配置”方案已被本文件取代。

## 功能与使用

内置模块基于 CC Switch 3.20.4（标签 `v3.20.4`），固定提交 `43e1d99084ed9b2f5dc252fd35c5adaf29d6876e`，保留原版 React 内容 UI、供应商配置逻辑和 MIT 许可证。

外观沿用原版浅色/深色主题，在现有同源 iframe 内监听 MollyCloud 根节点主题并实时同步；内嵌主题设置提示从控制台更改。切换不重载 iframe、不改写独立版或内置旧主题偏好，也不调用原生窗口主题设置。控制台默认跟随系统，可在设置中选择跟随系统、浅色、深色。

1. API 密钥管理统一到 CC Switch，独立导航入口已移除。账户密钥在供应商卡片下方展示，支持创建、复制、删除、搜索、更换分组及配置模型；复制密钥由原生后台根据 ID 读取并写入剪贴板，账户密钥同步响应与导航消息只接收脱敏值；供应商配置仍由 CC Switch 原有权限读取。
2. Codex、Claude Code / Desktop、Gemini、Grok 首次未配置时默认选择空白 Official；不读取或复制官方登录凭据。兼容分组的有效账户密钥自动读取可用模型并保存为私有供应商选项，模型发现失败时可手动配置。重复同步保留已有模型、供应商设置和当前选择。
3. 点击“启用”或确认“保存并应用”后，配置才写入工具实际目录。OpenCode、OpenClaw、Hermes、Pi、MiniMax 等多供应商工具既不创建 Official，也不自动添加账户供应商或选择默认模型，需要用户选择密钥、模型并确认添加，原有工具配置保留。删除账户密钥后仅清理该账户的私有供应商库；不自动改写正在使用的工具文件。完整分页读取成功后才允许清理；账号切换或不完整响应会中止同步。
4. 已运行的 CLI 通常需要退出并重新启动。项目局部设置、终端继承的鉴权环境变量仍可能覆盖用户配置，可使用原版环境变量冲突检查处理。
5. 内置“启动 CLI”入口支持 Claude Code、Codex、Gemini，使用界面配置指向的同一目录。缺少可执行文件或项目目录无效时，在切换供应商前报错。

这里的 Codex 指 Codex CLI 及读取相同配置的工具；不会修改 ChatGPT 网页版的服务地址。各工具支持范围、模型协议及平台行为遵循所固定的上游版本。

恢复工具选择、供应商、MCP、提示词、Profiles、Skills 安装同步、会话管理、用量、工具安装更新、配置导入导出、数据库备份恢复与云同步功能。涉及写入的功能仍由用户在对应界面明确操作；后台云同步只在用户配置并开启后工作。

## 路径与所有权

| 对象 | 当前规则 |
| --- | --- |
| Molly 供应商数据库 | Molly 应用数据目录下 `ccswitch/data/cc-switch.db` |
| 设备设置 | `ccswitch/home/.cc-switch/settings.json`，保持旧内置版兼容路径 |
| 备份、缓存、日志 | Molly 的 `ccswitch/data`、`cache`、`logs` |
| Claude Code | 用户目录 `.claude/settings.json`；默认 MCP 为用户目录 `.claude.json` |
| Codex | 用户目录 `.codex/config.toml`、可选的 `auth.json`，以及模型目录 |
| Gemini CLI | 用户目录 `.gemini/.env` 和 `settings.json` |
| MiniMax Code | 默认 `.minimax`；支持 `MINIMAX_DATA_DIR`，兼容 `MAVIS_DATA_DIR`；供应商 `config.yaml`、MCP `mcp.json`、Skills `skills/`、提示词 `AGENTS.md` |
| 其他工具 | 采用各工具的默认目录、环境变量或 CC Switch 设置覆盖 |
| 独立版 CC Switch | 其数据库、设备设置、安装项与协议关联保持独立 |
| 本地代理 | Molly 默认 `127.0.0.1:24327`；占用时报错，不结束占用进程 |

工具目录支持设置覆盖；默认解析支持 `CODEX_HOME`、`CLAUDE_CONFIG_DIR`、`GEMINI_CLI_HOME`，以及其他工具的对应路径变量。CC Switch 的目录覆盖必须与工具实际启动环境一致，不能把目录覆盖误当作系统环境变量修改。Gemini CLI 启动所用目录必须以 `.gemini` 结尾。Molly 启动的终端通过子进程环境传递目标路径，清除继承的鉴权覆盖项，保留其他工具的路径及系统主目录。

供应商库和设备设置不读取用户的 `~/.cc-switch`。通用配置操作允许实际工具目录，但拒绝把独立版 CC Switch 默认数据目录作为工具配置目标，包括目录别名。独立版如使用自定义数据目录，仍不应将该目录选为工具目录。

Molly 私有目录的 Windows MSIX 虚拟化修复保持：先检查目录本身不是符号链接/重解析点，再解析真实存储位置。目录不可用时显示具体错误；不回退到独立版目录。

Codex 第三方凭据沿用上游写入 `config.toml` 的供应商条目；`auth.json` 可保留原生登录。不能仅根据是否出现 `auth.json` 判断启用成功。不再强制注入文件凭据存储策略，保留用户原有配置及上游凭据处理逻辑。

## 共享配置与代理

两个管理器现在可以操作同一工具配置，因此不能承诺同时切换时互不影响。最后一次启用会决定工具下一次启动使用的供应商。

- 首次写入 Claude、Codex、Gemini、Grok 的代理相关配置文件前，将原始字节和绝对路径保存到 Molly 私有 `data/system-config-originals/*.json`。该基线不会被后续切换覆盖，`bytes: null` 表示原文件不存在。
- 代理使用专属 `MOLLY_CCSWITCH_PROXY_MANAGED` 标记。恢复账本同时绑定绝对路径和文件哈希，旧私有目录或另一个自定义目录的记录不能授权恢复当前文件。
- 检测到独立版的 `PROXY_MANAGED` 时，拒绝切换/接管，提示先在独立版关闭该工具代理；不会误用其占位凭据或恢复其备份。
- Molly 接管后，如其他程序改动配置，自动恢复保留外部改动并报告错误。
- Molly 接管期间更换相关工具目录会被拒绝；先关闭接管并恢复原目录，再改目录。

文件快照降低误覆盖风险，不代表能够阻止独立版或其他进程并发写同一文件。请避免同时使用两个管理器切换同一工具。

## 旧内置版升级

第一次加载新版时，先保存旧设备设置及数据库备份，清除旧私有目录覆盖、旧当前供应商绑定和代理接管状态，记录一次性迁移标记。供应商条目、密钥、旧私有文件及历史备份保留。

升级本身不将旧配置复制到本机工具目录；用户重新点击“启用”后才应用。迁移重复执行不会清除新版已经选择的供应商。旧私有代理备份不会用于恢复本机文件。

## CC Switch 3.20.4 升级（2026-09-23）

从 3.20.3 基线合入上游版本差异，Molly 主程序版本为 0.1.4。本次主要变化：

- 新增 MiniMax Code 工具页、供应商配置、MCP、Skills、提示词，以及只读会话与用量导入。Molly 密钥导入窗口同步增加此工具。
- MiniMax 供应商采用累加配置；新增或移除供应商不代替原生工具选择默认模型。保留非自定义账户、未知 YAML 字段及原生锁；当前默认模型被引用时拒绝直接移除。上游尚未提供 MiniMax 代理、故障转移、Profiles 或删除会话能力。
- 数据库从 schema 18 迁移到 19，为 MCP 与 Skills 增加 `enabled_mcode` 列，默认关闭。既有供应商、其他工具启用状态及 `molly_system_targets_v1` 迁移标记保留，不重复执行早期私有模式迁移。
- 合入 Codex `additional_tools`、原始图片细节、第三方密钥保留及 OAuth 失效绑定修复；更新 Claude/Copilot 转换、提示词外部更新、Skills 大仓库与 ID 处理、OpenCode 模型搜索和批量添加等上游功能。
- 保留 Molly 独立管理器数据目录、代理端口与归属标记、固定 IPC 允许列表、主题跟随和宿主生命周期。新增工具的环境目录变量在隔离测试中同样被屏蔽；写配置前检查独立版数据目录边界。

上游独立启动器的托盘/窗口恢复及启动迁移由 Molly 自己的生命周期替代，未引入 `run()` 或 `ccswitch://`。上游新增、直接调用独立启动函数的测试作为参考保留但不编译，内嵌启动与异常恢复由宿主烟测验证。

本次验证结果：

| 验证范围 | 结果 |
| --- | --- |
| Molly 与三个内嵌前端生产构建、TypeScript、安装资源完整性 | 通过 |
| CC Switch 内嵌桥接 / 更新涉及的供应商预设测试 | 13 / 21 项通过 |
| Molly 前端 / 宿主 CLI 启动边界测试 | 32 / 5 项通过 |
| 内嵌后端隔离回归（含 MiniMax 原生锁、默认模型、MCP 和独立版目录保护） | 5 项通过 |
| v18→v19 迁移、协议转换、MiniMax 会话/用量、提示词限制 | 297 项通过 |
| 真实 WebView2 正常流程 / 初始化失败 | 18 / 4 项通过；只使用临时用户目录与模拟凭据 |
| 浅色、深色及跟随系统的外观检查 | 4 组通过，无运行时错误；已检查 CC Switch 浅/深色截图 |

验证日志位于本地 `artifacts/ccswitch-3.20.4-*.log`，外观截图位于 `artifacts/design-review/appearance/`。整组旧 schema 测试中，4 个早期迁移夹具因缺少 Molly 私有目录初始化被隔离保护拒绝；本次新增 v18→v19 回归及其他选定测试单独通过，没有放宽路径保护以兼容独立版夹具。未以真实密钥请求付费 API，未安装、发布或重启用户桌宠。

## 宿主与内容边界

外部侧栏、顶部栏、工作区背景遵循 Molly 公共设计规范；CC Switch 原 UI 只在同源 iframe 内生效。首次访问挂载，切换菜单保留编辑状态，滚动在内容区内部。

原生命令仍限定 `console` 窗口和固定允许列表，并受 Tauri capability 检查。导航消息检查同源地址和来源窗口，只传工具类型与供应商 ID。API 密钥导入在 Rust 内传递，不进入 URL、命令行或浏览器导航消息。

窗口、托盘、自启动、更新、退出归 Molly 管理；不运行上游独立应用启动器。按 2026-09-23 的新需求，用户可主动关联 `ccswitch://` 给 MollyCloud，外部网页链接只会打开内嵌导入确认框；已有独立版协议关联不会被自动覆盖。管理器数据目录固定，只读展示；工具配置目录可以修改。生图工作台的来源隔离与手动密钥策略不变。

## 实现入口

- `vendor/cc-switch/backend/src/embedded.rs`：宿主初始化、旧模式迁移、工具目录、导入。
- `app/src-tauri/src/molly_keys.rs`、`vendor/cc-switch/backend/src/molly_keys.rs`：认证账户分页同步、Official 默认、模型发现与私有库归属清理。
- `MollyKeyDirectory.tsx`、`CcSwitchPanel.vue`：脱敏密钥列表和固定动作消息；宿主原生弹窗保留创建、删除、分组功能。
- `config.rs`、各工具配置模块：实际用户目录和读写。
- `proxy_ownership.rs`：首次原始文件备份、代理冲突和恢复归属。
- `allowed_commands.rs`、`permissions/default.toml`：固定的工具管理能力范围。
- `app/src-tauri/src/ccswitch.rs`：已安装 CLI 查找、目标目录与安全启动参数。
- `vendor/cc-switch/frontend/src/App.tsx`、`components/settings`：原版功能入口和实际目录说明。

## 验证

测试必须显式提供临时系统用户目录与临时 Molly 数据目录；禁止用真实凭据或实际已安装工具进行切换试验。测试路径注入仅编译进测试/专项验证版本。

```powershell
# 仓库根目录
cargo test --manifest-path vendor/cc-switch/backend/Cargo.toml --target-dir app/src-tauri/target --lib embedded::tests -- --test-threads=1
cargo test --manifest-path vendor/cc-switch/backend/Cargo.toml --target-dir app/src-tauri/target --lib database::schema::tests::migrate_v18_to_v19_preserves_existing_molly_data
cargo test --manifest-path app/src-tauri/Cargo.toml --lib ccswitch::tests

# app 目录
npm run build
npm run check:ccswitch-upgrade

# app/src-tauri 目录，使用最终生产资源和真实 WebView2 IPC
cargo run --example ccswitch_smoke --features ccswitch-smoke
cargo run --example ccswitch_smoke --features ccswitch-smoke -- --init-failure
```

专项回归覆盖：DB-only 幂等导入、本机默认路径/自定义路径、Codex 实际配置写入、原生凭据策略保留、旧启用状态迁移、独立版数据保护、端口占用、代理恢复与外部改动保留。原生烟测点击真实供应商启用按钮并检查磁盘结果；初始化失败时宿主继续响应。

历史私有模式验证记录已被当前语义取代。实际付费 API、所有第三方 CLI 的完整运行流程、安装器升级和卸载需要另行实测；浏览器预览与模拟凭据测试不能代替这些验收。测试不重启用户桌宠。

源码来源：[farion1231/cc-switch v3.20.4](https://github.com/farion1231/cc-switch/tree/43e1d99084ed9b2f5dc252fd35c5adaf29d6876e)。

2026-09-12 验证与发行记录：4 项后端专项回归、13 项内嵌桥接测试、12 项 Molly 前端测试、39 组设计检查和生产登录检查通过；真实 WebView2 11 项正常流程验证通过，默认/自定义模拟用户目录均正确写入，独立版样本数据保持不变。EXE 与 MSI 已生成至 `output/installers/2026-09-12_1836/`；SHA-256、文件大小、升级使用说明和 MSI 文件表核对结果见同目录 `安装包说明.md`。未安装或重启用户应用。


## MCP 市场适配边界（2026-09-17）

新增 `backend/src/mcp_market.rs` 向宿主提供四种 Agent 的路径解析、连接格式转换、保留注释的配置合并和原子写入。市场继承现有配置目录覆盖与代理归属保护，使用用户级实际工具配置，不启用 CC Switch 代理，不自动修改供应商、启用状态或 MCP 数据库。

市场安装记录由宿主单独加密保存。CC Switch 和其他工具添加的 MCP 在市场显示为“外部配置”，仅供查看；若其修改市场管理的同名条目，市场会阻止后续覆盖。两种管理入口共享的是实际工具配置，不承诺各自数据库中的副本自动同步。CC Switch 的原始文件快照/归属账本仍遵循本方案原有规则；市场自己的事务备份和安装参数另使用 DPAPI 加密。


2026-10-02 更新：工作台资源现支持独立插件包的安装、卸载、更新和回退；此前“只随主应用发布”的描述已由 [功能插件方案](功能插件方案.md) 中的版本兼容边界取代。原生适配、私有目录和生图隔离要求保持有效。

## 2026-10-04 统一密钥管理回归

宿主 86 项前端 / 6 项脚本测试、CC Switch 24 项嵌入测试、110 项原生宿主单测及 3 项私有库单测通过。覆盖首次空白 Official、重复同步保留选择、完整分页及分页变化拒绝清理、脱敏响应、模型配置保留、账户归属清理、网络失败下 Official 刷新、手动启用与 OpenCode 手动添加。

深浅色各 84 组 UI 检查通过，覆盖登录、五项主导航、概览子页及 960×640 / 1180×760 / 1440×900；失败与运行时异常为空。已查看 CC Switch、分组弹窗、紧凑窗口等截图。原生 WebView 烟测在临时用户目录使用模拟凭据，验证 Codex 启用后才写工具配置、重复同步保留选择、MiniMax 添加保留默认模型、OpenCode 确认模型后添加且保留原供应商和默认选择。证据位于 `artifacts/unified-keys-20261004/` 和 `artifacts/design-review/{light,dark}/`。这些检查不代表真实付费 API 或第三方 CLI 登录验收。

## 0.2.2 供应商编辑与 Codex 配置模式

账户密钥的编辑图标打开原版 CC Switch 完整供应商编辑器，支持名称、端点、鉴权、默认模型、映射和高级配置。尚未准备配置的密钥仅在用户点击编辑后创建私有草稿；OpenCode 等累加工具的草稿没有默认模型，不自动添加到本机工具。编辑保存后立即刷新对应账户密钥行；编辑器修改供应商 ID 后仍按账户与密钥归属找到该配置。

MollyCloud Codex 配置默认加入 `model_context_window = 1050000` 和 `model_auto_compact_token_limit = 900000`。已有配置仅在首次缺少上下文设置时补默认值，保留显式上下文覆盖；完成初始化后，原生模式中用户关闭 1M 不会被定时同步重新开启。Official 和其他来源的供应商保持原有语义。

Codex 的每个账户密钥独立选择“原生”或“映射”，首次为原生。映射模式使用该密钥调用 MollyCloud `/v1/models`，每分钟和用户刷新时重新发现分组模型。仅下列白名单与服务实际返回 ID 的精确交集写入 `settingsConfig.modelCatalog.models`，去重并按白名单顺序排列：

`gpt-5.6-luna`、`gpt-5.6-terra`、`gpt-5.6-sol`、`gpt-6-luna`、`gpt-6-sol`、`gpt-6-astra`、`gpt-6-astra-fast`、`gpt-6.1-sol`、`gpt-6.1-sol-fast`。

每行的 `contextWindow` 为 `1050000`，`reasoningLevels` 为 `low, medium, high, xhigh, max, ultra`，默认思考等级为 `high`。保留默认模型仍在交集中的选择，否则选择交集首项。拉取失败或交集为空时保留已有配置并提示原因，不生成不存在的模型。切回原生恢复切换前的默认模型、上下文覆盖及原始映射表；端点、鉴权、TOML 注释和其他高级选项保留。

自动发现、模式切换及默认值迁移只保存私有库。点击“启用”或当前密钥的“应用配置”后，上游写入本机 Codex 的 `config.toml` 和模型目录。待应用的私有配置不接受旧 live 配置回填；若写入期间准备了更新版本的配置，保留待应用状态。完整编辑器保存当前已启用供应商时，沿用 CC Switch 原有即时应用行为。

回归日志位于 `artifacts/mapping-*.log`；UI 截图位于 `artifacts/design-review/{light,dark}/`。浏览器预览拒绝真实模式写入；原生回归仅使用临时用户目录及模拟凭据，不读取真实账户、不调用付费模型、不重启用户桌宠。

最终验证：宿主前端 86 项与脚本 6 项、嵌入前端 27 项、宿主原生 110 项、供应商与模型目录后端 14 项，共 243 项测试通过。深浅色各 84 组界面检查通过；最终生产资源的原生窗口正常流程 40 项通过，初始化失败流程 4 项通过。实际磁盘目录验证了 `1050000` 上下文、六个思考等级、手动应用及原生恢复。0.2.2 arm64 DMG 的签名、镜像校验、只读挂载后的版本和更新清单校验值已核对；使用本地签名，未进行 Apple 公证。

## 2026-10-04 菜单与凭据适配

外部导航使用“API 密钥”，提示词和 MCP 管理为独立菜单页，同一个插件/iframe 通过严格限定为 providers、prompts、mcp 的同源消息切换视图，仍共用原版管理能力和升级包。供应商工具栏移除品牌文字以及 Skills、提示词、MCP 快捷入口，强调色复用 Molly 荧光绿 token。macOS 官方用量查询仅读取工具的凭据文件，不调用 `security find-generic-password`；仅存系统钥匙串的官方凭据在 Molly 中不可读，CLI 的登录本身不受影响。
