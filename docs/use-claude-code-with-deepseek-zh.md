# 用 AgentGate 让 Claude Code 使用 DeepSeek

English: [Use Claude Code with DeepSeek through AgentGate](./use-claude-code-with-deepseek.md)

AgentGate 把 Claude Code 原本发往 Anthropic Messages 的入口，变成你本地可控的模型入口。Claude Code 继续发 Messages API 请求，AgentGate 在本地决定原生直连还是协议转换到 DeepSeek，附带 endpoint 处理、模型映射、故障转移、请求日志和成本统计。

## 什么时候用这个

如果你想：

- 让 Claude Code 调用 DeepSeek 模型，同时保留一键回退到官方配置的能力。
- 想要一个 claude-code-router 这类单客户端路由工具的本地替代方案。
- 在官方 Claude Code 配置和 AgentGate 配置之间一键切换。
- 用一个 Route Profile，以后随时把 Claude Code 在 DeepSeek、MiMo、Anthropic、GitHub Copilot、Kimi 等 Provider 之间切换。

## 快速配置

1. 从 [Releases](../../releases) 下载 AgentGate 并打开应用。
2. 进入 **快速配置** 或 **供应商**。
3. 添加 DeepSeek Provider，粘贴你的 DeepSeek API Key。
4. 在 **概览** 或 **网关** 启动网关。默认客户端端点是 `http://127.0.0.1:9090`。
5. 打开 **客户端**，在 Claude Code 卡片上点 **应用配置**。
6. 在 Claude Code 里发一条测试消息。
7. 确认请求出现在 AgentGate 的 **日志** 里，Provider 和路由符合预期。

## AgentGate 配置了什么

| Claude Code 侧 | AgentGate 侧 | DeepSeek 侧 |
|---|---|---|
| Anthropic Messages API | `/v1/messages` 本地网关路由 | DeepSeek 的 Anthropic 兼容或 Chat 兼容上游 |
| Claude 模型名 | Model Mapping 或 `agentgate` 虚拟模型 | DeepSeek 模型 ID |
| 工具调用和流式输出 | 协议感知路由和追踪 | DeepSeek 专属处理 |

## 给 claude-code-router 用户的说明

AgentGate 不是 claude-code-router 的 drop-in 复刻。它是一个桌面控制台，同时支持 Codex、Gemini CLI、OpenCode，覆盖 Provider 故障转移、成本看板、诊断、请求追踪、一键还原客户端配置。

如果你只需要一个小巧的 Claude Code 路由工具，claude-code-router 就够了。如果你想要一个本地模型入口同时服务多个 AI agent 客户端，用 AgentGate。

## 排查

| 现象 | 检查 |
|---|---|
| Claude Code 还在调用官方 endpoint | 回到 **客户端**，重新应用一次 Claude Code 配置。 |
| DeepSeek 返回模型错误 | 检查 Provider 的默认模型和 Model Mapping。 |
| 工具调用失败 | 看请求日志，确认所选上游支持工具调用。 |
| 网关无法连接 | 确认 AgentGate 在 `127.0.0.1:9090` 上运行；`1420` 只是开发用的 UI 端口。 |

## 相关教程

- [让 Codex 使用 DeepSeek](./use-codex-with-deepseek-zh.md)
- [用 GitHub Copilot 订阅跑 Claude Code](./use-claude-code-with-github-copilot-zh.md)
- [English README](../README.md)
- [中文 README](../README_ZH.md)
