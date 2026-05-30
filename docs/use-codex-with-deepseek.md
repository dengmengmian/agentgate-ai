# Use Codex with DeepSeek through AgentGate

中文：让 Codex 使用 DeepSeek

AgentGate lets Codex use DeepSeek through one local gateway. Codex sends OpenAI Responses API requests to AgentGate, and AgentGate routes them to DeepSeek with protocol conversion, model mapping, failover, request logs, and cost tracking.

## When to use this

Use this guide if you want:

- Codex to call DeepSeek models without hand-editing `~/.codex/config.toml`.
- OpenAI Responses API requests from Codex converted to DeepSeek-compatible Chat Completions or Anthropic-compatible endpoints.
- One-click switching between official Codex config and AgentGate config.
- A local gateway that can later switch Codex from DeepSeek to MiMo, OpenAI, Kimi, GLM, DashScope, or another provider.

## Quick Setup

1. Download AgentGate from [Releases](../../releases) and open the app.
2. Go to **Quick Setup** or **Providers**.
3. Add a DeepSeek provider and paste your DeepSeek API key.
4. Start the gateway from **Overview** or **Gateway**. The default client endpoint is `http://127.0.0.1:9090`.
5. Open **Clients** and click **Apply Config** on the Codex card.
6. Send a test message in Codex.

AgentGate keeps the official Codex configuration restorable, so you can switch back from the Codex card when needed.

## What AgentGate configures

| Codex side | AgentGate side | DeepSeek side |
|---|---|---|
| OpenAI Responses API | `/v1/responses` local gateway route | DeepSeek Chat Completions or Anthropic-compatible endpoint |
| Codex model names | Model Mapping or `agentgate` virtual model | DeepSeek model IDs such as `deepseek-v4-flash` or `deepseek-v4-pro` |
| Codex tools and streaming | Protocol conversion and request tracing | Provider-specific DeepSeek handling |

## Chinese Notes / 中文说明

如果你搜索的是“Codex 使用 DeepSeek”“Codex 接入 DeepSeek”“DeepSeek 作为 Codex 后端”，AgentGate 的作用是把 Codex 的 OpenAI Responses API 请求统一接到本地网关，再转发到 DeepSeek。

常见路径是：

```text
Codex -> http://127.0.0.1:9090/v1/responses -> AgentGate -> DeepSeek
```

你不需要长期手改 Codex 配置文件，也不需要在 DeepSeek、MiMo、OpenAI 等 Provider 之间来回改模型名。AgentGate 会通过 Provider、Route Profile、Model Mapping 和 `agentgate` 虚拟模型处理这些差异。

## Troubleshooting

| Symptom | Check |
|---|---|
| Codex still calls the official endpoint | Re-open **Clients** and apply the Codex config again. |
| DeepSeek returns a model error | Check the DeepSeek provider's default model and Model Mapping. |
| Gateway is unreachable | Make sure the AgentGate gateway is running on `127.0.0.1:9090`; `1420` is only the development UI port. |
| You want to restore official Codex | Use the Codex card's switch-back action in **Clients**. |

## Related

- [Use Codex Desktop with third-party APIs and plugins](./use-codex-desktop-with-third-party-api-and-plugins.md)
- [Use Codex with Xiaomi MiMo](./use-codex-with-mimo.md)
- [Main README](../README.md)
- [中文 README](../README_ZH.md)
