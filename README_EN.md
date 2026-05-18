<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">
  <b>Local AI Gateway for Codex, Claude Code, Gemini CLI, OpenCode & AtomCode</b><br>
  Protocol conversion · 23+ provider presets · Smart failover · Vision-aware routing · Desktop app
</p>

<p align="center">
  <a href="https://github.com/dengmengmian/AgentGate/releases"><img src="https://img.shields.io/github/v/release/dengmengmian/AgentGate?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/dengmengmian/AgentGate/stargazers"><img src="https://img.shields.io/github/stars/dengmengmian/AgentGate?style=flat-square" alt="Stars"></a>
  <a href="https://github.com/dengmengmian/AgentGate/releases"><img src="https://img.shields.io/github/downloads/dengmengmian/AgentGate/total?style=flat-square&color=green" alt="Downloads"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/github/license/dengmengmian/AgentGate?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="./README.md">中文</a> · <a href="https://github.com/dengmengmian/AgentGate/releases">Download</a> · <a href="#getting-started">Getting Started</a>
</p>

---

AgentGate is a **local model gateway** for AI coding agents. One entry point connects Codex, Claude Code, Gemini CLI, OpenCode, and AtomCode to 23+ providers including DeepSeek, OpenAI, Anthropic, Kimi, GLM, and DashScope — with automatic protocol conversion, smart failover, and vision-aware routing.

**Why not just edit config files?** AgentGate is a desktop app with a GUI — switch providers in one click, no command line needed. It supports multi-provider priority chains (A fails → auto-switch to B), image-aware routing (skip non-vision providers), request logging, token stats, and diagnostics.

## Features

**Protocol Conversion & Unified Entry Point**
- OpenAI Responses API (`/v1/responses`) → Chat Completions conversion / Claude Messages native conversion / Responses pass-through, for Codex
- Anthropic Messages API (`/v1/messages`) → Chat Completions conversion / Anthropic endpoint pass-through, for Claude Code
- Chat Completions (`/v1/chat/completions`) pass-through forwarding
- Native Anthropic Claude API support: `tool_use`/`tool_result`, `input_schema`, `thinking.budget_tokens`, SSE event stream conversion
- Full DeepSeek reasoning_content (thinking mode) support without degradation
- Tool call (function_call) streaming reassembly and multi-turn conversations

**Multimodal Support & Vision-Aware Routing**
- Image content is fully preserved during protocol conversion, supporting `input_image`/`image_url` → Chat Completions `image_url` and Anthropic `image source` format conversion
- Vision capability is auto-detected when a provider is saved or tested (sends a 1x1 pixel probe request)
- In failover mode, requests containing images automatically skip providers that don't support vision
- Providers that don't support images (e.g., DeepSeek) have images stripped at the provider-specific layer, avoiding 400 errors

**Multi-Provider Management**
- Supports DeepSeek, OpenAI, Anthropic, OpenRouter, Kimi, MiniMax, and custom OpenAI-compatible endpoints
- Route Profiles with multi-provider priority chains, auto-matched by protocol
- Manual switching or automatic failover
- Provider cooldown and runtime status tracking
- Per-request failover: Provider A fails → automatically tries Provider B
- Vision-aware routing: requests with images auto-skip non-vision providers
- New providers are automatically added to all route chains
- Automatic model list fetching from providers

**Client Configuration**
- Codex: one-click config + toggle between official and AgentGate (preserves conversations)
- Claude Code: one-click config + toggle between official and AgentGate
- OpenCode: one-click config
- Local gateway access token (`ag_local_*`) authentication

**Desktop Application**
- System tray with background operation on window close
- Tray menu for gateway start/stop control
- Auto-start on system boot
- Request logging, self-diagnostics, and diagnostic bundle export
- Bilingual UI (Chinese and English)

## Screenshots

| Overview | Client Configuration |
|:---:|:---:|
| ![Overview](docs/screenshots/dashboard.png) | ![Clients](docs/screenshots/tools.png) |

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

<details>
<summary><b>macOS: "Cannot verify the developer"?</b> (click to expand)</summary>

