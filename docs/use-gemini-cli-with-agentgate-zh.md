# 让 Gemini CLI 通过 AgentGate 使用多供应商模型

English: [Use Gemini CLI with AgentGate](./use-gemini-cli-with-agentgate.md)

AgentGate 给 Gemini CLI 一个本地模型入口，让它的请求和 Codex、Claude Code、OpenCode、AtomCode 一起被路由、追踪、统一管理。

## 什么时候用这个

如果你想：

- 让 Gemini CLI 和你其他 AI agent 客户端共享同一个本地模型入口和 Provider 列表。
- 把 Gemini 风格的请求通过 AgentGate 路由，附带模型映射和成本统计。
- 用一键配置代替手编 CLI 配置文件。
- 在主 Provider 限流或失败时自动切到另一个 Provider。

## 快速配置

1. 从 [Releases](../../releases) 下载 AgentGate 并打开应用。
2. 在 **快速配置** 或 **供应商** 添加至少一个 Provider。
3. 在 **概览** 或 **网关** 启动网关。默认端点是 `http://127.0.0.1:9090`。
4. 打开 **客户端**，在 Gemini CLI 卡片上点 **应用配置**。
5. 从 Gemini CLI 发一条测试 prompt。
6. 在 AgentGate 的 **日志** 里确认 Provider 和路由符合预期。

## AgentGate 配置了什么

| Gemini CLI 侧 | AgentGate 侧 | Provider 侧 |
|---|---|---|
| Gemini API 风格的请求 | 本地网关路由 | 由 Route Profile 选中的 Chat 兼容上游 |
| Gemini 模型名 | Model Mapping 或 `agentgate` 虚拟模型 | Provider 对应的模型 ID |
| 请求日志 | 按客户端和路由归因 | 在可用时附带 Token 和成本估算 |

## 排查

| 现象 | 检查 |
|---|---|
| Gemini CLI 还在用原来的 Provider | 在 **客户端** 重新应用一次 Gemini CLI 配置。 |
| 网关无法连接 | 确认 AgentGate 在 `127.0.0.1:9090` 上运行。 |
| 模型名被拒 | 检查 Provider 的默认模型和 Model Mapping。 |
| 成本缺失 | 确认模型在 **设置** 里有内置或自定义的价格。 |

## 相关教程

- [让 Codex 使用 DeepSeek](./use-codex-with-deepseek-zh.md)
- [让 OpenCode 通过 AgentGate 切换多供应商模型](./use-opencode-with-agentgate-zh.md)
- [English README](../README.md)
- [中文 README](../README_ZH.md)
