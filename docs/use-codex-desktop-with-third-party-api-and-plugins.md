# Use Codex Desktop with third-party APIs and plugins

中文：让 Codex 桌面端使用第三方 API，并保留插件能力

AgentGate can route Codex Desktop model requests to third-party providers such as DeepSeek and Xiaomi MiMo while keeping Codex Desktop on its official OpenAI-authenticated provider path. This is useful when you want lower-cost or region-friendly third-party models without giving up Codex Desktop plugin and account features.

## Why this matters

Many proxy setups make Codex behave like a generic OpenAI-compatible client. That can be enough for plain chat, but it may break the parts of Codex Desktop that expect the official OpenAI provider shape and signed-in account state.

AgentGate uses a different pattern:

```text
Codex Desktop
  -> OpenAI provider entry in Codex config
  -> local AgentGate base URL
  -> DeepSeek / Xiaomi MiMo / OpenAI / Kimi / GLM / DashScope / other provider
```

Codex Desktop still sees an OpenAI-style provider entry, while AgentGate receives the model request locally and routes it to the selected upstream provider.

## What keeps working

| Area | What AgentGate preserves |
|---|---|
| Codex Desktop sign-in | Keeps the official `auth.json` account tokens instead of replacing them with a local API key. |
| Plugin and account features | Keeps Codex Desktop on the OpenAI-authenticated provider path that plugin and account features expect. |
| Third-party model routing | Sends model requests through AgentGate to DeepSeek, MiMo, OpenAI, Kimi, GLM, DashScope, or another configured provider. |
| One-click restore | Saves the original Codex config so you can switch back to official behavior from the AgentGate UI. |

Plugin availability still depends on Codex Desktop, your signed-in account, and the upstream feature itself. AgentGate's role is to avoid breaking that path while routing the model request.

## Quick Setup

1. Download AgentGate from [Releases](../../releases) and open the app.
2. Add at least one provider, such as DeepSeek or Xiaomi MiMo.
3. Start the gateway from **Overview** or **Gateway**. The default endpoint is `http://127.0.0.1:9090`.
4. Open **Clients** and click **Apply Config** on the Codex card.
5. Keep your Codex Desktop account signed in.
6. Send a message in Codex Desktop and check AgentGate's request logs to confirm the selected provider was used.

## Chinese Notes / 中文说明

如果你搜索的是“Codex 桌面端 第三方 API”“Codex Desktop DeepSeek 插件”“Codex 桌面端 MiMo 插件”，这个能力是 AgentGate 和普通代理最不一样的地方之一。

普通代理通常只是把 Codex 请求改到一个 OpenAI-compatible 地址。这样能让模型请求跑起来，但 Codex 桌面端里依赖官方 OpenAI provider 形态、登录态、账号能力的部分可能会失效。

AgentGate 的做法是：

- Codex 配置里仍然使用 OpenAI provider 入口。
- `base_url` 指向本地 AgentGate 网关。
- `auth.json` 里的 ChatGPT / OpenAI 登录态保留，不被本地 token 覆盖。
- 本地访问 token 放在 Codex 配置里，由 AgentGate 网关校验。
- 真正的模型请求由 AgentGate 路由到 DeepSeek、小米 MiMo 或其他 Provider。

结果是：Codex 桌面端可以继续保持它熟悉的官方 provider 语义，同时你可以在 AgentGate 里切换第三方 API。

## How it differs from a simple proxy

| Simple proxy | AgentGate |
|---|---|
| Usually treats Codex as a generic OpenAI-compatible client. | Preserves Codex Desktop's OpenAI provider path while changing the local base URL. |
| May require replacing auth with a proxy API key. | Keeps official account tokens and stores the local gateway token in config. |
| Often focuses on one provider. | Supports route profiles, failover, model mapping, and multiple providers. |
| Harder to switch back cleanly. | Provides one-click switch back to official Codex config. |

## Related

- [Use Codex with DeepSeek](./use-codex-with-deepseek.md)
- [Use Codex with Xiaomi MiMo](./use-codex-with-mimo.md)
- [Main README](../README.md)
- [中文 README](../README_ZH.md)
