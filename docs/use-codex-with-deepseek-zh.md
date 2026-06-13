# 用 AgentGate 让 Codex 使用 DeepSeek

English: [Use Codex with DeepSeek through AgentGate](./use-codex-with-deepseek.md)

AgentGate 把 Codex 原本发往官方 Responses API 的入口，变成你本地可控的模型入口。Codex 继续发 Responses API 请求，AgentGate 在本地决定转换协议或直连 DeepSeek，附带模型映射、故障转移、请求日志和成本统计。

## 什么时候用这个

如果你想：

- 让 Codex 调用 DeepSeek 模型，同时保留一键回退到官方配置的能力。
- 让 Codex 的 OpenAI Responses API 请求被转换成 DeepSeek 兼容的 Chat Completions，或 Anthropic 兼容 endpoint。
- 在官方 Codex 配置和 AgentGate 配置之间一键切换。
- 用一个本地网关，以后随时把 Codex 从 DeepSeek 切到 MiMo、OpenAI、Kimi、GLM、DashScope 或其他 Provider。

## 快速配置

1. 从 [Releases](../../releases) 下载 AgentGate 并打开应用。
2. 进入 **快速配置** 或 **供应商**。
3. 添加 DeepSeek Provider，粘贴你的 DeepSeek API Key。
4. 在 **概览** 或 **网关** 启动网关。默认客户端端点是 `http://127.0.0.1:9090`。
5. 打开 **客户端**，在 Codex 卡片上点 **应用配置**。
6. 在 Codex 里发一条测试消息。

AgentGate 保留了原 Codex 配置的可恢复状态，你可以随时从 Codex 卡片切回官方。

## AgentGate 配置了什么

| Codex 侧 | AgentGate 侧 | DeepSeek 侧 |
|---|---|---|
| OpenAI Responses API | `/v1/responses` 本地网关路由 | DeepSeek Chat Completions 或 Anthropic 兼容 endpoint |
| Codex 模型名 | Model Mapping 或 `agentgate` 虚拟模型 | DeepSeek 模型 ID，如 `deepseek-v4-flash` 或 `deepseek-v4-pro` |
| Codex 的工具和流式输出 | 协议转换和请求追踪 | DeepSeek 专属处理 |

## 工作原理

AgentGate 的作用，是把 Codex 原本发往官方的 Responses API 入口变成本地模型入口，再由本地决定转换或直连到 DeepSeek。

常见路径是：

```text
Codex -> http://127.0.0.1:9090/v1/responses -> AgentGate -> DeepSeek
```

你不需要长期手改 Codex 配置文件，也不需要在 DeepSeek、MiMo、OpenAI 等 Provider 之间来回改模型名。AgentGate 会通过 Provider、Route Profile、Model Mapping 和 `agentgate` 虚拟模型处理这些差异。

## 排查

| 现象 | 检查 |
|---|---|
| Codex 还在调用官方 endpoint | 回到 **客户端**，重新应用一次 Codex 配置。 |
| DeepSeek 返回模型错误 | 检查 DeepSeek Provider 的默认模型和 Model Mapping。 |
| 网关无法连接 | 确认 AgentGate 网关在 `127.0.0.1:9090` 上运行；`1420` 只是开发用的 UI 端口。 |
| 想恢复官方 Codex | 在 **客户端** 页用 Codex 卡片上的切回官方动作。 |

## 相关教程

- [让 Codex Desktop 使用第三方 API 并保留插件能力](./use-codex-desktop-with-third-party-api-and-plugins-zh.md)
- [让 Codex 使用小米 MiMo](./use-codex-with-mimo-zh.md)
- [English README](../README.md)
- [中文 README](../README_ZH.md)
