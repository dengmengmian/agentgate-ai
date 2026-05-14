# Changelog / 更新日志

All notable changes to this project will be documented in this file.

## [0.1.4] - 2025-05-14

### Features / 功能

- **Claude Code 一键切换** — 与 Codex 相同的 save/restore 机制，保存原始 settings.json，切换到官方时自动恢复
- **OpenCode 一键配置** — 新增 `tools/opencode.rs` 模块，写入 `~/.config/opencode/opencode.json`，修正配置文件路径
- **路由系统完善** — 按协议自动创建 3 个默认路由（Codex / Claude Code / OpenCode），新增服务商自动加入所有路由链
- **路由 UI 增强** — 新增「创建配置」按钮和表单，支持 inline 重命名，删除按钮始终可见
- **导航重构** — 侧边栏按用户操作流程重排：概览 → 服务商 → 路由 → 网关 → 客户端 → 日志 → 诊断 → 设置

### Improvements / 优化

- 「仪表盘」→「概览」，「工具」→「客户端」，命名更符合用户认知
- 移除路由中未使用的 `client_type` 和 `max_retries` 字段
- 侧边栏移除装饰性星标，「设为默认」仅在同协议多配置时显示
- 路由页选中状态在操作后保持不变（修复跳回第一个的 bug）
- 移除所有 Codex/Claude Code 备份列表 UI 和相关死代码
- 协议显示为可读标签（如 "OpenAI Responses (Codex)"）

### Docs / 文档

- 更新全部 6 张截图为最新 UI
- README 中英文同步更新：导航命名、使用指南顺序、新增 OpenCode 配置章节

---

## [0.1.3] - 2025-05-14

### Features / 功能

- **Codex 配置一键切换** — 新增「切换到官方 / 切换到 AgentGate」按钮，整体保存和恢复 `config.toml` + `auth.json`，切换后 Codex 立即生效
- **保留官方会话** — 应用 AgentGate 配置时自动保存原始 Codex 官方配置（含 OAuth tokens），切回官方后对话记录不丢失
- **多语言切换文案** — 切换按钮、状态提示、污染警告均支持中英双语
- **污染检测与警告** — 检测 auth.json 中 OPENAI_API_KEY 被旧版覆盖的情况，显示黄色警告提示用户修复

### Improvements / 优化

- Codex 配置 apply 改为全量替换（不再 TOML 合并），逻辑更简洁可靠
- 移除 Codex 备份历史列表，统一由切换机制管理配置恢复
- `generate_codex_config` 命令统一调用 `codex::generate_snippet()`，修复模型名不一致 bug（gpt-5 → gpt-5.5）

### Cleanup / 清理

- 移除未使用的 Codex 备份/恢复/预览命令（`backup_codex_config`、`list_codex_backups`、`restore_codex_backup`、`preview_codex_config`）
- 移除死代码：`update_toml_content`、`create_backup`、`ConfigPreview`、`BackupResult`（codex 模块）
- 移除前端死导出 `previewClaudeCodeConfig`、死类型 `ClaudeCodeConfigPreview`
- 移除未使用的 i18n key（`tools.preview`、`tools.refresh`）

---

## [0.1.2] - 2025-05-13

### Improvements / 优化

- 版本号统一从 Tauri API 运行时读取，不再硬编码
- 检查更新失败时静默处理，不再弹出错误提示
- Codex 默认模型更新为 `gpt-5.5`
- 工具页面新增备份历史列表和一键恢复功能

---

## [0.1.1] - 2025-05-13

### Features / 功能

- **应用图标** — 全新轨道中心 Logo，替换默认 Tauri 图标
- **应用内自动更新** — 集成 tauri-plugin-updater，启动时自动检查 GitHub Releases 新版本
- **设置页检查更新** — 关于区域新增「检查更新」按钮和 GitHub 链接
- **工具配置备份列表** — Codex 和 Claude Code 配置支持查看备份历史和一键恢复
- **CI/CD 自动发版** — GitHub Actions 多平台构建（macOS ARM/Intel、Windows、Linux），push tag 自动发布
- **中英双语 README** — README.md（中文）+ README_EN.md（英文），含截图和安装说明

### Bug Fixes / 修复

- 修复 Codex 配置默认模型从 `gpt-5` 更新为 `gpt-5.5`
- 修复 `.gitignore` 中 `logs` 规则误排除 `src/components/logs/` 导致 CI 构建失败
- 修复侧边栏 Logo 图标与新品牌风格不一致
- 精简关于页面，移除技术栈细节，只保留版本号和协议

---

## [0.1.0] - 2025-05-13

First public release.

### Features / 功能

- **协议转换网关** — 支持 OpenAI Responses API、Anthropic Messages API、Chat Completions 三种协议的统一转换与转发
- **多 Provider 管理** — 支持 DeepSeek、OpenAI、OpenRouter、Kimi 及自定义 OpenAI 兼容接口
- **Route Profile 路由配置** — 多 Provider 优先级链，支持手动切换与自动故障转移（failover）
- **DeepSeek 深度兼容** — reasoning_content 思考模式完整支持、SSE 规范兼容、5 项专项修复
- **Codex 完整适配** — 通过 codex-app-transfer 审计，关闭 12 项兼容性差距
- **Anthropic Messages API 原生透传** — 支持 Claude Code 直连原生 Anthropic 接口
- **工具一键配置** — Codex（config.toml + auth.json）、Claude Code（settings.json）、OpenCode 一键写入
- **模型映射** — 客户端模型名称到 Provider 模型名称的自定义映射
- **Provider 额外请求头** — 支持 Kimi web_search 等 Provider 特有功能
- **Reasoning effort 传递** — 支持 low/medium/high 推理强度透传，自动标准化 xhigh/max/auto/none 等变体
- **Token 用量统计** — 仪表盘展示输入/输出 Token、每日消耗图表
- **仪表盘自动刷新** — 每 5 秒自动更新状态数据
- **多轮对话** — previous_response_id session store 支持
- **本地令牌认证** — ag_local_* 令牌，自动生成，权限 0600
- **请求日志** — 完整记录原始请求、转换请求、上游响应，支持脱敏
- **诊断自检** — Gateway、Provider、配置、数据库全面检查，支持导出诊断包
- **系统托盘** — 关闭窗口后台运行，托盘菜单控制启停
- **开机自启** — auto_start 开关，app 启动时自动启动 Gateway
- **中英双语界面**
- **配置自动备份与恢复**

### Bug Fixes / 修复

- 修复 Provider 活跃状态在 providers/route_profiles/gateway_settings 间同步问题
- 修复 Messages 路由在无匹配 route profile 时回退到 openai_responses 的逻辑
- 修复 SSE 解析器不支持 `data:` 无空格格式（Kimi 兼容）
- 修复 Codex reasoning effort `xhigh` 映射为 `high`（Kimi 兼容）
- 修复 failover 选择器在 manual 模式下忽略 active_provider_id
- 修复 Tauri v2 command 参数需要 camelCase
- 修复 Provider 表单对话框内容溢出
- 修复日志计数文本不换行
- 修复 Token 统计在数据为 0 时不显示
