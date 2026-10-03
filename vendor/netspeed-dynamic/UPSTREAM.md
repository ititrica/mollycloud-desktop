# NetSpeed Dynamic 内置适配

- 来源：https://github.com/GEORGEWWWU/NetSpeed-Dynamic
- 上游版本：2.4.6
- 固定提交：`422931fbd4927a98e242bbd1d1d144b3dab2dffa`（2026-10-01）
- 许可证：MIT，完整文本见 `LICENSE`。

`frontend/views/WidgetIsland.vue`、`frontend/i18n.ts` 和 `backend/src` 来自上游。控制台由 `app/src/components/NetSpeedPanel.vue` 使用 MollyCloud 共享主题呈现，入口、状态及流量适配由 `bridge.ts`、`traffic.ts` 与 `app/src/netspeed.ts` 提供。

这是静态编译进 MollyCloud 的内置功能，不进入功能插件目录，不提供单独安装、卸载或更新。Rust 的内部 Tauri 命令命名空间只承担 IPC 路由，不是可安装插件。

移除上游主程序入口、单实例注册、托盘、独立自启动和窗口生命周期；独立窗口标签改为 `netspeed-widget`，事件使用 `molly-netspeed:` 前缀。外观和流量数据仅使用 MollyCloud WebView 用户目录内的 `mollycloud:netspeed:` 存储，设计预览使用独立前缀。

原版可选的任务栏与 FPS 功能使用仓库自带的两个辅助程序。它们不是 Molly 的可安装插件，默认不运行；仅从安装资源的固定路径加载并校验 SHA-256，以 Windows Job Object 跟随宿主退出。不从当前目录、PATH 或用户下载目录查找程序；不通过 shell 启动。

| 文件 | SHA-256 |
| --- | --- |
| NSD_Fps_Plugin.exe | 60c86f785a63f86d1e614a33676e20bbe54969657662c758a69cfe8b862bb810 |
| NSD_Taskbar_Plugin.exe | 94c56b721c807461f7d886ab4ce99e80a0d0058cd22de1c90763da7209b88a3b |

更新时应比较此提交到目标版本的源代码、能力权限、端口、辅助程序及许可证；重新计算和审查辅助程序哈希，运行隔离原生测试、界面检查和安装资源检查，再随 MollyCloud 主程序发布。