The app is ad-hoc signed (won't show "damaged"), but macOS Gatekeeper blocks unnotarized apps. Three ways to fix (pick one):

**Option 1: System Settings (recommended)**
1. Double-click AgentGate, click **Cancel** on the prompt
2. Open **System Settings → Privacy & Security**
3. Scroll down, find `"AgentGate" was blocked` → click **Open Anyway**
4. Open AgentGate again, click **Open**

**Option 2: Right-click open**
1. Find AgentGate.app in Finder
2. Hold **Control** and click (or right-click) → select **Open**
3. Click **Open** on the prompt

**Option 3: Terminal**
```bash
xattr -d com.apple.quarantine /Applications/AgentGate.app
```

> Only needed once.

</details>

<details>
<summary><b>Windows SmartScreen warning?</b> (click to expand)</summary>

On first run, SmartScreen may show a warning:
1. Click **More info**
2. Click **Run anyway**

> Only needed once.

</details>

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

**Basic fields:**

| Field | Description | Example |
|---|---|---|
| Name | Display name for the provider | `DeepSeek` |
| Type | Provider type, affects request transformation | `deepseek` |
| Protocol | Upstream API protocol format | `OpenAI Chat Completions` |
| Base URL | Provider API endpoint | `https://api.deepseek.com` |
| API Key | Provider API key | `sk-...` |
| Default Model | Model used when no match is found | `deepseek-v4-flash` |
| Reasoning Model | Model for reasoning/thinking (optional) | `deepseek-v4-pro` |
| Timeout | Request timeout in seconds | `120` |

**Advanced fields:**

| Field | Description | Example |
|---|---|---|
| Model Mapping | Maps client model names to provider models | `gpt-5.5` → `deepseek-v4-flash` |
| Anthropic Endpoint | Claude Code pass-through URL (optional) | `https://api.deepseek.com/anthropic` |
| Responses API Endpoint | Codex Responses API pass-through URL (optional). If set, requests are proxied directly; if empty, protocol conversion is used | `https://api.openai.com` |
| Extra Headers | Custom HTTP headers (JSON) | `{"User-Agent":"KimiCLI/1.40.0"}` |

**Provider configuration examples:**

<details>
<summary>DeepSeek</summary>

| Field | Value |
|---|---|
| Name | `DeepSeek` |
| Type | `deepseek` |
| Base URL | `https://api.deepseek.com` |
| Default Model | `deepseek-v4-flash` |
| Reasoning Model | `deepseek-v4-pro` |
| Model Mapping | `gpt-5.5` → `deepseek-v4-flash`, `o3` → `deepseek-v4-pro` |
| Anthropic Endpoint | `https://api.deepseek.com/anthropic` (supports Claude Code pass-through) |

</details>

<details>
<summary>KimiCoding (Moonshot)</summary>

| Field | Value |
|---|---|
| Name | `KimiCoding` |
| Type | `kimi` |
| Base URL | `https://api.moonshot.cn` |
| Default Model | `kimi-k2` |
| Extra Headers | `{"User-Agent":"KimiCLI/1.40.0"}` |
| Model Mapping | `gpt-5.5` → `kimi-k2` |

> KimiCoding supports Vision and can serve as a failover target for image requests.

</details>

<details>
<summary>OpenAI</summary>

| Field | Value |
|---|---|
| Name | `OpenAI` |
| Type | `openai` |
| Base URL | `https://api.openai.com` |
| Default Model | `gpt-4o` |
| Responses API Endpoint | `https://api.openai.com` (OpenAI natively supports Responses API, uses pass-through) |
| Model Mapping | Usually not needed (client model names used directly) |

</details>

<details>
<summary>Anthropic (Claude)</summary>

| Field | Value |
|---|---|
| Name | `Anthropic` |
| Type | `anthropic` |
| Base URL | `https://api.anthropic.com` |
| Default Model | `claude-sonnet-4-6` |
| Model Mapping | `gpt-5.5` → `claude-sonnet-4-6` |

> When type is set to `Anthropic (Claude)`, Codex requests are automatically converted using Claude Messages API native format (`tool_use`/`tool_result`/`input_schema`), rather than being converted to Chat Completions.

</details>

<details>
<summary>MiniMax</summary>

| Field | Value |
|---|---|
| Name | `MiniMax` |
| Type | `minimax` |
| Base URL | `https://api.minimax.chat` |
| Default Model | `MiniMax-M1` |
| Model Mapping | `gpt-5.5` → `MiniMax-M1` |

</details>

<details>
<summary>OpenRouter</summary>

| Field | Value |
|---|---|
| Name | `OpenRouter` |
| Type | `openrouter` |
| Base URL | `https://openrouter.ai/api` |
| Default Model | `deepseek/deepseek-v4-flash` |
| Model Mapping | `gpt-5.5` → `deepseek/deepseek-v4-flash` |

</details>

<details>
<summary>Custom OpenAI Compatible</summary>

| Field | Value |
|---|---|
| Name | Your custom name |
| Type | `custom_openai_compatible` |
| Base URL | Your server URL, e.g., `http://localhost:8000` |
| Default Model | Your model name |

> Works with any OpenAI Chat Completions API-compatible service (e.g., vLLM, Ollama, LiteLLM).

</details>

**After saving:**

- Click **Fetch Models** to auto-load the available model list
- Click **Test Connection** to verify the config and auto-detect Vision capability

### 2. Start the Gateway

**Overview** or **Gateway** page → **Start Gateway**

Listens on `127.0.0.1:9090` by default.

### 3. Configure Codex

**Clients** → **Codex** → **Apply Config**

AgentGate will automatically:

- Save original `~/.codex/config.toml` and `auth.json`
- Write AgentGate provider settings and local token

Click **Switch to Official** to restore the original config at any time — conversations are preserved.

### 4. Configure Claude Code

**Clients** → **Claude Code** → **Apply Config**

AgentGate writes to `~/.claude/settings.json`, setting `ANTHROPIC_BASE_URL` to the local gateway and `ANTHROPIC_API_KEY` to the AgentGate local token.

Click **Switch to Official** to restore the original settings.json.

### 5. Configure OpenCode

**Clients** → **OpenCode** → **Apply Config**

AgentGate writes to `~/.config/opencode/opencode.json`, configuring an OpenAI-compatible provider pointing to the local gateway.

### 6. Direct API Calls

All endpoints (except `/health`) require a local access token.

**Getting the token:**

- **Copy from UI**: AgentGate → **Settings** → **Gateway Auth** → click the copy button next to the token
- **Read from terminal**:
  ```bash
  TOKEN=$(cat ~/.agentgate/token)
  ```
- **Regenerate**: **Settings** → **Regenerate Token** (old token is immediately invalidated)

The token format is `ag_local_*`. It is only used for local gateway auth and is never forwarded to upstream providers.

**Chat Completions (Pass-through)**

```bash
curl -X POST http://127.0.0.1:9090/v1/chat/completions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","messages":[{"role":"user","content":"Hello"}]}'
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

### 7. Multi-Provider & Failover

Configure Route Profiles on the **Routes** page:

1. Default routes are auto-created per protocol (Codex / Claude Code / OpenCode)
2. Add multiple providers to the provider chain, adjust priorities
3. Switch mode: manual / failover
4. In failover mode, 429/402/5xx/timeout errors automatically try the next provider

### 8. Vision-Aware Routing (Multimodal Image Support)

AgentGate auto-detects each provider's vision (image recognition) capability when a provider is saved or tested, and uses this information for routing decisions.

**Setup:**

1. Add multiple providers (e.g., DeepSeek + KimiCoding), ensuring at least one supports images
2. On the **Providers** page, click **Test Connection** — AgentGate sends a probe request to detect vision capability
3. After detection, provider cards display a **Vision** or **No Vision** badge
4. On the **Routes** page, switch mode to **failover**

**How it works:**

- Creating or updating a provider automatically triggers vision detection (can also be triggered manually via "Test Connection")
- Detection method: sends a request with a 1x1 pixel image (`max_tokens: 1`) — virtually zero token cost
- When a request contains images, failover automatically skips providers with `supports_vision = false`
- Undetected providers (`supports_vision = null`) are not skipped, ensuring backward compatibility
- Providers that don't support images (e.g., DeepSeek) strip image content at the provider transform layer, with no impact on text-only requests

**Example scenario:**

```
Codex sends a request with images
  → AgentGate detects the request contains images
  → Skips DeepSeek (supports_vision = false)
  → Routes directly to KimiCoding (supports_vision = true)
  → KimiCoding receives the full image + text request
```

### 9. Diagnostics

On the **Diagnostics** page:

- **Run Self-Check** — checks gateway, provider, config, and database status
- **Export Diagnostic Bundle** — generates a redacted diagnostic report for troubleshooting

## Supported Providers

| Provider | Type | Conversion | Provider-Specific Handling | Vision |
|---|---|---|---|---|
| DeepSeek | `deepseek` | Chat Completions | Image stripping, reasoning injection, schema cleaning, message reordering | ✗ |
| OpenAI | `openai` | Pass-through or Chat Completions | None | ✓ |
| Anthropic | `anthropic` | Claude Messages native | `tool_use`/`tool_result`, `input_schema`, thinking budget | ✓ |
| OpenRouter | `openrouter` | Chat Completions | None | Model-dependent |
| KimiCoding | `kimi` | Chat Completions | web_search → builtin_function, thinking control | ✓ |
| MiniMax | `minimax` | Chat Completions | Strip reasoning_effort/response_format, `<think>` extraction | ✓ |
| Custom | `custom_openai_compatible` | Chat Completions | None | Auto-detected |

## Data Flow

AgentGate operates in two modes: **protocol conversion** and **transparent proxy**.

> **How to tell?** If the client protocol matches the downstream provider protocol, it's a transparent proxy. Otherwise, protocol conversion is needed.

| Client | Sends | Downstream Provider | AgentGate Mode | Trigger |
|---|---|---|---|---|
| Codex | Responses API | Chat Completions | Protocol Conversion | Default (no special URL) |
| Codex | Responses API | Claude Messages API | Protocol Conversion | `provider_type` is `anthropic` |
| Codex | Responses API | Responses API | Transparent Proxy | `responses_base_url` is configured |
| Claude Code | Messages API | Chat Completions | Protocol Conversion | No `anthropic_base_url` |
| Claude Code | Messages API | Anthropic-compatible endpoint | Transparent Proxy | `anthropic_base_url` is configured |
| OpenCode | Chat Completions | Chat Completions | Transparent Proxy | Same protocol |
| curl / New API etc. | Chat Completions | Chat Completions | Transparent Proxy | Same protocol |

### Protocol Conversion

When the client protocol differs from the downstream provider, AgentGate converts the format. This is the most complex path, including vision-aware routing and provider-specific processing.

```
┌──────────────────┐    ┌──────────────────┐
│      Codex       │    │    Claude Code    │
│  (Responses API) │    │  (Messages API)   │
└────────┬─────────┘    └────────┬─────────┘
         │                       │
         ▼                       ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    AgentGate (127.0.0.1:9090)                           │
│                                                                         │
│  ① Auth: validate local token (ag_local_*)                              │
│                         ▼                                               │
│  ② Route Matching: match Route Profile by protocol                      │
│     /v1/responses → Codex Default                                       │
│     /v1/messages  → Claude Code Default                                 │
│                         ▼                                               │
│  ③ Protocol Conversion (shared layer)                                   │
│     Responses API → Chat Completions (input_image → image_url)          │
│     Messages API  → Chat Completions (image → image_url)                │
│                         ▼                                               │
│  ④ Vision-Aware Routing (failover mode)                                 │
│     Has images → skip providers with supports_vision=false              │
│     No images  → select by priority as normal                           │
│                         ▼                                               │
│  ⑤ Provider-Specific Transform                                          │
│     DeepSeek   → strip images + reasoning_content + schema fix          │
│     KimiCoding → web_search conversion + thinking control               │
│     Anthropic  → convert to Claude Messages (image→source.base64)       │
│     Others     → send directly                                          │
│                         ▼                                               │
│  ⑥ Failover: 429/402/5xx/timeout → cooldown → try next provider        │
│                         ▼                                               │
│  ⑦ Logging → SQLite                                                    │
│                         ▼                                               │
│  ⑧ Response reverse-conversion: back to original protocol for client    │
└─────────┬───────────────────────────────┬───────────────────────────────┘
          │                               │
          ▼                               ▼
   ┌──────────────┐               ┌──────────────┐
   │   DeepSeek   │               │  KimiCoding  │  ...
   │  (text only) │               │ (text+image) │
   └──────────────┘               └──────────────┘
```

### Transparent Proxy

When the client protocol matches the downstream provider, AgentGate does not convert the format. It only replaces the URL, credentials, and model name. Request body and response stream are fully proxied.

```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│    Claude Code   │  │     OpenCode     │  │  curl / New API  │
│  (Messages API)  │  │(Chat Completions)│  │(Chat Completions)│
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │                     │                      │
         ▼                     ▼                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    AgentGate (127.0.0.1:9090)                           │
│                                                                         │
│  ① Auth: validate local token (ag_local_*)                              │
│                         ▼                                               │
│  ② Route Matching: match Route Profile by protocol                      │
│     /v1/messages          → Claude Code Default                         │
│     /v1/chat/completions  → OpenCode Default                            │
│                         ▼                                               │
│  ③ Transparent Proxy                                                    │
│     Replace target URL (base_url or anthropic_base_url)                 │
│     Inject provider API key                                             │
│     Map model name (e.g. gpt-5.5 → deepseek-v4-flash)                  │
│     Forward request body as-is ──→ Proxy response stream as-is         │
│                         ▼                                               │
│  ④ Logging → SQLite                                                    │
└─────────┬───────────────┬───────────────────┬───────────────────────────┘
          │               │                   │
          ▼               ▼                   ▼
  ┌──────────────┐ ┌──────────────┐   ┌──────────────┐
  │  DeepSeek    │ │   OpenAI     │   │  New API     │  ...
  │  /anthropic  │ │              │   │  / aggregator│
  └──────────────┘ └──────────────┘   └──────────────┘

Trigger conditions:
  • /v1/messages    + Provider has anthropic_base_url → Messages API transparent proxy
  • /v1/chat/completions → Chat Completions transparent proxy (all providers)
```

### Flow Examples

**Codex with images (protocol conversion + vision-aware routing):**

```
Codex sends input_image
  → /v1/responses (Responses API)
  → ① Auth passes
  → ② Matches Codex Default Route Profile
  → ③ Protocol conversion: input_image → image_url (image preserved)
  → ④ Vision routing: image detected → skip DeepSeek (No Vision) → select KimiCoding (Vision)
  → ⑤ KimiCoding transform: no image stripping, send directly
  → ⑥ KimiCoding returns success → mark healthy
  → ⑦ Log request
  → ⑧ Reverse-convert to Responses API format → return to Codex
```

**Claude Code → DeepSeek (transparent proxy):**

```
Claude Code sends Messages API request
  → /v1/messages
  → ① Auth passes
  → ② Matches Claude Code Default Route Profile
  → ③ DeepSeek has anthropic_base_url → transparent proxy
  → Replace URL with api.deepseek.com/anthropic + inject API key
  → Request body proxied as-is → SSE response stream proxied as-is
  → ④ Log request
```

**OpenCode / curl / New API (transparent proxy):**

```
Client sends Chat Completions request
  → /v1/chat/completions
  → ① Auth passes
  → ② Matches Route Profile
  → ③ Transparent proxy: replace URL + API key + model mapping
  → Request body forwarded as-is → SSE response stream proxied as-is
  → ④ Log request
```

## Gateway Routes

| Method | Path | Mode | Description |
|---|---|---|---|
| GET | `/health` | internal | Health check (no auth) |
| GET | `/v1/models` | internal | Model list |
| POST | `/v1/responses` | auto | `responses_base_url` set → pass-through; Anthropic type → Claude conversion; otherwise → Chat Completions conversion |
| POST | `/v1/chat/completions` | pass-through | Chat Completions direct |
| POST | `/v1/messages` | auto | `anthropic_base_url` set → pass-through; otherwise → Chat Completions conversion |

## Project Structure

```
AgentGate/
├── src/                          # Frontend (React)
│   ├── app/App.tsx               # App entry point
│   ├── pages/                    # Pages (Overview/Providers/Routes/Gateway/Clients/Logs/Diagnostics/Settings)
│   ├── components/               # UI components
│   ├── lib/                      # API wrapper, i18n, utilities
│   └── types/                    # TypeScript type definitions
├── src-tauri/                    # Backend (Rust)
│   └── src/
│       ├── gateway/              # HTTP gateway (server/routes/SSE/SSE Anthropic/pass-through/failover)
│       ├── protocol/             # Protocol types (Responses/ChatCompletions/Messages/SSE events)
│       ├── transform/            # Protocol conversion (responses→chat/responses→anthropic/schema cleanup/tool_calls/reasoning store/providers)
│       ├── providers/            # Provider adapters
│       ├── storage/              # SQLite storage layer
│       ├── models/               # Data models
│       ├── tools/                # Client config management (Codex/Claude Code/OpenCode)
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
