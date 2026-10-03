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
- 原版深色样式通过 iframe 继承的 `color-scheme` 跟随控制台解析后的外观，支持明确深/浅色和系统变化；内嵌版复用 Molly 共享主题 token、荧光绿强调、字体和光标颜色，保留原版布局与交互。切换不重载工作台，不改动 sandbox 权限。
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


2026-10-02 更新：工作台资源现支持独立插件包的安装、卸载、更新和回退；此前“只随主应用发布”的描述已由 [功能插件方案](功能插件方案.md) 中的版本兼容边界取代。原生适配、私有目录和生图隔离要求保持有效。

## 2026-10-03 Images 流式尾部截断修复

用户提供的错误记录包含未闭合的 `b64_json` JSON 字符串。解码 PNG 为 1388×1133，最后 `IDAT` 块声明 48,583 字节数据，当前块至少缺少 19,527 字节才能完整，且没有 `IEND`。这表明缺的是实际响应尾部。

已在宿主网络桥接层复现：`forwardImageRequest()` 原本在 `await invoke('image_request')` 后直接向 sandbox 发送 `end`。原生命令返回时，大块 Channel 消息还可能正在通过缓存取回；`mollyBridge.ts` 收到 `end` 后关闭并删除响应流，迟到的尾部数据会被丢弃。原版 `serverSentEvents.ts` 对完整的大事件可正常解析，其“无法解析”提示只是收到不完整 JSON 后的结果。

修复：原生 `ImageEvent::End { bytes }` 放在同一有序 Channel 的所有 Chunk 之后发送。宿主收到结束标记、且累计接收字节数等于原生总数后才通知子页面关闭流；调用完成后继续等待尾部交付，保留取消与错误处理，并以 30 秒尾部交付超时作为异常防护。没有更改原版工作台 UI、API 配置、手动密钥、opaque sandbox、MessageChannel 权限或网络上限。

验证使用本机模拟接口，未重发真实图片生成：旧代码的大响应 / 字节不匹配 / 尾部取消三项回归均失败，修复后四项桥接回归通过；85 项前端 / 5 项脚本测试、59 项上游 SSE / API 相关测试、2 项原生图片测试通过。完整前端构建、资源检查通过。独立真实 WebView2 的 16 组集成检查通过，包括 4,194,424 字符 PNG base64 SSE 的完整末尾与 DONE、生成 / 下载 / 参考图编辑、取消、错误、账户隔离和 sandbox 无通用 IPC。固定报告与日志在 `artifacts/image-stream-20261003/`。

当前安装器保持原归档，需重建含本修复的原生版本后使用。附件中已丢失的图片尾部不能从这份截断数据本地还原；本次没有自动重试付费生成。

## 2026-10-03 0.2.0 默认预设

新配置默认 OpenAI 兼容接口 / Images API / gpt-image-2.5 / 流式开启 / 中间步骤图像数 1；手动密钥默认空，已有保存配置保持原值。由 `app/scripts/build-image-workbench.mjs` 的构建 URL 参数明确指定，避免改变整个上游项目的默认行为。真实隔离 WebView2 已确认五项默认参数和生成请求 `stream:true`、`partial_images:1`，默认创建与已保存配置保留单测通过。已收入 0.2.0 本地安装器，证据位于 `artifacts/releases/0.2.0/`。
