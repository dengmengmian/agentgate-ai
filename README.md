<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">
  <b>Run Codex, Claude Code, Gemini CLI, OpenCode, and AtomCode through one local model gateway.</b><br>
  Agent-native compatibility · one-click client setup · cost-aware multi-provider routing.
</p>

<p align="center">
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/v/release/dengmengmian/agentgate-ai?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/stargazers"><img src="https://img.shields.io/github/stars/dengmengmian/agentgate-ai?style=flat-square" alt="Stars"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/downloads/dengmengmian/agentgate-ai/total?style=flat-square&color=green" alt="Downloads"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/github/license/dengmengmian/agentgate-ai?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="./README_ZH.md">中文</a> · <a href="https://github.com/dengmengmian/agentgate-ai/releases">Download</a> · <a href="#5-minute-quick-start">5-Minute Quick Start</a> · <a href="./docs/full-reference.md">Full Reference</a>
</p>

<p align="center">
  <img src="docs/demo-header-v2.gif" width="800" alt="AgentGate routes coding agents through a local gateway to cheaper providers with live cost tracking">
</p>

## Download

| Your machine | Download |
|---|---|
| macOS Apple Silicon | [AgentGate_1.4.1_aarch64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_aarch64.dmg) |
| macOS Intel | [AgentGate_1.4.1_x64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_x64.dmg) |
| Windows 10 / 11 | [AgentGate_1.4.1_x64-setup.exe](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_x64-setup.exe) |
| Debian / Ubuntu | [AgentGate_1.4.1_amd64.deb](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_amd64.deb) |
| Other Linux distros | [AgentGate_1.4.1_amd64.AppImage](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_amd64.AppImage) |

Headless CLI (`agentgate-serve`) tarballs and older versions are on the [Releases](https://github.com/dengmengmian/agentgate-ai/releases) page.

## Why AgentGate

| Agent-native compatibility | One-click client setup | Cost-aware routing |
|:---|:---|:---|
| Bridges Codex Responses, Claude Messages, Gemini, and Chat Completions so coding agents can talk to more providers. | Applies and restores Codex / Claude Code / OpenCode / Gemini CLI / AtomCode configs without hand-editing local files. | Routes across 26 providers with failover, capability checks, latency/price strategies, and per-request cost tracking. |

AgentGate is not a hosted API reseller. It is a local-first desktop workbench for people who use coding agents and want provider flexibility without breaking client-specific behavior.

## 5-Minute Quick Start

1. Download and install AgentGate.
2. Open **Quick Setup** or **Providers**, then paste your provider API key.
3. Click **Start Gateway** on **Overview** or **Gateway**. The default endpoint is `127.0.0.1:9090`.
4. On **Clients**, click **Apply Config** for Codex, Claude Code, OpenCode, Gemini CLI, or AtomCode.
5. Send a test message in the client. Use **Switch to Official** or history rollback whenever you want to restore the original config.

Provider presets fill common base URLs, protocols, model defaults, and capability matrices. Most users do not need to touch model mapping or advanced endpoint fields at first.

## Common Uses

| Goal | What AgentGate does |
|---|---|
| Use Codex with DeepSeek or MiMo | Converts Codex Responses API traffic to compatible upstream formats and handles model mapping. |
| Keep Codex Desktop plugins working | Preserves the official OpenAI-authenticated provider path while routing model requests through AgentGate. |
| Use Claude Code with DeepSeek / MiMo / Copilot | Supports Anthropic-compatible pass-through, model-name mapping, and optional GitHub Copilot provider setup. |
| Avoid provider outages or quota stalls | Tries failover providers on configured status codes, keywords, timeouts, and cooldown state. |
| Track spend by model and client | Stores request logs, token counts, latency, route decisions, and estimated cost. |

## Guides

- [Codex Desktop plugins with third-party APIs](./docs/use-codex-desktop-with-third-party-api-and-plugins.md)
- [Codex + DeepSeek](./docs/use-codex-with-deepseek.md)
- [Codex + Xiaomi MiMo](./docs/use-codex-with-mimo.md)
- [Claude Code + DeepSeek](./docs/use-claude-code-with-deepseek.md)
- [Claude Code + GitHub Copilot](./docs/use-claude-code-with-github-copilot.md)
- [Gemini CLI](./docs/use-gemini-cli-with-agentgate.md)
- [OpenCode](./docs/use-opencode-with-agentgate.md)
- [Full reference](./docs/full-reference.md)

## Screenshots

| Dashboard | Providers |
|---|---|
| ![Dashboard](docs/screenshots/dashboard.png) | ![Providers](docs/screenshots/providers.png) |

| Routes | Logs |
|---|---|
| ![Routes](docs/screenshots/routes.png) | ![Logs](docs/screenshots/logs.png) |

## Notes

- GitHub Copilot provider support is optional. Using Copilot subscriptions outside official clients may carry account risk; read the dedicated guide before enabling it.
- The gateway endpoint is `127.0.0.1:9090`; `localhost:1420` is only the development UI port.
- AgentGate is local-first and single-user. If you operate a shared API server or billing platform, tools like one-api, new-api, or LiteLLM may fit better.

## Development

```bash
pnpm install
pnpm tauri dev
```

Useful checks:

```bash
pnpm test
pnpm build
cd src-tauri && cargo test
```

## Community

- Questions and setup help: [Discussions](https://github.com/dengmengmian/agentgate-ai/discussions)
- Bugs and provider requests: [Issues](https://github.com/dengmengmian/agentgate-ai/issues)
- Contributing: [CONTRIBUTING.md](./CONTRIBUTING.md)

## License

MIT
