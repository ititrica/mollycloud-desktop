# MollyCloud 项目说明

修改登录或控制台 UI 前，先阅读 `design-system/molly-desktop/MASTER.md`。该文件是当前设计规范，页面细则不能覆盖公共导航、顶部栏、字体或工作区背景。

设计 token 位于 `app/src/design-tokens.css`，组件主题位于 `app/src/theme.ts`。复用共享规则，不用页面专属覆盖修补公共框架。

涉及公共样式时检查登录和全部七个控制台页面，覆盖 960×640、1180×760、1440×900。开发服务运行时可在 `app` 目录执行 `npm run check:design`，截图输出到 `artifacts/design-review/`。

生图工作台仅在 sandbox 内保留 GPT Image Playground 原版 UI；不能解除 sandbox 的来源隔离或注入通用 Tauri IPC。密钥由用户手动填写并持久保存，禁止自动读取账户 API 密钥。存储、网络及上游版本边界见 `docs/GPTImagePlayground集成方案.md`。

CC Switch 仅在内嵌内容区保留原版 UI；外部导航、顶部栏和工作区仍遵循 MollyCloud 公共规范。供应商库、设备设置、备份保存在 MollyCloud 私有目录，代理使用独立端口；工具配置、MCP、Skills、会话及用户明确操作的环境变量管理作用于本机实际工具。API 密钥导入只入库，点击启用后才写工具配置。不注册或唤起外部 `ccswitch://`，不覆盖独立版 CC Switch 数据；共享工具配置的代理归属、升级与测试边界见 `docs/CCSwitch内置方案.md`。回归测试必须使用临时用户目录和模拟凭据。

Petra 透明窗口使用独立布局；不要把控制台背景、最小窗口尺寸或滚动规则应用到该窗口。保留用户已有修改，不为界面检查重启用户正在运行的桌宠。
