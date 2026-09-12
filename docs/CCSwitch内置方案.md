# CC Switch 内置方案

更新：2026-09-12。按用户最新需求，内置 CC Switch 管理本机工具实际配置。早期“仅 Molly 私有 CLI 配置”方案已被本文件取代。

## 功能与使用

内置模块基于 CC Switch 3.20.3，固定提交 `d695a2d77fd9081eafd3e9eedcbf2a97b3410928`，保留原版 React 内容 UI、供应商配置逻辑和 MIT 许可证。

1. 在 MollyCloud API 密钥页选择“导入到内置 CC Switch”：只写入 Molly 供应商库。同一账户、同一密钥重复导入更新同一条目。
2. 点击“打开”，进入内置界面定位供应商。
3. 点击供应商“启用”，配置写入该工具的实际用户目录。普通终端启动的对应 CLI 同样读取这些文件。
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

## 宿主与内容边界

外部侧栏、顶部栏、工作区背景遵循 Molly 公共设计规范；CC Switch 原 UI 只在同源 iframe 内生效。首次访问挂载，切换菜单保留编辑状态，滚动在内容区内部。

原生命令仍限定 `console` 窗口和固定允许列表，并受 Tauri capability 检查。导航消息检查同源地址和来源窗口，只传工具类型与供应商 ID。API 密钥导入在 Rust 内传递，不进入 URL、命令行或浏览器导航消息。

窗口、托盘、自启动、更新、退出归 Molly 管理；不运行上游独立应用启动器，不注册或唤起外部 `ccswitch://`。管理器数据目录固定，只读展示；工具配置目录可以修改。生图工作台的来源隔离与手动密钥策略不变。

## 实现入口

- `vendor/cc-switch/backend/src/embedded.rs`：宿主初始化、旧模式迁移、工具目录、导入。
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
cargo test --manifest-path app/src-tauri/Cargo.toml --lib ccswitch::tests

# app 目录
npm run build

# app/src-tauri 目录，使用最终生产资源和真实 WebView2 IPC
cargo run --example ccswitch_smoke --features ccswitch-smoke
cargo run --example ccswitch_smoke --features ccswitch-smoke -- --init-failure
```

专项回归覆盖：DB-only 幂等导入、本机默认路径/自定义路径、Codex 实际配置写入、原生凭据策略保留、旧启用状态迁移、独立版数据保护、端口占用、代理恢复与外部改动保留。原生烟测点击真实供应商启用按钮并检查磁盘结果；初始化失败时宿主继续响应。

历史私有模式验证记录已被当前语义取代。实际付费 API、所有第三方 CLI 的完整运行流程、安装器升级和卸载需要另行实测；浏览器预览与模拟凭据测试不能代替这些验收。测试不重启用户桌宠。

源码来源：[farion1231/cc-switch](https://github.com/farion1231/cc-switch/tree/d695a2d77fd9081eafd3e9eedcbf2a97b3410928)。

2026-09-12 验证与发行记录：4 项后端专项回归、13 项内嵌桥接测试、12 项 Molly 前端测试、39 组设计检查和生产登录检查通过；真实 WebView2 11 项正常流程验证通过，默认/自定义模拟用户目录均正确写入，独立版样本数据保持不变。EXE 与 MSI 已生成至 `output/installers/2026-09-12_1836/`；SHA-256、文件大小、升级使用说明和 MSI 文件表核对结果见同目录 `安装包说明.md`。未安装或重启用户应用。
