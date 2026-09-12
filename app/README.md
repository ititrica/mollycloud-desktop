# MollyCloud

Windows 桌面客户端，包含透明 Live2D 桌宠窗口、MollyCloud 用户控制台和可读取账户状态的 Molly AI 助手。

## 本地运行

```powershell
npm install
npm run import:model -- --source "C:\path\to\model" --core "C:\path\to\live2dcubismcore.min.js" --preview "C:\path\to\preview.png"
npm run tauri dev
# 等价的 MollyCloud 开发入口：npm run dev:desktop
```

MollyCloud 账号连接固定 HTTPS 域名 `https://mollycloud.cn`。Access Token 仅保存在 Rust 内存，Refresh Token 保存在 Windows 凭据管理器，控制台展示的 API Key 始终由原生层掩码后再交给界面。

AI 助手设置与对话都位于桌宠窗口，控制台只保留总开关。助手支持 MollyCloud、OpenAI、DeepSeek、Kimi、智谱、通义千问、SiliconFlow、OpenRouter、Groq、Ollama 和自定义 OpenAI 兼容接口；第三方 API Key 由 Windows DPAPI 加密保存。MollyCloud 账户工具只读余额、订阅、用量和掩码密钥状态；Petra 原有的本机助手工具继续保留，涉及命令执行时会在桌宠界面要求确认。程序不会自动充值或修改订阅。

控制台使用 MollyCloud 官网的亮色视觉基线（`#F7F7F4` 背景、品牌蓝与荧光绿）。Petra 桌宠窗口保持透明、无边框、置顶、跳过任务栏，并依据模型和菜单区域动态点击穿透；右键角色可打开 Petra 完整菜单，系统托盘可打开控制台或显示/隐藏 Molly。

## Petra 集成

Petra 0.2.3（提交 `9b4af14efc14696e4b249ecde1ad66008af502e0`）的完整上游源码快照保存在 `third_party/Petra/`。当前桌宠窗口直接运行内化后的 Petra 前端与原生能力，包括右键菜单、拖拽/穿透、漫游、模型管理、助手、日记、抽卡、提醒、托盘、自启动和音频响应；MollyCloud 控制台作为同一 Tauri 应用的第二窗口运行。归属和 MIT 许可证全文见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。

当前是开发阶段，自动更新模块已保留但不会连接 Petra 的发行源；等 MollyCloud 自有签名更新地址和公钥就绪后再启用。

## Live2D 许可提醒

仓库中的模型资源和 Cubism Core 仅用于当前项目本地开发。正式分发前必须确认模型素材、Live2D Cubism SDK/Core 以及相关运行时依赖的发布许可。
