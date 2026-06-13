# 让 OpenCode 通过 AgentGate 切换多供应商模型

English: [Use OpenCode with AgentGate](./use-opencode-with-agentgate.md)

AgentGate 把 OpenCode 的模型 endpoint 变成一个本地 AgentGate 入口，由 AgentGate 接管 Provider 选择、模型映射、故障转移、诊断、请求追踪和成本统计。

## 什么时候用这个

如果你想：

- 在一个本地 UI 里让 OpenCode 在 DeepSeek、MiMo、OpenAI、Kimi、GLM、DashScope 等 Provider 之间切换。
- 让 `agentgate` 虚拟模型解析成当前路由选中的真实模型。
- 让 OpenCode 的请求和 Codex、Claude Code 出现在同一份请求日志和成本看板里。
- 想回到上一版 OpenCode 配置时能一键还原。

## 快速配置

1. 从 [Releases](../../releases) 下载 AgentGate 并打开应用。
2. 在 **快速配置** 或 **供应商** 添加至少一个 Provider。
3. 在 **概览** 或 **网关** 启动网关。
4. 打开 **客户端**，在 OpenCode 卡片上点 **应用配置**。
5. 在 OpenCode 里发一条测试 prompt。
6. 在 AgentGate 的 **日志** 里确认 OpenCode 客户端、选中的 Provider、模型和路由都符合预期。

## AgentGate 配置了什么

| OpenCode 侧 | AgentGate 侧 | Provider 侧 |
|---|---|---|
| OpenAI 兼容 endpoint | `/v1/chat/completions` 本地网关路由 | Chat 兼容上游 |
| `openai/agentgate` 虚拟模型 | 由路由选中的真实模型 | Provider 对应的模型 ID |
| 客户端配置 | 写入前先做快照 | 在 AgentGate 里一键还原 |

## 排查

| 现象 | 检查 |
|---|---|
| OpenCode 还在用旧模型 | 重新应用一次 OpenCode 配置，必要时重启 OpenCode。 |
| Provider 拒绝模型 | 检查 Model Mapping 和 Provider 的默认模型。 |
| 请求不出现在日志里 | 确认 OpenCode 用的是 `127.0.0.1:9090` 作为 base URL。 |
| 切换了 Provider 但 OpenCode 还在发旧模型名 | 用 `agentgate` 虚拟模型这条路径，别在 OpenCode 里硬编码具体 Provider 的模型名。 |

## 相关教程

- [让 Gemini CLI 通过 AgentGate 使用多供应商模型](./use-gemini-cli-with-agentgate-zh.md)
- [让 Codex 使用 DeepSeek](./use-codex-with-deepseek-zh.md)
- [English README](../README.md)
- [中文 README](../README_ZH.md)
