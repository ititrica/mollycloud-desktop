# MollyCloud MCP 市场

> 2026-10-02：独立 MCP 市场已按用户要求移除。本文仅保留历史实现说明。已有 Agent 配置和 CC Switch 自带 MCP 管理不受影响。

实施日期：2026-09-17。提供本机 MCP 来源解析、安装、配置管理与卸载，Molly 不参与 Agent 与 MCP 的日常通信。

## 使用流程

控制台侧栏进入 **MCP 市场**，在“发现”中搜索工具，选择“安装到 Agent”，勾选 Codex、Claude Code、OpenCode 或 Gemini CLI，填写该服务所需目录/API Key 后安装。安装弹窗显示实际配置文件。完成后到目标 Agent 重新加载 MCP；OAuth 服务在目标 Agent 内登录。

“已安装”提供连接测试、启用、禁用、检查并更新和卸载。禁用从 Agent 配置中暂时移除该条目，启用从加密安装记录恢复。卸载默认先移除指定 Agent 配置，再将无其他引用的当前版本软件包移入回收站，服务数据始终保留。取消“同时清理”则仅移除配置，软件包保留。被其他工具修改的条目会阻止覆盖；需先手动确认并恢复与记录一致的配置才能继续管理。外部配置只读显示。

第一版仅写用户级配置；项目级配置、WSL/远程主机、ChatGPT 网页连接器不在此范围。这里的 OpenAI 目标为 **Codex**，不是向模型本身安装工具。

## 目录来源

- 内置 21 个固定版本安装模板，仍可直接选择 Agent 安装。
- Awesome MCP Servers 中文 README 的“服务器实现”部分提供社区发现目录，当前 430 个去重 GitHub 条目。点击“从来源安装”会读取项目的 `server.json`、`.mcp.json` 或 README 中的 JSON 连接示例，支持仓库默认分支与目录链接指定的分支/子目录。保留原文描述和分类；框架、客户端及实用工具章节不作为 MCP 服务导入。
- 官方 MCP Registry 使用 v0.1 API 在线关键词搜索（最多 40 条），缓存最近一次搜索供查看。点击“从来源安装”重新拉取该条目的最新完整元数据，将受支持的 `packages` / `remotes` 转为安装选项。缓存仍只保存发现信息，不能携带执行模板。

解析支持标准 npm、PyPI 和 Streamable HTTP。npm 从官方包元数据确定固定版本和唯一 bin；Python 安装后读取分发元数据中的唯一 console_scripts 入口。Registry 中的环境变量、请求头、命名/位置参数、默认值、选项与密钥字段转换成表单。社区 JSON 支持 npx / uvx 连接与 HTTP 地址，常见占位凭据转换为待填参数。

安装前弹窗显示安装方式、来源链接、固定版本及参数模板，可切换多个候选。解析只读取文档和软件包元数据，点击安装后才下载、初始化并写入所选 Agent。原生端保存 30 分钟有效的安装计划，前端只能提交计划 ID 与用户参数，不能从缓存注入任意包名/入口。

Docker/OCI、MCPB、源码构建、shell 脚本、私有软件源、多个或缺失启动入口、额外运行时参数，以及声明了暂不能验证的文件哈希的包，均保留手动配置入口并说明原因。README 只有 shell 示例或 JSON 无法解析时不会猜测执行命令。

