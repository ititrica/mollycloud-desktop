# MollyCloud Skill 管理器

## 来源与功能

基于 [xingkongliang/skills-manager](https://github.com/xingkongliang/skills-manager) 1.40.0（MIT），固定 commit `6ae02e39d9efea0faf75e643b8205f97833a593d`。完整版权文本见 `app/THIRD_PARTY_NOTICES.md` 和 vendor/LICENSE。

| 能力 | Molly 中的位置 |
| --- | --- |
| 技能统计、Agent 检测 | 概览 |
| 技能查看、标签、更新、批量操作、导出与删除 | 技能库 |
| skills.sh 市场、Git、本地文件夹与 ZIP 导入 | 发现与安装 |
| 本机 Agent 全局技能、已有技能导入、部署与移除 | Agent 工作区 |
| 预设管理、项目扫描与关联目录 | 分组与项目 |
| Git 备份、快照、恢复、冲突处理 | 备份同步 |
| 54+ Agent、目录覆盖、自定义 Agent、复制/链接、库迁移、代理、更新和日志 | 设置 |

Molly 只管理技能文件和部署，不作为 Agent 的执行代理。首次打开才初始化模块；新库默认复制部署，启动不会自动部署默认预设。定时更新与备份依照用户保存的设置执行。

## 横向菜单标准

内嵌菜单复用概览的 `app/src/subnav.css` 和 `app/src/navigationMotion.ts`，保持透明底、字体、荧光绿滑动下划线及内容淡入淡出一致。快速切换取消旧导航请求；结束后移除动画 transform，避免影响弹层定位。窄窗口只滚动横向菜单本身，减少动态效果设置下直接切换。后续新增分类继续使用共享规则。

## 数据与调用边界

默认私有根为 `%APPDATA%/cn.mollycloud.client/skills-manager`；`library` 包含数据库和技能，`repo-config.json` 记录库位置，`bundled` 仅存放管理技能说明。库迁移先保存意图，下次启动迁移，本次会话仍使用原库。拒绝把新旧库设置成父子目录，并保护独立版 `.skills-manager`、`.agent-skills` 和配置目录及其父子目录，不自动搬迁独立版数据。

Agent 的实际技能目录只在用户明确安装、部署、移除等操作时修改。未受管理的冲突目录保留，不能用覆盖方式接管。Git 凭据采用独立 `mollycloud-skills-git-backup` namespace。GitHub 设备授权使用上游 Skills Manager OAuth 应用，页面明确说明，也支持用户自己的令牌或 Git 远程仓库。

前端为同源内嵌 React 内容区，公共框架由 Molly Vue 控制。仅允许 `console` 窗口调用 `plugin:molly-skills` 的明确命令列表；初始化前拒绝其他业务命令。没有独立应用退出、重启、托盘、自动启动、更新器或主题控制接口。浏览器预览拒绝真实安装与写入。生图工作台的 sandbox 与来源隔离保持原有边界。

## Agent 自管理 CLI 与安全软件报告

用户在设置中选择 Agent 后可部署 `manage-molly-skills`。该步骤只生成 `SKILL.md`，指向安装目录中的 Molly 主程序：

```powershell
& 'C:/Program Files/MollyCloud/mollycloud.exe' --molly-skills-cli --json skills list
```

`--molly-skills-cli` 必须为第一个参数，直接进入 CLI，不初始化桌面 UI。生成的全部命令示例包含此参数。主程序移动后需重新生成/安装该管理技能。

2026-09-18 用户报告火绒 `Trojan/ShellLoader.gx`，病毒 ID `E0769237CB429657`，路径为 `app/src-tauri/target/debug/deps/molly_skills-a603bbdff0d86822.exe`。该文件是 Cargo 单元测试二进制，不是发布安装器。报告名称不足以判定命中规则或证明误报。

已经移除旧版 CLI bridge 的自复制、staged 可执行文件、后台启动副本验证与版本戳逻辑。不再根据程序文件名进入 CLI，也不自动发布私有 bin 副本。不通过改名、压壳、混淆、关闭安全软件或增加排除规则解决检测问题。

Cargo 的 `target/debug/deps` 测试文件不会作为 Tauri 安装资源发布。安装前后资源检查额外拒绝 sidecar 配置，以及前端、模型目录内的 EXE、DLL、MSI 和启动脚本。当前仓库没有配置 Windows 发行者代码签名证书；正式发布应使用真实证书签名主程序及安装包并添加时间戳，在没有信任/排除规则的测试环境中复扫。若仍报毒，可将检测名称、样本 SHA-256、版本和用途提交火绒复核；此操作需要维护者自行决定，构建不会上传二进制。不能保证所有安全软件不误报。

## 验证边界

回归使用临时 Agent home、私有库、WebView 配置和本地 Git remote，不操作用户真实 Agent 文件或凭据。只运行显式隔离的后端测试，避免运行可能假定真实 home 的上游测试。原生 smoke 覆盖导入、标签、预设、部署保护、本地更新、项目、备份恢复、CLI 文档生成、卸载与迁移。

浏览器检查覆盖八个控制台内容页面（概览含订阅、用量二级菜单）、七个 Skill 子页面、主题、三种窗口尺寸及弹窗焦点。独立 MCP 市场及首次提示已于 2026-10-02 移除。浏览器几何断言不替代实际 WebView IPC 测试；本地 Git fixture 不代表 GitHub 授权或真实网络导入已经验证。安全软件信任区中的成功执行不代表清除误报，需独立复扫。


### 本次验证记录（2026-09-18）

- 前端生产构建及资源检查通过，31 项前端测试通过。
- 浅色、深色各 69 组界面检查通过，外观专项通过；主题实时变化不重载三种嵌入工作台，生图 sandbox 不变。已查看三尺寸公共页面汇总和最小尺寸 Skill 子页截图。
- 隔离 WebView2 回归 37 项检查通过，同时验证独立数据 sentinel、默认复制、无 EXE 副本、卸载与迁移状态。没有连接真实 GitHub 或执行真实 Agent 部署。
- 主程序 `cargo check` 通过；明确 CLI 参数的版本、帮助和无效命令错误输出已验证。
- 移除自复制后，原受信任路径下的两项后端测试曾通过。补齐 Tauri 权限注册后，Cargo 生成的 `molly_skills-d701b4abf5692c9c.exe` 在启动时被拒绝访问，随后文件已不在构建目录；不能据此前一次运行声称最终单元测试已通过或误报已消除。没有修改火绒设置、恢复隔离样本或扩大信任区。

日志位于 `artifacts/skills-manager-research/`；界面报告位于 `artifacts/design-review/{light,dark,appearance}/`。

正式版主程序已使用 `cargo build --release --bin mollycloud --features tauri/custom-protocol` 构建成功；Windows GUI 子系统下通过管道捕获 CLI 的版本、帮助和 JSON 错误输出，退出码分别为 0、0、2，未启动桌面 UI。产物为 `app/src-tauri/target/release/mollycloud.exe`（41,733,120 字节），SHA-256：`a714f2a62d96fb7286488f3bf6fdbe8434fcf139bab654415dd10cfcf5ba45c8`。Authenticode 状态为 `NotSigned`，本轮未生成安装器。构建与 CLI 验证通过不代表独立安全软件复扫通过，详细记录见 `artifacts/skills-manager-research/release-validation.json`。


2026-10-02 更新：工作台资源现支持独立插件包的安装、卸载、更新和回退；此前“只随主应用发布”的描述已由 [功能插件方案](功能插件方案.md) 中的版本兼容边界取代。原生适配、私有目录和生图隔离要求保持有效。
