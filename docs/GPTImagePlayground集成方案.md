# GPT Image Playground 内置实现

日期：2026-09-12。已按用户确认集成为控制台的“生图工作台”，密钥使用手动填写和长期保存方案。

用户最终选择：CookSleep/gpt_image_playground。上游最新发行版为 v0.7.12（2026-09-09），评估提交 `da4fda85b59ecacc51d6a1e2ef680e3abb9e29b8`。

## 使用

控制台 → 生图工作台 → 右上角设置 → 手动填写 API Key → 保存密钥。默认 Base URL 为 `https://mollycloud.cn/v1`，可修改。默认 Images 模型为 `gpt-image-2`，可按所用服务商支持的模型修改。

首次进入时加载，由 MollyCloud 构建、打包和更新本地资源。用户不需要安装独立程序、Docker 或 Node.js。内置页面可以离线打开，实际生成仍需访问模型接口。

项目是 React 19 + Vite 前端，支持 OpenAI 兼容的 Images / Responses API、sub2api 异步及自定义供应商；已有参考图、遮罩编辑、流式预览、收藏夹、历史记录和 ZIP 导出，功能围绕生成与编辑图片展开。其 Agent 模式直接调用 Responses API，不依赖本地 Codex/Claude CLI 或 Canvas Agent 服务。

接口能力由所用服务商决定。上游支持 sub2api 不代表 Molly 线上部署已经开放所有图片端点、模型或 Agent 能力。

## 控制台呈现

- “生图工作台”位于 CC Switch 下方。外部导航、顶部栏、余额及公共背景遵循 MollyCloud 设计规范。
- 原版图片生成、编辑和画廊交互保留在独立内容区域，前端样式不注入宿主页面。后续如需独立放大窗口，可复用同一模块。
- 首次打开时加载，切换菜单保持模块挂载和生成请求；退出登录卸载页面、取消未完成请求，已保存作品保留。
- sandbox 不允许 `allow-same-origin` 或顶部导航。宿主校验当前 iframe 的 `event.source` 及 opaque origin，再转交一个 MessageChannel，仅提供当前账户的配置、四张数据表、图片网络请求与任务完成通知。
- 生产静态资源经 `http://molly-image.localhost` 提供，只允许打包的工作台静态路径；浏览器开发预览经 `/image-workbench/` 提供。子窗口不能读取宿主账户存储或直接调用通用 Tauri IPC。
- 移除独立 PWA 安装入口、Service Worker、上游更新检查和首次赞助弹窗，保留原版名称、关于和版权信息。
- 控制台关闭原生文件拖放拦截，使工作台可以处理 HTML 文件拖放；Petra 窗口保留自己的原生拖放配置。

## 账户与网络

- 密钥初始为空，不调用控制台账户密钥的读取或复制接口，不自动导入 Molly 账户密钥。手动填写后离开输入框保存，“保存密钥”等待本地写入确认后才提示成功，失败显示错误。
- 默认地址只用于初始化，不覆盖用户保存的地址、模型或密钥。密钥不放入 URL、启动参数、构建常量或日志。
- 桌面版使用独立 reqwest 请求，不继承账户 Cookie、控制台令牌或其他服务商密钥。支持 JSON、多段表单、状态码、逐块响应、SSE 与取消；上游 Docker `/api-proxy/` 不在内置版启用。
- 允许 HTTPS 和明确的本地 loopback HTTP，拒绝文件协议、Tauri/IPC 内部来源和含用户密码的 URL。重定向不自动跟随，用户需填写最终地址。上传上限 128 MB，响应上限 256 MB，同时最多 12 个请求；连接超时 30 秒，整体上限 30 分钟，页面较短的超时设置仍生效。
- 浏览器预览直接 fetch，仍受目标服务 CORS 限制，不能用浏览器预览替代原生请求验证。
- 生成结束后刷新 Molly 余额与用量。以服务端账单为准，失败或取消不能被解释为一定未计费。

## 数据与版本

配置保存在宿主 WebView 本地存储，任务、图片、缩略图与 Agent 对话使用 IndexedDB，沿用上游 schema 3 与迁移逻辑。内置版按账户 ID（无 ID 时使用邮箱）分配专用存储名称，子页面不能自行选择另一个账户；退出登录后下一账户只恢复自己的密钥和作品。

生产应用使用稳定来源，正常重启和更新保留数据；开发预览与正式应用数据分开。密钥与原项目一样保存在当前设备的本地配置中，不是系统密钥库加密存储。清除应用数据或换设备前使用原版导出备份。

固定上游版本随 MollyCloud 更新，不在生产运行中自动拉取 main 或热替换页面脚本。MIT 许可证允许修改与商业分发，随包保留 CookSleep 的版权及 MIT 许可证。

## 开发与验证

在 `app` 目录执行：

```powershell
npm run dev:desktop
# 单独构建或完整构建
npm run build:image-workbench
npm run build
# 开发服务运行后，隔离预览与 Mock 联调
npm run check:design
node scripts/verify-image-workbench.mjs
# 独立临时 WebView，真实生产资源与原生请求，模拟账户
cargo build --manifest-path src-tauri/Cargo.toml --example image_workbench_smoke --features image-workbench-smoke
node scripts/verify-image-workbench.mjs --native
```

上游测试在 `vendor/gpt-image-playground` 执行 `npm run build`、`npm test`。生成的 `app/public/image-workbench` 不能手工编辑。

自动联调覆盖默认配置、手动保存及写入失败、自定义地址、文字生图、原版图片下载、原版参考图上传和编辑请求、流式响应、取消、HTTP 错误、多段参考图/遮罩上传、切换保留、历史恢复和账户隔离。原生进程退出后重新使用同一临时数据目录，验证密钥、自定义地址和作品仍能恢复。截图及报告输出到 `artifacts/image-workbench/`、`artifacts/image-workbench-native/`、`artifacts/design-review/`。

2026-09-12 验证：登录和七个页面、三种尺寸共 36 组设计检查通过；上游 557 项测试、宿主 7 项测试、原生地址限制测试通过；浏览器与 Windows WebView2 的 Mock 联调及进程重启恢复通过。原生测试使用生产构建资源、独立临时配置和模拟账户，不操作真实桌宠或账户。

自动测试不发送真实付费 API 请求；Mock 通过不能证明线上具体模型、Agent 或所有第三方服务商兼容，实际使用按服务商开放的能力配置。

## 来源

- [项目功能与部署说明](https://github.com/CookSleep/gpt_image_playground/blob/da4fda85b59ecacc51d6a1e2ef680e3abb9e29b8/README.md)
- [v0.7.12 发布页](https://github.com/CookSleep/gpt_image_playground/releases/tag/v0.7.12)
- [前端依赖](https://github.com/CookSleep/gpt_image_playground/blob/da4fda85b59ecacc51d6a1e2ef680e3abb9e29b8/package.json)
- [图片 API 适配](https://github.com/CookSleep/gpt_image_playground/blob/da4fda85b59ecacc51d6a1e2ef680e3abb9e29b8/src/lib/openaiCompatibleImageApi.ts)
- [Agent API](https://github.com/CookSleep/gpt_image_playground/blob/da4fda85b59ecacc51d6a1e2ef680e3abb9e29b8/src/lib/agentApi.ts)
- [IndexedDB 存储](https://github.com/CookSleep/gpt_image_playground/blob/da4fda85b59ecacc51d6a1e2ef680e3abb9e29b8/src/lib/db.ts)
- [MIT 许可证](https://github.com/CookSleep/gpt_image_playground/blob/da4fda85b59ecacc51d6a1e2ef680e3abb9e29b8/LICENSE)
