# Use OpenCode with AgentGate

中文：[让 OpenCode 通过 AgentGate 切换多供应商模型](./use-opencode-with-agentgate-zh.md)

AgentGate turns OpenCode's model endpoint into a local AgentGate entry while AgentGate handles provider selection, model mapping, failover, diagnostics, request tracing, and cost tracking.

## When to use this

Use this guide if you want:

- OpenCode to switch between DeepSeek, MiMo, OpenAI, Kimi, GLM, DashScope, or another provider from one local UI.
- The `agentgate` virtual model to resolve to the currently selected route model.
- OpenCode requests to appear in the same request logs and cost dashboard as Codex and Claude Code.
- One-click restore if you want to go back to the previous OpenCode config.

## Quick Setup

1. Download AgentGate from [Releases](../../releases) and open the app.
2. Add at least one provider in **Quick Setup** or **Providers**.
3. Start the gateway from **Overview** or **Gateway**.
4. Open **Clients** and click **Apply Config** on the OpenCode card.
5. Send a test prompt from OpenCode.
6. Check AgentGate **Logs** to confirm the OpenCode client, selected provider, model, and route.

## What AgentGate configures

| OpenCode side | AgentGate side | Provider side |
|---|---|---|
| OpenAI-compatible endpoint | `/v1/chat/completions` local gateway route | Chat-compatible upstream |
| `openai/agentgate` virtual model | Route-selected real model | Provider-specific model ID |
| Client config | Snapshot before write | One-click restore from AgentGate |

## Troubleshooting

| Symptom | Check |
|---|---|
| OpenCode keeps using the old model | Apply the OpenCode config again and restart OpenCode if needed. |
| The provider rejects the model | Check Model Mapping and the provider default model. |
| Requests do not show in logs | Confirm OpenCode is using `127.0.0.1:9090` as its base URL. |
| You changed providers but OpenCode still sends an old model name | Use the `agentgate` virtual model path rather than hardcoding a provider model. |

## Related

- [Use Gemini CLI with AgentGate](./use-gemini-cli-with-agentgate.md)
- [Use Codex with DeepSeek](./use-codex-with-deepseek.md)
- [Main README](../README.md)
- [中文 README](../README_ZH.md)
