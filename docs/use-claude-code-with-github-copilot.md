# Use Claude Code with GitHub Copilot through AgentGate

中文：用 GitHub Copilot 订阅跑 Claude Code

AgentGate can route Claude Code to Claude models available through a GitHub Copilot subscription. This is optional and intended for personal evaluation; using Copilot outside official clients may be a gray area under GitHub's terms.

## When to use this

Use this guide if you want:

- Claude Code to use Copilot-provided Claude models without a separate Anthropic API key.
- AgentGate to exchange a GitHub OAuth token for a Copilot API credential automatically.
- Tool continuations and history compaction requests tagged as agent traffic so they do not consume premium user requests.
- Logs that show the `x-initiator` classification per request.

## Risk disclosure

This feature is optional. If you do not add a `copilot` provider, AgentGate never uses this path.

Using a Copilot subscription outside official clients is a terms-of-service gray area. Community tools with similar behavior have existed for a long time, but account risk cannot be ruled out. Avoid using important corporate accounts for experiments.

## Quick Setup

1. Make sure you have an active GitHub Copilot subscription.
2. Get a GitHub OAuth token. If you are signed into VS Code Copilot, check `~/.config/github-copilot/apps.json` for `oauth_token`.
3. Open AgentGate, go to **Providers**, and add a provider with type **GitHub Copilot**.
4. Paste the `gho_` or `ghu_` token as the API key. AgentGate fills the base URL and model list.
5. Start the gateway from **Overview** or **Gateway**.
6. Open **Clients** and apply the Claude Code config.
7. Send a test message and check **Logs** for provider `GitHub Copilot` and `x-initiator` classification.

## What AgentGate handles

| Area | Behavior |
|---|---|
| Credential exchange | GitHub OAuth token is exchanged for a Copilot bearer credential and renewed automatically. |
| Storage | Copilot credentials are cached by hash and are not stored as plaintext tokens. |
| Premium request classification | User messages are tagged as user traffic; tool continuations and compaction are tagged as agent traffic. |
| Model names | Claude Code model names such as `claude-sonnet-4-6` are normalized to Copilot's expected form. |

## Troubleshooting

| Symptom | Check |
|---|---|
| Token exchange fails | Confirm the token starts with `gho_` or `ghu_` and belongs to an account with Copilot access. |
| Model is rejected | Check the Copilot provider's model list after saving the provider. |
| Premium requests look higher than expected | Open request logs and compare `x-initiator: user` vs `x-initiator: agent`. |
| You want to avoid this path | Remove or disable the Copilot provider and use DeepSeek, MiMo, Anthropic, or another provider. |

## Related

- [Use Claude Code with DeepSeek](./use-claude-code-with-deepseek.md)
- [Use Codex with DeepSeek](./use-codex-with-deepseek.md)
- [Main README](../README.md)
- [中文 README](../README_ZH.md)
