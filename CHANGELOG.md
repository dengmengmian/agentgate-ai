# Changelog / 更新日志

All notable changes to this project will be documented in this file.

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
