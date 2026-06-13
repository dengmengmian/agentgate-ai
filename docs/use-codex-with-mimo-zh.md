# 让 Codex 使用小米 MiMo

English: [Use Codex with Xiaomi MiMo through AgentGate](./use-codex-with-mimo.md)

AgentGate 把 Codex 原本发往官方 Responses API 的入口，变成你本地可控的模型入口。Codex 继续发 Responses API 请求，AgentGate 在本地决定转换协议或直连小米 MiMo，附带模型映射、reasoning 支持、能力检查和 Provider 专属处理。

## 什么时候用这个

如果你想：

- 让 Codex 调用小米 MiMo 模型，同时保留一键回退到官方配置的能力。
- 当所选 MiMo 模型支持时，把 Codex 的 `web_search_preview` 请求翻译成 MiMo 的 `web_search`。
- 在多轮对话里保留 MiMo 的 reasoning 内容。
- 让 AgentGate 自动处理 MiMo Open API Key 和 Token Plan Key 的差异。
- 用一个本地网关，把 Codex 在 MiMo、DeepSeek、OpenAI、Kimi、GLM、DashScope 等 Provider 之间随意切。

## 快速配置

1. 从 [Releases](../../releases) 下载 AgentGate 并打开应用。
2. 进入 **快速配置** 或 **供应商**。
3. 添加小米 MiMo Provider，粘贴你的 MiMo API Key。
4. 让 AgentGate 检测 Key 类型，自动填好 base URL、协议 endpoint、默认模型和能力矩阵。
5. 在 **概览** 或 **网关** 启动网关。默认客户端端点是 `http://127.0.0.1:9090`。
6. 打开 **客户端**，在 Codex 卡片上点 **应用配置**。
7. 在 Codex 里发一条测试消息。

AgentGate 保留原 Codex 配置的可恢复状态，你可以随时从 Codex 卡片切回官方。

## AgentGate 配置了什么

| Codex 侧 | AgentGate 侧 | MiMo 侧 |
|---|---|---|
| OpenAI Responses API | `/v1/responses` 本地网关路由 | MiMo Chat 或 Anthropic 兼容 endpoint |
| Codex 模型名 | Model Mapping 或 `agentgate` 虚拟模型 | MiMo 模型 ID，如 `mimo-v2.5-pro`、`mimo-v2.5`、`mimo-v2-flash` |
| Codex 的 reasoning 和工具流 | 协议转换和请求追踪 | MiMo 的 `reasoning_content`、模型能力矩阵、web search 处理 |

## 工作原理

AgentGate 的定位不是单个 MiMo 代理，而是把 Codex 的官方模型入口变成本地可控入口：它可以连 MiMo，也能在 MiMo、DeepSeek、OpenAI、Kimi、GLM、通义千问等 Provider 之间切换。

常见路径是：

```text
Codex -> http://127.0.0.1:9090/v1/responses -> AgentGate -> 小米 MiMo
```

MiMo 支持多种模型和 Key 类型。AgentGate 会按 MiMo Open API / Token Plan 的差异自动处理 host、模型能力、`reasoning_content`、`web_search` 降级和模型映射。

## MiMo 专属处理

| 方面 | 行为 |
|---|---|
| 模型 | 支持 MiMo chat 模型，如 `mimo-v2.5-pro`、`mimo-v2-pro`、`mimo-v2.5`、`mimo-v2-omni`、`mimo-v2-flash`。 |
| Token Plan | 为 `cn`、`sgp`、`ams` 三个区域保持一致的 Token Plan host。 |
| Reasoning | 在 MiMo 期望的位置保留多轮 `reasoning_content`。 |
| Web search | 当付费 MiMo Web Search Plugin 不可用时，自动优雅降级。 |
| Vision | 用每个模型的能力矩阵，把图像请求自动升级到同系列里支持视觉的模型。 |

## 排查

| 现象 | 检查 |
|---|---|
| MiMo 返回 host 或鉴权错误 | 看 Key 是 Open API `sk-*` 还是 Token Plan `tp-*`，再确认对应的区域 host。 |
| MiMo 拒绝 `web_search` | 看付费 Web Search Plugin 有没有开启；AgentGate 在支持的场景下会自动剥离并重试。 |
| 图像请求失败 | 看 MiMo 模型的能力矩阵；不是每个 MiMo 模型都支持视觉。 |
| 网关无法连接 | 确认 AgentGate 网关在 `127.0.0.1:9090` 上运行；`1420` 只是开发用的 UI 端口。 |

## 相关教程

- [让 Codex Desktop 使用第三方 API 并保留插件能力](./use-codex-desktop-with-third-party-api-and-plugins-zh.md)
- [让 Codex 使用 DeepSeek](./use-codex-with-deepseek-zh.md)
- [English README](../README.md)
- [中文 README](../README_ZH.md)
