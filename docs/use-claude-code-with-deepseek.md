# Use Claude Code with DeepSeek through AgentGate

中文：让 Claude Code 使用 DeepSeek

AgentGate lets Claude Code use DeepSeek through a local gateway. Claude Code sends Anthropic Messages API requests to AgentGate, and AgentGate routes them to DeepSeek with endpoint handling, model mapping, failover, request logs, and cost tracking.

## When to use this

Use this guide if you want:

- Claude Code to call DeepSeek models without hand-editing `~/.claude/settings.json`.
- A local alternative to single-client routers such as claude-code-router.
- One-click switching between the official Claude Code config and AgentGate.
- A route profile that can later switch Claude Code between DeepSeek, MiMo, Anthropic, GitHub Copilot, Kimi, or another provider.

## Quick Setup

1. Download AgentGate from [Releases](../../releases) and open the app.
2. Go to **Quick Setup** or **Providers**.
3. Add a DeepSeek provider and paste your DeepSeek API key.
4. Start the gateway from **Overview** or **Gateway**. The default client endpoint is `http://127.0.0.1:9090`.
5. Open **Clients** and click **Apply Config** on the Claude Code card.
6. Send a test message in Claude Code.
7. Confirm the request appears in AgentGate **Logs** with the expected provider and route.

## What AgentGate configures

| Claude Code side | AgentGate side | DeepSeek side |
|---|---|---|
| Anthropic Messages API | `/v1/messages` local gateway route | DeepSeek Anthropic-compatible or Chat-compatible upstream |
| Claude model names | Model Mapping or `agentgate` virtual model | DeepSeek model IDs |
| Tool calls and streaming | Protocol-aware routing and tracing | Provider-specific DeepSeek handling |

## Notes for claude-code-router users

AgentGate is not a drop-in clone of claude-code-router. It is a desktop gateway that also supports Codex, Gemini CLI, OpenCode, provider failover, cost dashboards, diagnostics, and one-click client config restore. If you only need a small Claude Code router, claude-code-router may be enough. If you want one local control plane for multiple coding agents, use AgentGate.

## Troubleshooting

| Symptom | Check |
|---|---|
| Claude Code still calls the official endpoint | Re-open **Clients** and apply the Claude Code config again. |
| DeepSeek returns a model error | Check the provider default model and Model Mapping. |
| Tool calls fail | Check request logs and confirm the selected upstream supports tool calling. |
| Gateway is unreachable | Make sure AgentGate is running on `127.0.0.1:9090`; `1420` is only the development UI port. |

## Related

- [Use Codex with DeepSeek](./use-codex-with-deepseek.md)
- [Use Claude Code with GitHub Copilot](./use-claude-code-with-github-copilot.md)
- [Main README](../README.md)
- [中文 README](../README_ZH.md)
