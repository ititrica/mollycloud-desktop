# MollyCloud 项目说明

修改登录或控制台 UI 前，先阅读 `design-system/molly-desktop/MASTER.md`。该文件是当前设计规范，页面细则不能覆盖公共导航、顶部栏、字体或工作区背景。

设计 token 位于 `app/src/design-tokens.css`，组件主题位于 `app/src/theme.ts`。复用共享规则，不用页面专属覆盖修补公共框架。

涉及公共样式时检查登录和全部七个控制台页面，覆盖 960×640、1180×760、1440×900。开发服务运行时可在 `app` 目录执行 `npm run check:design`，截图输出到 `artifacts/design-review/`。

生图工作台仅在 sandbox 内保留 GPT Image Playground 原版 UI；不能解除 sandbox 的来源隔离或注入通用 Tauri IPC。密钥由用户手动填写并持久保存，禁止自动读取账户 API 密钥。存储、网络及上游版本边界见 `docs/GPTImagePlayground集成方案.md`。

CC Switch 仅在内嵌内容区保留原版 UI；外部导航、顶部栏和工作区仍遵循 MollyCloud 公共规范。供应商库、设备设置、备份保存在 MollyCloud 私有目录，代理使用独立端口；工具配置、MCP、Skills、会话及用户明确操作的环境变量管理作用于本机实际工具。API 密钥导入只入库，点击启用后才写工具配置。不注册或唤起外部 `ccswitch://`，不覆盖独立版 CC Switch 数据；共享工具配置的代理归属、升级与测试边界见 `docs/CCSwitch内置方案.md`。回归测试必须使用临时用户目录和模拟凭据。

Petra 透明窗口使用独立布局；不要把控制台背景、最小窗口尺寸或滚动规则应用到该窗口。保留用户已有修改，不为界面检查重启用户正在运行的桌宠。

## Mac 先行开发与 Windows 同步

macOS 是先行开发版，功能、业务规则和共享视觉改动先在 `codex/macos-port` 实现并验证；Windows 从可运行的 Windows 工作区按改动升级，保留 NetSpeed Dynamic 和 Windows 平台能力。不能把 Mac 编译或测试通过当作 Windows 已验证，也不能直接合并用户反馈不可用的 `codex/windows-parity` 分支。

`docs/Windows更新记录.md` 是持续维护的逐次更新记录，`docs/Windows功能同步交接.md` 是完整功能目标、平台差异和验收清单。每次推送 Mac 源码或上传安装包前，都必须同步更新这两份文档：记录版本 / 更新批次、源码分支或 tag、改动、Windows 移植要求、验证结果和未验证项；没有 Windows 相关行为变化也必须注明。保留历史批次，不能仅用新的快照覆盖记录。

同一次发布须提交最新文档、在 GitHub Release 说明中链接它们，并附上两份 Markdown 供 Windows 端下载。附件使用便于跨平台下载的英文文件名：`WINDOWS_SYNC_CHANGELOG.md` 对应更新记录，`WINDOWS_SYNC_HANDOFF.md` 对应完整交接；内容必须与仓库中文文档一致。Release tag 应指向对应源码，附件哈希应与最终安装包一致。Windows 同步状态默认“待 Windows 实机验证”；只有 Windows 端提供实际编译和运行结果后才能标记完成。

Mac 使用独立 `v<版本>-macos` tag 和 `latest-macos-<架构>.json`，不覆盖 Windows tag、安装包或 `latest.json`。新版本原则上保留历史；用户明确要求同版本覆盖时，先备份被替换的 Mac tag / Release 附件，更新源码、安装包、清单、校验和及同步文档，并注明同版本不会触发自动更新。GitHub 与更新域名分开发布，不能将 GitHub 上传成功记为更新服务器已部署。
