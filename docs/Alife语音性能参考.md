> 历史研究记录：2026-09-17 已按用户要求移除 TTS 功能。下述方案与语音包不再属于当前实现或构建要求。

# Alife 语音性能参考与 Molly 实现

研究日期：2026-09-17。Alife 固定版本：`73d7d9674154f2abbae860605e58a170e56fa61d`。

## 参考结论

- [GenieSpeechModel.cs](https://github.com/BDFFZI/Alife/blob/73d7d9674154f2abbae860605e58a170e56fa61d/sources/Alife.Function/Alife.Function.Speech.Genie/GenieSpeechModel.cs)：组件启动时初始化 Python 和音色，销毁组件时才退出；生成前按角色和文本摘要查询 WAV 文件缓存。实际合成仍调用 `genie.tts`，没有发现替代 Genie 的特殊加速内核。
- [SpeechService.cs](https://github.com/BDFFZI/Alife/blob/73d7d9674154f2abbae860605e58a170e56fa61d/sources/Alife.Function/Alife.Function.Speech/SpeechService.cs)：接收文本片段，合成下一段时播放上一段；每段仍然等待完整音频文件，不能称为单段 PCM 实时输出。
- [XmlStreamExecutor.cs](https://github.com/BDFFZI/Alife/blob/73d7d9674154f2abbae860605e58a170e56fa61d/sources/Alife.Function/Alife.Function.FunctionCaller/XmlFunctionCaller/XmlStreamExecutor.cs)：按配置的分隔符发送内容片段。
- Alife 同时提供 VITS 与在线 Edge-TTS，不能把其他引擎的速度直接视为当前菲比音色的速度。Alife 使用 AGPL-3.0；本次仅参考调度思路，Molly 中的实现独立编写，没有移植其源码。

## 本次修改

1. **等待助手回复期间预加载。** 验证助手配置后即异步发起语音模型准备，包括参考音频的说话人嵌入。预加载失败不影响文字聊天；回复到达时复用同一 Python 进程。
2. **独立处理加载和播放取消。** 播放代次用于拦截过期音频，另一个生命周期代次用于取消模型准备；发送回复不会误杀正在准备的模型。关闭朗读、切换设备和卸载会取消准备，卸载等待子进程退出。
3. **模型常驻可选。** Molly 助手齿轮 → 小助手设置 → 本地语音 → 保持语音模型就绪。默认关闭，保留原来的 120 秒空闲释放。开启且允许自动朗读时，在启动/保存时准备并保留模型；持续占用数 GB 内存。修改偏好仍需要保存，取消不改变已保存值。
4. **有界内存缓存。** 最多 32 MiB、64 段，按最近使用顺序淘汰；缓存键包含文本、音色、语速和设备偏好，音量由播放器实时处理。缓存不会写入磁盘，关闭朗读、设备切换和卸载时清理。命中时跳过模型启动和推理，但仍检查播放代次；界面明确显示复用语音。
5. **更早播放第一段。** 对开头的长句，达到 12 个字符后遇到自然逗号可先生成第一分句，后续保持正常句子分段；保留小数和千位数字。仍由唯一的桌宠播放器负责顺序播放与下一段合成。

菲比基础预设 A、自然语速、可选 DirectML 与 CPU 回退保持当前方案。缓存对全新句子的推理耗时没有帮助；预加载把加载时间移到助手思考期间，不能消除模型本身的计算。

## 验证与测量

环境：i5-12600KF、RTX 3070 Ti、同版本私有 Python / ONNX Runtime DirectML 1.22.0。CPU 与 GPU 串行执行，每组 3 个新进程，以下为中位数，单位秒。引擎测量包括真实音频生成，不使用语音缓存；不包括助手网络请求、Rust 文件校验、IPC 或声卡延迟。

| 测量 | CPU | GPU（DirectML） |
| --- | ---: | ---: |
| 模型、参考音频与依赖加载 | 18.95 | 15.59 |
| 短句冷启动：加载加合成 | 24.80 | 19.35 |
| 提前加载完成后的同一短句合成 | 4.99 | 4.20 |
| 长句等待整段音频 | 18.43 | 11.80 |
| 长句先生成第一自然分句 | 6.67 | 4.21 |

短句为「你好，我是茉莉。」；长句为「我已经整理好了这次修改内容，接下来我们可以一起确认效果，再开始新的任务。」；第一分句为「我已经整理好了这次修改内容，」。分句数据表示第一段更早可用，不表示整段文字已全部生成，CPU 跟不上播放时仍可能出现段间等待。冷启动数据不等同于首次系统磁盘冷缓存，操作系统文件缓存与实时负载会影响结果。原始数据位于 `artifacts/tts-alife/latency/{cpu,gpu}.json`，样例 WAV 也在同目录。

真实 WebView2 回归额外测得，同一句「你好，我是 Molly。」预加载后的 CPU 合成到播放调度为 2.761 秒，重复语句缓存命中为 0.0209 秒（单次观测，另有约 20 毫秒的播放器调度提前量，不含声卡延迟）。缓存仅针对重复内容。该回归使用 Rust debug 构建，带完整文件校验的冷启动为 43.56 秒，不能将其作为正式安装包的启动性能。

验证结果：

- 9 项 Rust 语音测试通过，覆盖配置兼容、缓存键、最近使用淘汰、容量限制、清理和资源校验。
- 7 项前端文本/播放器测试通过，覆盖自然分句、数字、代码过滤、过期音频丢弃和口型停止。
- 语音设置深浅色各 55 项检查通过，覆盖 960×640、1180×760、1440×900，已查看六张截图。首次深色 1180 截图出现浏览器合成拼接异常，独立重跑后恢复正常，布局断言始终通过。
- 23 项隔离 WebView2 检查通过：真实运行包安装、预加载不播放、回复不取消预加载、缓存命中、改音量复用、禁用清缓存、CPU/GPU 切换、停止与过期代次、加载中禁用和卸载。使用静音音频调度，没有调用付费接口。
- `npm run build` 通过，运行包摘要与发行资源检查通过。

日志在 `artifacts/tts-alife/{rust-tests,native-webview,frontend-build,ui-light,ui-dark}.log`；界面截图在 `artifacts/tts-settings/{light,dark}/`。本次没有改变公共布局或共享样式。

复查命令（在指定目录执行）：

```powershell
# app
npx vitest run src/tts/text.test.ts src/petra/audio/TtsPlayer.test.ts
node scripts/verify-tts-settings.mjs
node scripts/verify-tts-settings.mjs --dark
npm run build

# app/src-tauri，需 Vite localhost:24320
cargo test --lib tts::
cargo run --example tts_webview_smoke -- ../../artifacts/tts-genie/install-cache

# 项目根目录，使用同版本私有推理环境；CPU/GPU 串行执行，避免互相竞争
artifacts/tts-genie/.runtime-venv/Scripts/python.exe scripts/tts-genie/verify-latency.py --backend cpu
artifacts/tts-genie/.runtime-venv/Scripts/python.exe scripts/tts-genie/verify-latency.py --backend gpu
```

原生检查使用独立临时目录和静音 WebView2，验证真实运行包、WAV 解码与播放调度，不修改用户配置或重启正在运行的桌宠。

## 剩余边界

目前 Molly 的助手接口仍等待完整文字回复（原生请求 `stream: false`），因此本次是「等待期间加载、回复后分段播放」，不是大模型文字边生成边朗读。要进一步减少长回复的首音等待，需要单独改造流式网络传输、工具调用组装和可朗读内容筛选。

当前运行的安装版来自上一轮 CPU 安装包，尚未包含 DirectML 或本次优化。本次只修改和验证源码，不覆盖安装目录，不生成或安装新的发行包。更新后的适配器需要更新本地语音运行环境，已验证的约 747 MB 模型权重可复用。

## 气泡与语音同步（后续修改）

开启回复朗读时，Live2D 上方的回复气泡等待首段音频实际进入播放时钟再显示完整回复，整段语音（含后续分段）结束后保留 8 秒再淡出。语音合成期间隐藏回复气泡；控制台历史在文字回复返回时正常更新。未开启语音、没有可朗读内容或合成失败时直接显示文字。工具执行和错误提示仍正常显示。

每次回复通过独立 `replyId` 关联原生播放代次和气泡。停止朗读、清空气泡或新的回复会取消旧回调，旧合成结果不会再弹出气泡；设置中的音色试听不会触发回复气泡。显示计时使用 AudioContext 播放时钟，暂停的音频不会因墙钟到期提前显示气泡。

验证：13 项文本/播放器测试、10 项隔离浏览器集成检查通过，浏览器使用真实 AssistantPanel、播放器和静音 Web Audio，只模拟本地接口与聊天回复；没有调用付费模型或操作现有桌宠。测试涵盖首音显示、分段长语音不提前淡出、完整结束后的消失、关闭朗读、合成失败、停止和清空。观察到气泡显示比计划音频开始晚约 44 毫秒（单次，不包含声卡硬件延迟），无浏览器未捕获异常。原生语音模块测试 9 项通过。

复查：在 `app` 下运行 `node scripts/verify-tts-bubble.mjs`（需 Vite 24320）；报告在 `artifacts/tts-bubble/report.json`。这部分没有改动公共样式或桌宠布局。

## 后续安装包（2026-09-17）

按用户要求完成 0.1.3 正式版打包，位于 `output/installers/2026-09-17_141504-0.1.3-tts-sync/`。EXE 为 112,815,827 字节，MSI 为 120,377,344 字节。已核对 MSI 全部 6 个资源、x64/GUI 标记和内置运行包摘要；使用包内实际主程序与运行环境完成 CPU、DirectML 预加载及中文合成检查，均通过。校验值、安装说明及验证记录随安装包存放。未自动安装、发布到服务器或重启用户桌宠。
