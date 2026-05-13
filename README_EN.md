<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">Local gateway for AI coding agents.</p>

<p align="center">
  <a href="./README.md">中文</a>
</p>

AgentGate is a local model gateway and provider-switching tool for AI coding agents. It provides a unified local entry point for CLI tools like Codex, Claude Code, and OpenCode, with protocol conversion, multi-provider switching, automatic failover, and request logging.

## Features

**Protocol Conversion & Unified Entry Point**
- OpenAI Responses API (`/v1/responses`) → Chat Completions conversion, for Codex
- Anthropic Messages API (`/v1/messages`) → Chat Completions conversion, for Claude Code
- Chat Completions (`/v1/chat/completions`) pass-through forwarding
- Full DeepSeek reasoning_content (thinking mode) support without degradation
- Tool call (function_call) streaming reassembly and multi-turn conversations

**Multi-Provider Management**
- Supports DeepSeek, OpenAI, OpenRouter, Kimi, and custom OpenAI-compatible endpoints
- Route Profiles for configuring multi-provider priority chains
- Manual switching or automatic failover
- Provider cooldown and runtime status tracking
- Per-request failover: Provider A fails → automatically tries Provider B
- Automatic model list fetching from providers

**Tool Configuration Management**
- One-click Codex configuration (config.toml + auth.json)
- One-click Claude Code configuration (settings.json with inline token)
- Automatic config backup and restore
- Local gateway access token (`ag_local_*`) authentication

**Desktop Application**
- System tray with background operation on window close
- Tray menu for gateway start/stop control
- Auto-start on system boot
- Request logging, self-diagnostics, and diagnostic bundle export
- Bilingual UI (Chinese and English)

## Screenshots

| Dashboard | Tool Configuration |
|:---:|:---:|
| ![Dashboard](docs/screenshots/dashboard.png) | ![Tools](docs/screenshots/tools.png) |

| Provider Management | Route Configuration |
|:---:|:---:|
| ![Providers](docs/screenshots/providers.png) | ![Routes](docs/screenshots/routes.png) |

| Gateway Details | Request Logs |
|:---:|:---:|
| ![Gateway](docs/screenshots/gateway.png) | ![Logs](docs/screenshots/logs.png) |

## Tech Stack

| Layer | Technology |
|---|---|
| Desktop Framework | Tauri v2 |
| Frontend | React 19 + TypeScript + Tailwind CSS v4 |
| Backend | Rust + Tokio + Axum |
| Database | SQLite (rusqlite, WAL mode) |
| HTTP Client | reqwest |

## Getting Started

### Download

Download the installer for your platform from the [Releases](../../releases) page.

| Platform | Format |
|---|---|
| macOS (Apple Silicon) | `.dmg` (aarch64) |
| macOS (Intel) | `.dmg` (x86_64) |
| Windows | `.msi` / `.exe` |
| Linux | `.AppImage` / `.deb` |

> **macOS users**: The app is not signed with an Apple Developer certificate. On first launch, macOS will block it. Go to **System Settings → Privacy & Security**, find AgentGate and click **Open Anyway**. Or run:
> ```bash
> xattr -d com.apple.quarantine /Applications/AgentGate.app
> ```

> **Windows users**: SmartScreen may show a warning on first run. Click **More info → Run anyway**.

### Build from Source

**Prerequisites**

- Node.js >= 20
- pnpm >= 10
- Rust >= 1.75
- macOS / Windows / Linux

**Install Dependencies**

```bash
pnpm install
```

**Development Mode**

```bash
pnpm tauri dev
```

**Build**

```bash
pnpm tauri build
```

## Usage Guide

### 1. Add a Provider

Launch AgentGate → **Providers** → **Add Provider**

Fill in:
- Name: e.g., `DeepSeek`
- Type: `deepseek`
- Base URL: `https://api.deepseek.com`
- API Key: your DeepSeek API key
- Default Model: `deepseek-v4-pro`

After saving, click **Fetch Models** to auto-load the available model list.

### 2. Start the Gateway

**Dashboard** or **Gateway** page → **Start Gateway**

Listens on `127.0.0.1:9090` by default.

### 3. Configure Codex

**Tools** → **Codex** → **Apply Config**

AgentGate will automatically write:
- `~/.codex/config.toml` — provider settings
- `~/.codex/auth.json` — AgentGate local token

Codex works immediately after configuration — no additional environment variables needed.

### 4. Configure Claude Code

