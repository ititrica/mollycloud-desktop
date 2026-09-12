# Infinite Canvas 集成评估

评估日期：2026-09-12。仅完成源码与发布物评估，尚未安装或接入 MollyCloud。

状态：用户已改选 CookSleep/gpt_image_playground，本方案暂不推进。当前接入方向见 `GPTImagePlayground集成方案.md`。

结论：优先采用“控制台管理可选下载、安装与打开，画布在独立窗口运行”。可以作为 MollyCloud 扩展提供，但当前不适合把整个项目直接并入控制台主界面。

## 已核实的上游现状

- 仓库为 `basketikun/infinite-canvas`，当前最新发行版 `v0.18.0`，发布于 2026-09-07；本次读取的 `main` 提交为 `d213a74614e0e4bd8a26383d1e1e907249e9c61b`。
- 核心是 React 19 + Vite 的静态网页应用。官方 Docker 镜像用 Nginx 提供前端，AI 请求由浏览器直接发送到用户配置的接口；基础画布不需要独立业务后端。
- 检查的最新三个 GitHub Release 均没有上传独立安装包。不能把 GitHub 源码压缩包当成 Windows 安装器；若提供“一键安装”，需要由 MollyCloud 构建并发布经过验证的组件资源包。
- 画布、素材、生成历史和 API 配置主要保存在浏览器本地。原作者明确说明仍处于开发阶段，不保证历史数据格式兼容。
- 基础功能不依赖 Canvas Agent。可选 `@basketikun/canvas-agent` 当前源码版本为 `0.6.0`，要求 Node.js ≥18，含 `@openai/codex` 依赖；默认监听 `127.0.0.1:17371`，使用连接令牌和 Origin 绑定。网页侧边栏当前开放 Codex，Claude 适配代码保留但入口未开放。
- 另有可选 `canvas-proxy` 用于不支持浏览器跨域请求的模型接口；是否需要取决于具体服务的 CORS 配置。
- MIT 许可证允许修改及商业分发，需要保留版权与许可证。README 还请求二次开发保留作者信息及前端页面标识，产品接入时建议一并保留。

## 方案比较

| 方案 | 用户体验 | 维护成本与限制 | 建议 |
| --- | --- | --- | --- |
| 可选安装后直接嵌入控制台 | 单窗口切换，入口集中 | 画布空间被侧栏压缩；需处理快捷键、下载、文件选择、存储隔离及第三方插件权限；上游更新与宿主耦合更多 | 后续可选显示方式 |
| 控制台管理组件，独立窗口打开 | 控制台展示安装状态，画布拥有完整工作空间 | 需要组件下载器和独立数据区，但资源、更新和故障更容易隔离 | 首选 |
| 仅提供官网链接 | 交付最快，无本地安装 | 数据属于官网的浏览器存储，依赖在线站点与其更新，无法称作本地安装 | 可提供“在线体验”辅助入口 |

## 建议的第一阶段

控制台增加“扩展工具 / 无限画布”入口，展示版本、状态和大小。未安装时提供“下载安装”，安装后提供“打开”“更新”“卸载”；画布使用原版 UI，在独立窗口中打开。可以另提供标明联网访问的“在线体验”。

下载的是 MollyCloud 发布的固定版本静态资源包，校验完整性后原子切换版本。基础画布由桌面宿主提供本地静态资源，不要求用户安装 Docker、Bun 或开发工具。暂不默认安装 Canvas Agent、Canvas Proxy，也不执行上游 `npx ...@latest` 来替代受控安装。

画布窗口使用独立、稳定的来源和 WebView 数据目录，版本更新不能改变来源导致 IndexedDB 中的作品看似丢失。组件文件与作品数据分开保存；更新前备份，卸载组件默认保留作品，删除作品提供单独的明确操作。

原版支持远程插件和自定义接口脚本。画布页面不能继承 Molly 控制台的原生命令桥、账户会话或任意命令执行权限。宿主交互只提供必要的受限接口，外部链接通过明确的链接打开流程处理。

## API 与 Agent 接入边界

可以后续增加从 Molly API 密钥页“一键配置画布”的受限通道，但不在 URL、日志或启动参数里传递密钥。首先验证 Molly 服务实际开放的图片生成、图片编辑、视频与聊天接口；“兼容 OpenAI”不等于所有画布能力都能使用。

Canvas Agent 作为第二阶段独立可选组件。它有自己的后台进程、端口、配置、Codex 会话及 MCP 生命周期。接入时需要固定版本、检查端口归属、使用私有配置并管理退出清理，不能默认改写系统 Codex/Claude 配置，也不能与内置 CC Switch 共用未经明确约定的配置目录。

## 验收范围

实现时至少验证首次安装与失败重试、重复打开、安装版本回滚、更新后作品保留、卸载保留数据、离线打开基础画布、真实图片接口及 CORS、第三方页面无法调用宿主高权限命令。Agent 功能单独验收，不以基础网页能打开替代 CLI/MCP 链路验证。

## 来源

- [README 与功能说明](https://github.com/basketikun/infinite-canvas/blob/d213a74614e0e4bd8a26383d1e1e907249e9c61b/README.md)
- [v0.18.0 发布页](https://github.com/basketikun/infinite-canvas/releases/tag/v0.18.0)
- [静态前端构建与运行镜像](https://github.com/basketikun/infinite-canvas/blob/d213a74614e0e4bd8a26383d1e1e907249e9c61b/Dockerfile)
- [前端依赖](https://github.com/basketikun/infinite-canvas/blob/d213a74614e0e4bd8a26383d1e1e907249e9c61b/web/package.json)
- [快速开始与数据存储](https://github.com/basketikun/infinite-canvas/blob/d213a74614e0e4bd8a26383d1e1e907249e9c61b/docs/content/docs/overview/quick-start.mdx)
- [Canvas Agent 说明](https://github.com/basketikun/infinite-canvas/blob/d213a74614e0e4bd8a26383d1e1e907249e9c61b/canvas-agent/README.md)
- [MIT 许可证](https://github.com/basketikun/infinite-canvas/blob/d213a74614e0e4bd8a26383d1e1e907249e9c61b/LICENSE)