社区来源：[Awesome MCP Servers](https://github.com/punkpeye/awesome-mcp-servers/blob/main/README-zh.md)。官方目录：[MCP Registry](https://registry.modelcontextprotocol.io)。Awesome 的 MIT 许可全文在 `app/THIRD_PARTY_NOTICES.md`，来源和快照 SHA-256 在 `app/src/mcp/community-source.json`。

可用 `node app/scripts/sync-mcp-community.mjs --update` 显式更新发现目录；也可传入 README 文件离线生成。格式变化或提取数量异常时保留已有目录。该脚本不会生成安装命令。

## 独立运行

本地软件包和持久数据位于 `%LOCALAPPDATA%/MCP/Packages/<id>/`，版本目录与 `data` 分开。该目录不在 Molly 的安装位置或应用标识目录内，不列入卸载删除资源。包管理下载缓存也位于此目录。

npm 服务需要用户安装 Node.js 22.13+（含 npm），固定主包版本安装，禁用生命周期脚本，Agent 配置直接引用 Node 的绝对路径及软件包入口。Python 服务需要 uv，使用 Python 3.12 独立虚拟环境，Agent 直接引用环境中的 EXE。缺少运行环境时界面提供官方安装说明，不自动修改系统 PATH 或全局安装软件。首次安装需要网络；部分工具另需 Chrome、Git、API Key 或浏览器下载。

远程服务只写服务 URL/请求头。所有 MCP 均由 Agent 自行启动或连接；Molly 不创建代理端口、后台转发、常驻 MCP 进程，也不在 Agent 配置里写入 Molly 启动器。退出/卸载 Molly 后，这些路径和连接仍独立有效，前提是用户保留对应 Node/Python 环境和软件包。

## 配置与恢复

| Agent | 默认用户级配置 | 格式 |
| --- | --- | --- |
| Codex | `%USERPROFILE%/.codex/config.toml` | `mcp_servers`；请求头为 `http_headers` |
| Claude Code | `%USERPROFILE%/.claude.json` | `mcpServers` |
| OpenCode | `%USERPROFILE%/.config/opencode/opencode.json` 或 `.jsonc` | `mcp`；local 的 command 数组、environment；remote 的 url/headers |
| Gemini CLI | `%USERPROFILE%/.gemini/settings.json` | `mcpServers`；Streamable HTTP 为 `httpUrl` |

实际路径复用 CC Switch 的系统环境变量/自定义目录解析，并在安装前展示。OpenCode 两个配置文件同时存在且没有明确文件覆盖时拒绝猜测。Codex 不接受旧版 SSE。手动配置接收单个 stdio/http/sse 的通用 JSON，再转换为目标格式。

安装前检查文件可解析、同名条目冲突和路径；下载/初始化后再次检查。原子合并保留其他设置、MCP 与注释。多 Agent 写入失败会恢复已写文件，检测到外部并发修改时保留现场，防止回滚覆盖别人修改。加密写前日志用于中断恢复，已提交事务通过账本 revision 识别，不重复撤销。

市场安装参数、密钥副本和事务备份使用 Windows DPAPI，绑定当前 Windows 用户。Agent 的实际配置遵循该 Agent 的原生格式，可能包含明文环境变量或请求头；市场不向前端返回这些凭据，不上传到 MollyCloud。复用 CC Switch 写入适配时，其已有原始快照和代理归属规则继续生效，详见 `CCSwitch内置方案.md`。密钥不会自动取自 Molly 账户。

## 连接检查与更新

只发送 MCP `initialize`，不枚举或执行工具。第三方进程启动本身仍由对应程序决定行为。stdio 测试限 25 秒，使用 Windows Job Object 清理临时进程树；测试在临时工作目录进行。HTTP 支持 JSON/SSE 形式的初始化响应、401/403 等待授权状态、大小/时间限制，并尽力删除临时会话。在线目录、包版本查询与远程检查读取已有 Windows/环境代理配置，回环地址绕过代理；不修改系统代理设置。此次同时修正了共享代理读取器将 Windows UTF-16 注册表值误作 UTF-8 的问题。旧版 SSE 和自定义脚本启动命令可以写入，但需要在 Agent 里进一步检查兼容性。

固定版本本地模板及来源计划在写入配置前需通过初始化；远程授权/网络失败允许保存配置，并明确显示“等待授权/待检查”。手动添加只保存用户提供的连接，不立即执行未知启动命令；用户点击测试后才进行握手。

“检查并更新”显式读取原包的最新版本，安装到新版本目录，初始化通过后才切换所选 Agent；旧目录保留。更新只影响被操作的 Agent。来自来源计划的更新仍使用原包名，并从该包新版本元数据重新确定启动入口。初始化通过不等于所有工具、付费 API 或 OAuth 业务已验证。

## 卸载与文件清理

配置事务成功后才清理文件；配置冲突或写入失败时保留软件包。清理前检查所有市场安装记录（包括已禁用的记录）、四种 Agent 的当前用户级配置，以及安装记录中的旧配置路径。其他 Agent 仍引用相同版本时，只卸载所选 Agent 的连接并说明共享文件已保留。配置读取失败时也保守保留软件包。

仅清理独立安装目录内 `<id>/<固定版本>` 且带有匹配市场安装标记的版本目录；拒绝路径穿越、符号链接与目录联接。文件移入系统回收站后检查原路径确已不存在，占用、权限或部分失败会明确显示。更新、只移除配置或清理失败留下的完整版本，出现在“保留的软件包”中，可重新检查引用后清理。`data`、下载缓存、系统 Node.js / uv / Python 环境不删除。

手动添加的程序不归市场管理，卸载仅移除其 Agent 配置；远程服务也只移除连接。检查范围不包含任意项目级、WSL 或远程主机配置；项目级配置若引用市场的软件包路径，需要先停用相关连接。外部工具创建的 MCP 保持只读。

## 验证与实现入口

- 前端：`app/src/components/McpMarketPanel.vue`、`app/src/mcp/`。
- 原生安装/事务/检查：`app/src-tauri/src/mcp_market/`。
- Agent 格式适配：`vendor/cc-switch/backend/src/mcp_market.rs`。
- 独立 WebView2 流程：`app/src-tauri/examples/mcp_market_smoke.rs`，测试路径注入仅随专项 feature 编译，使用临时 Agent 用户目录及模拟凭据。

```powershell
# app 目录
npm test
npm run build
npm run check:design
npm run check:design -- --dark

# app/src-tauri 目录
cargo test --lib mcp_market -- --test-threads=1
# 额外下载真实公开固定版本，临时目录内安装并初始化四个服务
cargo test --lib actual_npm_and_python_initialize -- --ignored --nocapture --test-threads=1
# 在线来源与 Python 入口解析（临时目录）
cargo test --lib live_source_resolution_and_install -- --ignored --nocapture --test-threads=1
cargo test --lib resolved_python_entry_point -- --ignored --nocapture --test-threads=1
# 完整生产前端 + 原生 IPC，来源安装 UI、四种 Agent、共享卸载及残留包清理
cargo run --example mcp_market_smoke --features mcp-market-smoke
```

测试不使用真实 Agent 配置或账户密钥，不重启用户正在运行的桌宠。实际 Agent 中完整工具调用及各第三方授权仍需对应服务的有效环境；21 个模板不标记为全部实测。


2026-09-17 初版验证记录：25 项前端测试通过；15 项 MCP 后端测试通过（含真实 npm 的 Sequential Thinking、Memory、Filesystem 与 uv/Python Time 安装/初始化/重用），新增代理 UTF-16 修复的 4 项测试及 HTTP 定向回归通过。生产前端构建与打包资源检查通过；真实 WebView2 的 21 项端到端检查通过，包括四目标写入、凭据遮蔽、安装/测试/禁用/启用/更新/移除、原有配置保留和在线 Registry 搜索。浅色、深色各 45 组界面回归通过，覆盖 960×640、1180×760、1440×900，failures/runtimeErrors 为空。

报告位于 `artifacts/mcp-market-research/`，界面截图位于 `artifacts/design-review/light/` 和 `dark/`；市场页面/弹窗与最小尺寸九页汇总图已人工查看。上述验证未启动真实 Agent 进行有副作用的业务工具调用，未提供真实远程 OAuth 凭据，也未打包或安装新安装器。


2026-09-17 来源安装与卸载增量验证：25 项前端测试、20 项 MCP 后端常规测试通过；另行执行并通过官方 Registry / GitHub 在线解析、动态 npm 安装与 Python 入口识别安装测试。真实 WebView2 的 29 项检查通过，覆盖来源计划防篡改、社区来源安装 UI、四种 Agent 写入、禁用后的共享引用保护、更新、卸载与残留版本清理。测试均使用临时配置目录与模拟凭据。生产前端构建通过，深浅色各 45 组界面检查通过，新增来源安装/卸载弹窗截图已查看。

在线验证同时发现 `mcp-server-time@2025.8.4` 与其当前解析到的 MCP Python 依赖存在 `McpError` / `MCPError` 导入不兼容；市场在初始化失败时拒绝写入 Agent 配置，并区分进程退出与超时。当前 `2026.8.18` 的动态入口识别与初始化通过。来源可解析不代表第三方任意历史版本都能在当前依赖下运行。

本次日志为 `artifacts/mcp-market-research/source-*.log`，新增截图为 `artifacts/design-review/{light,dark}/mcp-source-plan-*.png` 和 `mcp-uninstall-*.png`。未执行真实账户授权或有副作用的 MCP 工具业务操作。
