# MollyCloud 内置适配记录

- 上游：https://github.com/CookSleep/gpt_image_playground
- 固定版本：v0.7.12（2026-09-09）
- 固定提交：`da4fda85b59ecacc51d6a1e2ef680e3abb9e29b8`
- 许可证：MIT，Copyright (c) 2026 CookSleep，见 `LICENSE`。
- 导入方式：GitHub codeload 固定提交源码归档。仅通过 MollyCloud 构建与更新，不在运行时下载源码。

本地修改：

1. `VITE_MOLLY_EMBEDDED` 构建模式和 `mollyBridge.ts` 负责受限 MessageChannel、流式 fetch、配置持久化。
2. `db.ts` 保留原数据格式和迁移逻辑；内置模式通过宿主访问账户专用 IndexedDB，操作在事务提交后返回。
3. 默认 URL/model 由构建传入 `https://mollycloud.cn/v1` / `gpt-image-2`，密钥保持空白；用户填写并保存，不锁定供应商、模型或地址。
4. 不注册 Service Worker，不检查上游新版本，不显示“安装为应用”和首次赞助弹窗；保留原项目名称、关于与版权声明。
5. 设置中增加手动密钥保存反馈，区分 MollyCloud 本地数据与普通浏览器部署。

构建由 `app/scripts/build-image-workbench.mjs` 执行，输出到 `app/public/image-workbench`，随后随主应用打包。不要手工修改构建输出。其他构建保持上游行为。
