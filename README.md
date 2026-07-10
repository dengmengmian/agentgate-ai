<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">
  <b>One local gateway for your AI model requests.</b><br>
  AgentGate is a local AI gateway for AI apps and clients, including Codex, Claude Code, Gemini CLI, OpenCode, AtomCode, and apps compatible with OpenAI, Anthropic, or Gemini protocols. It routes each request to the model you choose, fails over automatically when a provider breaks, traces everything locally, and bridges protocol differences across OpenAI-compatible providers.
</p>

<p align="center">
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/v/release/dengmengmian/agentgate-ai?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/stargazers"><img src="https://img.shields.io/github/stars/dengmengmian/agentgate-ai?style=flat-square&cacheSeconds=3600" alt="Stars"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/downloads/dengmengmian/agentgate-ai/total?style=flat-square&color=green&cacheSeconds=3600" alt="Downloads"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="./README_ZH.md">中文</a> · <a href="https://github.com/dengmengmian/agentgate-ai/releases">Download</a> · <a href="#5-minute-quick-start">5-Minute Quick Start</a> · <a href="./docs/full-reference.md">Full Reference</a> · <a href="https://github.com/dengmengmian/agentgate-ai/discussions">💬 Discussions</a>
</p>

<p align="center">
  GitHub: <a href="https://github.com/dengmengmian/agentgate-ai">dengmengmian/agentgate-ai</a>
</p>

<p align="center">
  <img src="docs/demo-header-v2.gif" width="800" alt="AgentGate intercepts requests from Claude Code, Codex, and Gemini CLI at a local gateway — converting, passing through, routing, or failing over to 26 providers, with every request traced live">
</p>

## Download

| Your machine | Download |
|---|---|
| macOS Apple Silicon | [AgentGate_1.4.12_aarch64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.12/AgentGate_1.4.12_aarch64.dmg) |
| macOS Intel | [AgentGate_1.4.12_x64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.12/AgentGate_1.4.12_x64.dmg) |
| Windows 10 / 11 | [AgentGate_1.4.12_x64-setup.exe](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.12/AgentGate_1.4.12_x64-setup.exe) |
| Debian / Ubuntu | [AgentGate_1.4.12_amd64.deb](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.12/AgentGate_1.4.12_amd64.deb) |
| Other Linux distros | [AgentGate_1.4.12_amd64.AppImage](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.12/AgentGate_1.4.12_amd64.AppImage) |

On macOS you can also install with Homebrew:

```bash
brew install --cask dengmengmian/tap/agentgate
```

Headless CLI (`agentgate-serve`) tarballs and older versions are on the [Releases](https://github.com/dengmengmian/agentgate-ai/releases) page.

## Why AgentGate

| Official experience intact | Model routing is yours | Every request visible |
|:---|:---|:---|
| Keeps Codex / Claude Code / Gemini CLI / OpenCode / AtomCode usable the way you already use them, with one-click restore to official configs. | Official client requests enter AgentGate first, then route through protocol conversion or native pass-through to the upstream provider you choose. | Route decisions, converted payloads, upstream errors, tokens, cost, latency, and failover attempts are traced locally. |

AgentGate is not a hosted API reseller or a generic proxy. It is a local entry point for AI model requests — every request enters AgentGate first, then routes, converts, fails over, or passes through under your control.

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
| Use Codex with DeepSeek or MiMo | Lets Codex keep sending official Responses API traffic, then converts or passes it through to the selected upstream. |
| Keep Codex Desktop plugins working | Preserves the official OpenAI-authenticated provider path while routing model requests through AgentGate. |
| Use Claude Code with DeepSeek / MiMo / Copilot | Supports Anthropic-compatible pass-through, model-name mapping, and optional GitHub Copilot provider setup. |
| Avoid provider outages or quota stalls | Tries failover providers on configured status codes, keywords, timeouts, and cooldown state. |
| Understand every request | Stores raw and converted payloads, route decisions, upstream errors, token counts, latency, and estimated cost. |

<details>
<summary>Supported providers</summary>

<!-- PROVIDER_CATALOG_TABLE:START -->
| Provider | Type | Native Protocols | Provider-Specific Handling |
|---|---|---|---|
| Xiaomi MiMo | `mimo` | Chat + Anthropic | Multi-turn `reasoning_content` round-trip, region-aware `tp-*` host auto-routing, temperature strip in thinking mode, tool_choice non-auto strip, omni web_search strip, web_search builtin gated by matrix, Web Search Plugin auto-degrade / retry |
| DeepSeek | `deepseek` | Chat + Anthropic | Image stripping with explicit notice, DeepSeek V4 thinking history reasoning backfill, schema cleaning, message reordering |
| Anthropic (Claude) | `anthropic` | Anthropic | `tool_use`/`tool_result`, `input_schema`, thinking budget, native cache_control |
| GitHub Copilot | `copilot` | Chat + Anthropic | GitHub token → Copilot bearer exchange, `x-initiator` billing classification, Claude model dash→dot normalization |
| OpenAI | `openai` | Chat + Responses | None (Responses passthrough or Chat conversion) |
| Google Gemini | `google_gemini` | Chat | None |
| Kimi / Moonshot | `kimi` | Chat | `web_search` → `builtin_function`/`$web_search`, thinking control |
| MiniMax | `minimax` | Chat | Strip reasoning_effort / response_format, `<think>` extraction |
| GLM (Zhipu) | `glm` | Chat | Generic |
| DashScope (Qwen) | `dashscope` | Chat | Generic |
| SiliconFlow | `siliconflow` | Chat | Generic |
| Volcengine (Doubao) | `volcengine` | Chat | Generic |
| Baichuan | `baichuan` | Chat | Generic |
| StepFun | `stepfun` | Chat | Generic |
| SenseNova | `sensenova` | Chat | Drops null strict / response_format / non-function tools, merges system messages |
| Yi (01.AI) | `yi` | Chat | Generic |
| ModelScope | `modelscope` | Chat | Generic |
| xAI (Grok) | `xai` | Chat | Generic |
| Mistral | `mistral` | Chat | Generic |
| Groq | `groq` | Chat | Generic |
| Together | `together` | Chat | Generic |
| Fireworks | `fireworks` | Chat | Generic |
| Cerebras | `cerebras` | Chat | Generic |
| Perplexity | `perplexity` | Chat | Generic |
| Cohere | `cohere` | Chat | Generic |
| OpenRouter | `openrouter` | Chat | None |
| Custom | `custom_openai_compatible` | Chat | None (set Base URL yourself) |
<!-- PROVIDER_CATALOG_TABLE:END -->

</details>

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
