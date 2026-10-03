# MollyCloud 应用

Vue 3 / TypeScript 控制台、Petra 透明桌宠和 Tauri 2 / Rust 原生层。macOS 13+ 与 Windows 使用独立的平台窗口、系统接口、凭据与打包配置，内嵌工具继续保持各自来源和存储边界。

## 开发

```sh
npm ci
npm run dev:desktop
```

macOS 构建和发行归档详见 [macOS 开发](../docs/macOS开发.md)，完整功能与 Windows 安装入口详见 [项目 README](../README.md)。Mac 不编译或分发 NetSpeed Dynamic 灵动岛。

```sh
npm test
npm run build
npm run package:macos
# Vite 开发服务运行时：
npm run check:design
```

控制台复用 [设计规范](../design-system/molly-desktop/MASTER.md) 的浅色、深色与系统主题。Petra 透明窗口保持独立布局。UI 回归使用临时用户目录，账户和工具测试使用模拟凭据；不操作用户已有桌宠或真实账户。

登录刷新令牌保存在系统凭据存储，Access Token 只在 Rust 内存。助手密钥使用本机加密存储；生图密钥由用户手动填写，不能自动读取账户密钥。CC Switch 的供应商导入只入库，点击启用才写入实际工具配置；供应商库、设置和备份位于 MollyCloud 私有目录。

Petra 原始快照位于 `third_party/Petra/`。模型、Live2D Cubism Core、素材和上游项目归属详见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。正式发行前需要确认各模型与运行时的分发许可。