**Tools** → **Claude Code** → **Apply Config**

AgentGate writes to `~/.claude/settings.json`, setting `ANTHROPIC_BASE_URL` to the local gateway and `ANTHROPIC_API_KEY` to the AgentGate local token.

Restart your terminal, and Claude Code will work through AgentGate.

### 5. Direct API Calls

All endpoints (except `/health`) require authentication:

```bash
TOKEN=$(cat ~/.agentgate/token)
```

**Chat Completions (Pass-through)**

```bash
curl -X POST http://127.0.0.1:9090/v1/chat/completions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-pro","messages":[{"role":"user","content":"Hello"}]}'
```

**Responses API (Codex Protocol)**

```bash
curl -X POST http://127.0.0.1:9090/v1/responses \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-5.5","input":"Hello","stream":true}'
```

**Messages API (Claude Code Protocol)**

```bash
curl -X POST http://127.0.0.1:9090/v1/messages \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4-6","max_tokens":1024,"messages":[{"role":"user","content":"Hello"}]}'
```

**Model List**

```bash
curl http://127.0.0.1:9090/v1/models -H "Authorization: Bearer $TOKEN"
```

**Health Check (No Auth Required)**

```bash
curl http://127.0.0.1:9090/health
```

### 6. Multi-Provider & Failover

Configure Route Profiles on the **Routes** page:

1. Add multiple providers to the provider chain
2. Adjust priority (lower number = higher priority)
3. Switch mode: manual / failover
4. In failover mode, 429/402/5xx/timeout errors automatically try the next provider

### 7. Diagnostics

On the **Diagnostics** page:

- **Run Self-Check** — checks gateway, provider, config, and database status
- **Export Diagnostic Bundle** — generates a redacted diagnostic report for troubleshooting

## Supported Providers

| Provider | Type | Protocol |
|---|---|---|
| DeepSeek | `deepseek` | OpenAI Chat Completions |
| OpenAI | `openai` | OpenAI Chat Completions |
| OpenRouter | `openrouter` | OpenAI Chat Completions |
| Kimi | `kimi` | OpenAI Chat Completions |
| Custom | `custom_openai_compatible` | OpenAI Chat Completions |

## Gateway Routes

| Method | Path | Mode | Description |
|---|---|---|---|
| GET | `/health` | internal | Health check (no auth) |
| GET | `/v1/models` | internal | Model list |
| POST | `/v1/responses` | transform | Responses → Chat Completions |
| POST | `/v1/chat/completions` | pass-through | Chat Completions direct |
| POST | `/v1/messages` | transform | Messages → Chat Completions |

## Project Structure

```
AgentGate/
├── src/                          # Frontend (React)
│   ├── app/App.tsx               # App entry point
│   ├── pages/                    # Pages (Dashboard/Tools/Providers/Routes/Gateway/Logs/Diagnostics/Settings)
│   ├── components/               # UI components
│   ├── lib/                      # API wrapper, i18n, utilities
│   └── types/                    # TypeScript type definitions
├── src-tauri/                    # Backend (Rust)
│   └── src/
│       ├── gateway/              # HTTP gateway (server/routes/SSE/pass-through/failover)
│       ├── protocol/             # Protocol types (Responses/ChatCompletions/Messages/SSE events)
│       ├── transform/            # Protocol conversion (responses→chat/schema cleanup/tool_calls/reasoning store)
│       ├── providers/            # Provider adapters
│       ├── storage/              # SQLite storage layer
│       ├── models/               # Data models
│       ├── tools/                # Tool config management (Codex/Claude Code)
│       ├── security/             # Authentication & redaction
│       ├── diagnostics/          # Diagnostics & self-checks
│       ├── app/                  # Tauri commands & app state
│       └── errors/               # Unified error types
├── scripts/                      # Test scripts
└── package.json
```

## Security

- Gateway uses local token authentication by default
- Provider API keys are stored only in local SQLite and never sent to clients
- Client tokens are never forwarded to upstream providers
- Logs and diagnostic bundles automatically redact sensitive information
- Gateway binds only to `127.0.0.1` by default; `0.0.0.0` is rejected
- Token file permissions set to `0600` (Unix)

## Development

### Test Scripts

```bash
# Health check
./scripts/test-gateway-health.sh

# Auth test
./scripts/check-gateway-auth.sh

# Responses API test
./scripts/test-responses-stream.sh

# Chat Completions test
./scripts/test-chat-completions-pass-through.sh
```

## License

MIT
