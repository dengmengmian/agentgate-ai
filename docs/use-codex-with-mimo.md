# Use Codex with Xiaomi MiMo through AgentGate

中文：让 Codex 使用小米 MiMo

AgentGate lets Codex use Xiaomi MiMo through a local AI gateway. Codex sends OpenAI Responses API requests to AgentGate, and AgentGate routes them to MiMo models with model mapping, reasoning support, capability checks, and provider-specific handling.

## When to use this

Use this guide if you want:

- Codex to call Xiaomi MiMo models without manually editing Codex config files.
- Codex `web_search_preview` requests translated to MiMo `web_search` when the selected MiMo model supports it.
- MiMo reasoning content preserved across multi-turn coding conversations.
- Automatic handling for MiMo Open API keys and Token Plan keys.
- One local gateway that can switch Codex between MiMo, DeepSeek, OpenAI, Kimi, GLM, DashScope, or another provider.

## Quick Setup

1. Download AgentGate from [Releases](../../releases) and open the app.
2. Go to **Quick Setup** or **Providers**.
3. Add a Xiaomi MiMo provider and paste your MiMo API key.
4. Let AgentGate detect the key type and fill the base URL, protocol endpoints, default model, and capability matrix.
5. Start the gateway from **Overview** or **Gateway**. The default client endpoint is `http://127.0.0.1:9090`.
6. Open **Clients** and click **Apply Config** on the Codex card.
7. Send a test message in Codex.

AgentGate keeps the official Codex configuration restorable, so you can switch back from the Codex card when needed.

## What AgentGate configures

| Codex side | AgentGate side | MiMo side |
|---|---|---|
| OpenAI Responses API | `/v1/responses` local gateway route | MiMo Chat or Anthropic-compatible endpoint |
| Codex model names | Model Mapping or `agentgate` virtual model | MiMo model IDs such as `mimo-v2.5-pro`, `mimo-v2.5`, or `mimo-v2-flash` |
| Codex reasoning/tool flow | Protocol conversion and request tracing | MiMo `reasoning_content`, model capability matrix, and web search handling |

## Chinese Notes / 中文说明

如果你搜索的是“Codex 使用小米 MiMo”“Codex 接入 MiMo”“mimo2codex 替代方案”，AgentGate 的定位是一个更通用的本地 AI 网关：它不只连接 MiMo，也能在 MiMo、DeepSeek、OpenAI、Kimi、GLM、通义千问等 Provider 之间切换。

常见路径是：

```text
Codex -> http://127.0.0.1:9090/v1/responses -> AgentGate -> Xiaomi MiMo
```

MiMo 支持多种模型和 key 类型。AgentGate 会按 MiMo Open API / Token Plan 的差异自动处理 host、模型能力、`reasoning_content`、`web_search` 降级和模型映射。

## MiMo-specific behavior

| Area | Behavior |
|---|---|
| Models | Supports MiMo chat models such as `mimo-v2.5-pro`, `mimo-v2-pro`, `mimo-v2.5`, `mimo-v2-omni`, and `mimo-v2-flash`. |
| Token Plan | Keeps Token Plan region hosts consistent for `cn`, `sgp`, and `ams`. |
| Reasoning | Preserves multi-turn `reasoning_content` where MiMo expects it. |
| Web search | Degrades gracefully when the paid MiMo Web Search Plugin is unavailable. |
| Vision | Uses the per-model capability matrix to promote image requests to a vision-capable sibling model when possible. |

## Troubleshooting

| Symptom | Check |
|---|---|
| MiMo returns a host or auth error | Check whether the key is Open API `sk-*` or Token Plan `tp-*`, and verify the region host. |
| MiMo rejects `web_search` | Check whether the paid Web Search Plugin is enabled; AgentGate can strip and retry in supported cases. |
| Image requests fail | Check the MiMo model capability matrix; not every MiMo model supports vision. |
| Gateway is unreachable | Make sure the AgentGate gateway is running on `127.0.0.1:9090`; `1420` is only the development UI port. |

## Related

- [Use Codex Desktop with third-party APIs and plugins](./use-codex-desktop-with-third-party-api-and-plugins.md)
- [Use Codex with DeepSeek](./use-codex-with-deepseek.md)
- [Main README](../README.md)
- [中文 README](../README_ZH.md)
