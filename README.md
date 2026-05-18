<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">
  <b>面向 Codex / Claude Code / Gemini CLI / OpenCode / AtomCode 的本地 AI 网关</b><br>
  协议转换 · 23+ Provider 预设 · 智能 Failover · Vision 感知路由 · 桌面应用
</p>

<p align="center">
  <a href="https://github.com/dengmengmian/AgentGate/releases"><img src="https://img.shields.io/github/v/release/dengmengmian/AgentGate?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/dengmengmian/AgentGate/stargazers"><img src="https://img.shields.io/github/stars/dengmengmian/AgentGate?style=flat-square" alt="Stars"></a>
  <a href="https://github.com/dengmengmian/AgentGate/releases"><img src="https://img.shields.io/github/downloads/dengmengmian/AgentGate/total?style=flat-square&color=green" alt="Downloads"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/github/license/dengmengmian/AgentGate?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="./README_EN.md">English</a> · <a href="https://github.com/dengmengmian/AgentGate/releases">下载安装</a> · <a href="#快速开始">快速开始</a>
</p>

---

AgentGate 是面向 AI 编程 Agent 的**本地模型网关**。一个入口连接 Codex、Claude Code、Gemini CLI、OpenCode、AtomCode 五种客户端，支持 DeepSeek、OpenAI、Anthropic、Kimi、GLM、通义千问等 23+ Provider，自动协议转换、智能 failover 和 Vision 感知路由。

**和手动改配置文件有什么区别？** AgentGate 是桌面应用，图形界面一键切换 Provider，不用碰命令行。支持多 Provider 优先级链——A 挂了自动切 B，含图片的请求自动跳过不支持视觉的 Provider。所有请求有日志、有统计、可诊断。

## 核心功能

**协议转换与统一入口**

- OpenAI Responses API (`/v1/responses`) → Chat Completions 转换 / Claude Messages 原生转换 / Responses 直通透传，支持 Codex
- Anthropic Messages API (`/v1/messages`) → Chat Completions 转换 / Anthropic 端点直通透传，支持 Claude Code
- Chat Completions (`/v1/chat/completions`) 直通转发
- Anthropic Claude API 原生支持：`tool_use`/`tool_result`、`input_schema`、`thinking.budget_tokens`、SSE 事件流转换
- DeepSeek reasoning_content 思考模式完整支持（不降智）
- 工具调用（function_call）流式拼接与多轮对话

**多模态支持与 Vision 感知路由**

- 图片内容在协议转换中完整保留，支持 `input_image`/`image_url` → Chat Completions `image_url` 和 Anthropic `image source` 格式转换
- Provider 保存或测试连接时自动探测 Vision 能力（发送 1x1 像素探测请求）
- 智能转移模式下，包含图片的请求会自动跳过不支持 Vision 的 Provider
- 不支持图片的 Provider（如 DeepSeek）在 Provider 特定层自动剥离图片，避免 400 错误

**多 Provider 管理**

- 支持 DeepSeek、OpenAI、Anthropic、OpenRouter、Kimi、MiniMax、自定义 OpenAI 兼容接口
- Route Profile 配置多 Provider 优先级链，按协议自动匹配
- 手动切换或智能转移（failover）
- Provider cooldown 和运行时状态追踪
- 请求级 failover：A 失败 → 自动 try B
- Vision 感知路由：含图片请求自动跳过不支持视觉的 Provider
- 新增 Provider 自动加入所有路由链
- Provider 模型列表自动获取

**客户端配置管理**

- Codex 一键配置 + 官方/AgentGate 一键切换（保留对话记录）
- Claude Code 一键配置 + 官方/AgentGate 一键切换
- OpenCode 一键配置写入
- Gateway 本地访问令牌（ag_local_\*）认证

**桌面应用体验**

- 系统托盘常驻，关闭窗口后台运行
- 托盘菜单控制 Gateway 启停
- 开机自启支持
- 请求日志、诊断自检、诊断包导出
- 中英双语界面

## 截图

|                  概览                   |              客户端配置               |
| :---------------------------------------: | :---------------------------------: |
| ![概览](docs/screenshots/dashboard.png) | ![客户端](docs/screenshots/tools.png) |

|                服务商管理                 |               路由配置               |
| :---------------------------------------: | :----------------------------------: |
| ![服务商](docs/screenshots/providers.png) | ![路由](docs/screenshots/routes.png) |

|               网关详情                |              请求日志              |
| :-----------------------------------: | :--------------------------------: |
| ![网关](docs/screenshots/gateway.png) | ![日志](docs/screenshots/logs.png) |

## 技术栈

| 层          | 技术                                    |
| ----------- | --------------------------------------- |
| 桌面框架    | Tauri v2                                |
| 前端        | React 19 + TypeScript + Tailwind CSS v4 |
| 后端        | Rust + Tokio + Axum                     |
| 数据库      | SQLite (rusqlite, WAL 模式)             |
| HTTP 客户端 | reqwest                                 |

## 快速开始

### 下载安装

从 [Releases](../../releases) 页面下载对应平台的安装包。

| 平台 | 格式 |
|---|---|
| macOS (Apple Silicon) | `.dmg` (aarch64) |
| macOS (Intel) | `.dmg` (x86_64) |
| Windows | `.msi` / `.exe` |
| Linux | `.AppImage` / `.deb` |

> **macOS 用户注意**：由于未签名 Apple 开发者证书，首次打开会提示"无法验证开发者"。请前往 **系统设置 → 隐私与安全性**，找到 AgentGate 点击 **仍要打开**。或在终端执行：
> ```bash
> xattr -d com.apple.quarantine /Applications/AgentGate.app
> ```

> **Windows 用户注意**：首次运行可能弹出 SmartScreen 警告，点击 **更多信息 → 仍要运行** 即可。

### 从源码构建

**环境要求**

- Node.js >= 20
- pnpm >= 10
- Rust >= 1.75
- macOS / Windows / Linux

**安装依赖**

```bash
pnpm install
```

**开发模式**

```bash
pnpm tauri dev
```

**构建**

```bash
pnpm tauri build
```

## 使用指南

### 1. 添加 Provider

启动 AgentGate → **服务商** → **添加服务商**

**基础配置字段：**

| 字段 | 说明 | 示例 |
|---|---|---|
| 名称 | Provider 显示名称 | `DeepSeek` |
| 类型 | Provider 类型，影响请求转换行为 | `deepseek` |
| 协议 | 上游 API 协议格式 | `OpenAI Chat Completions` |
| Base URL | Provider API 地址 | `https://api.deepseek.com` |
| API Key | Provider 的 API 密钥 | `sk-...` |
| 默认模型 | 未匹配时使用的模型 | `deepseek-v4-flash` |
| 推理模型 | 用于推理/思考的模型（可选） | `deepseek-v4-pro` |
| 超时 | 请求超时时间（秒） | `120` |

**高级配置字段：**

| 字段 | 说明 | 示例 |
|---|---|---|
| 模型映射 | 将客户端模型名映射到 Provider 实际模型 | `gpt-5.5` → `deepseek-v4-flash` |
| Anthropic 兼容端点 | Claude Code 直通转发地址（可选） | `https://api.deepseek.com/anthropic` |
| Responses API 端点 | Codex Responses API 直通透传地址（可选），填了走透传，不填走协议转换 | `https://api.openai.com` |
| 额外请求头 | 自定义 HTTP 请求头（JSON） | `{"User-Agent":"KimiCLI/1.40.0"}` |

**各 Provider 配置示例：**

<details>
<summary>DeepSeek</summary>

| 字段 | 值 |
|---|---|
| 名称 | `DeepSeek` |
| 类型 | `deepseek` |
| Base URL | `https://api.deepseek.com` |
| 默认模型 | `deepseek-v4-flash` |
| 推理模型 | `deepseek-v4-pro` |
| 模型映射 | `gpt-5.5` → `deepseek-v4-flash`，`o3` → `deepseek-v4-pro` |
| Anthropic 端点 | `https://api.deepseek.com/anthropic`（支持 Claude Code 直通） |

</details>

<details>
<summary>KimiCoding（Moonshot）</summary>

| 字段 | 值 |
|---|---|
| 名称 | `KimiCoding` |
| 类型 | `kimi` |
| Base URL | `https://api.moonshot.cn` |
| 默认模型 | `kimi-k2` |
| 额外请求头 | `{"User-Agent":"KimiCLI/1.40.0"}` |
| 模型映射 | `gpt-5.5` → `kimi-k2` |

> KimiCoding 支持 Vision，可作为图片请求的智能转移目标。

</details>

<details>
<summary>OpenAI</summary>

| 字段 | 值 |
|---|---|
| 名称 | `OpenAI` |
| 类型 | `openai` |
| Base URL | `https://api.openai.com` |
| 默认模型 | `gpt-4o` |
| Responses API 端点 | `https://api.openai.com`（OpenAI 原生支持，走透传） |
| 模型映射 | 通常无需映射（客户端模型名直接使用） |

</details>

<details>
<summary>Anthropic（Claude）</summary>

| 字段 | 值 |
|---|---|
| 名称 | `Anthropic` |
| 类型 | `anthropic` |
| Base URL | `https://api.anthropic.com` |
| 默认模型 | `claude-sonnet-4-6` |
| 模型映射 | `gpt-5.5` → `claude-sonnet-4-6` |

> 类型选 `Anthropic (Claude)` 后，Codex 请求会自动走 Claude Messages API 原生转换（`tool_use`/`tool_result`/`input_schema`），而非转成 Chat Completions。

</details>

<details>
<summary>MiniMax</summary>

| 字段 | 值 |
|---|---|
| 名称 | `MiniMax` |
| 类型 | `minimax` |
| Base URL | `https://api.minimax.chat` |
| 默认模型 | `MiniMax-M1` |
| 模型映射 | `gpt-5.5` → `MiniMax-M1` |

</details>

<details>
<summary>OpenRouter</summary>

| 字段 | 值 |
|---|---|
| 名称 | `OpenRouter` |
| 类型 | `openrouter` |
| Base URL | `https://openrouter.ai/api` |
| 默认模型 | `deepseek/deepseek-v4-flash` |
| 模型映射 | `gpt-5.5` → `deepseek/deepseek-v4-flash` |

</details>

<details>
<summary>自定义 OpenAI 兼容接口</summary>

| 字段 | 值 |
|---|---|
| 名称 | 自定义名称 |
| 类型 | `custom_openai_compatible` |
| Base URL | 你的服务地址，如 `http://localhost:8000` |
| 默认模型 | 你的模型名称 |

> 适用于任何兼容 OpenAI Chat Completions API 的服务（如 vLLM、Ollama、LiteLLM 等）。

</details>

**保存后：**

- 点击 **获取模型** 自动加载可用模型列表
- 点击 **测试连接** 验证配置是否正确，同时自动探测 Vision 能力

### 2. 启动 Gateway

**概览** 或 **网关** 页面 → **启动网关**

默认监听 `127.0.0.1:9090`。

### 3. 配置 Codex

**客户端** → **Codex** → **应用配置**

AgentGate 会自动：

- 保存原始 `~/.codex/config.toml` 和 `auth.json`
- 写入 AgentGate 的 Provider 配置和本地令牌

点击 **切换到官方** 可随时恢复原始配置，对话记录不丢失。

### 4. 配置 Claude Code

**客户端** → **Claude Code** → **应用配置**

AgentGate 会写入 `~/.claude/settings.json`，将 `ANTHROPIC_BASE_URL` 指向本地 Gateway，`ANTHROPIC_API_KEY` 设为 AgentGate 本地令牌。

点击 **切换到官方** 可恢复原始 settings.json。

### 5. 配置 OpenCode

**客户端** → **OpenCode** → **应用配置**

AgentGate 会写入 `~/.config/opencode/opencode.json`，配置 OpenAI 兼容 Provider 指向本地 Gateway。

### 6. 直接调用 API

所有接口（除 `/health`）需要携带本地访问令牌。

**获取令牌：**

- **界面复制**：AgentGate → **设置** → **Gateway 认证** → 点击令牌旁的复制按钮
- **终端读取**：
  ```bash
  TOKEN=$(cat ~/.agentgate/token)
  ```
- **重新生成**：**设置** → **重新生成令牌**（旧令牌立即失效）

令牌格式为 `ag_local_*`，仅用于本地 Gateway 认证，不会转发给上游 Provider。

**Chat Completions（直通转发）**

```bash
curl -X POST http://127.0.0.1:9090/v1/chat/completions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","messages":[{"role":"user","content":"你好"}]}'
```

**Responses API（Codex 协议）**

```bash
curl -X POST http://127.0.0.1:9090/v1/responses \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-5.5","input":"你好","stream":true}'
```

**Messages API（Claude Code 协议）**

```bash
curl -X POST http://127.0.0.1:9090/v1/messages \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4-6","max_tokens":1024,"messages":[{"role":"user","content":"你好"}]}'
```

**模型列表**

```bash
curl http://127.0.0.1:9090/v1/models -H "Authorization: Bearer $TOKEN"
```

**健康检查（无需认证）**

```bash
curl http://127.0.0.1:9090/health
```

### 7. 多 Provider 与智能转移

**路由** 页面可以配置 Route Profile：

1. 每种协议自动创建默认路由（Codex / Claude Code / OpenCode）
2. 添加多个 Provider 到 Provider Chain，调整优先级
3. 切换模式：手动 / 智能转移
4. 智能转移模式下，429/402/5xx/超时 错误会自动尝试下一个 Provider

### 8. Vision 感知路由（多模态图片支持）

AgentGate 会在 Provider 保存或测试连接时自动探测其 Vision（图片识别）能力，并在路由决策中使用该信息。

**配置步骤：**

1. 添加多个 Provider（如 DeepSeek + KimiCoding），确保至少一个支持图片
2. **服务商** 页面点击 **测试连接**，AgentGate 会自动发送探测请求检测 Vision 能力
3. 测试完成后，Provider 卡片上会显示 **支持视觉** 或 **不支持视觉** 标记
4. **路由** 页面将模式切换为 **智能转移**

**工作原理：**

- 新建或更新 Provider 时会自动触发 Vision 探测（也可手动点击"测试连接"）
- 探测方式：向 Provider 发送一个带 1x1 像素图片的请求（`max_tokens: 1`），几乎不消耗 token
- 请求包含图片时，智能转移会自动跳过 `supports_vision = false` 的 Provider
- 未探测的 Provider（`supports_vision = null`）不会被跳过，保证向后兼容
- 不支持图片的 Provider（如 DeepSeek）会在转换层自动剥离图片内容，不影响纯文本请求

**示例场景：**

```
Codex 发送含图片的请求
  → AgentGate 检测到请求包含图片
  → 跳过 DeepSeek（supports_vision = false）
  → 直接路由到 KimiCoding（supports_vision = true）
  → KimiCoding 收到完整的图片 + 文本请求
```

### 9. 诊断

**诊断** 页面：

- **运行自检** — 检查 Gateway、Provider、配置、数据库状态
- **导出诊断包** — 生成脱敏诊断信息用于排查问题

## 支持的 Provider

| Provider   | 类型                       | 转换方式                | 专属处理 | Vision |
| ---------- | -------------------------- | ----------------------- | -------- | ------ |
| DeepSeek   | `deepseek`                 | Chat Completions 转换   | 图片剥离、reasoning 注入、schema 清洗、消息重排 | ✗ |
| OpenAI     | `openai`                   | 透传或 Chat Completions | 无 | ✓ |
| Anthropic  | `anthropic`                | Claude Messages 原生转换 | `tool_use`/`tool_result`、`input_schema`、thinking budget | ✓ |
| OpenRouter | `openrouter`               | Chat Completions 转换   | 无 | 取决于模型 |
| KimiCoding | `kimi`                     | Chat Completions 转换   | web_search → builtin_function、thinking 控制 | ✓ |
| MiniMax    | `minimax`                  | Chat Completions 转换   | 去 reasoning_effort/response_format、`<think>` 提取 | ✓ |
| 自定义     | `custom_openai_compatible` | Chat Completions 转换   | 无 | 自动探测 |

## 数据链路

AgentGate 有两种工作模式：**协议转换**和**透明代理**。

> **如何区分？** 看客户端协议和下游 Provider 是否一致。不一致就需要协议转换，一致就走透明代理。

| 客户端 | 发送协议 | 下游 Provider | AgentGate 模式 | 触发条件 |
|---|---|---|---|---|
| Codex | Responses API | Chat Completions | 协议转换 | 默认（无特殊 URL） |
| Codex | Responses API | Claude Messages API | 协议转换 | `provider_type` 为 `anthropic` |
| Codex | Responses API | Responses API | 透明代理 | 配置了 `responses_base_url` |
| Claude Code | Messages API | Chat Completions | 协议转换 | 无 `anthropic_base_url` |
| Claude Code | Messages API | Anthropic 兼容端点 | 透明代理 | 配置了 `anthropic_base_url` |
| OpenCode | Chat Completions | Chat Completions | 透明代理 | 同协议直通 |
| curl / New API 等 | Chat Completions | Chat Completions | 透明代理 | 同协议直通 |

### 协议转换

客户端协议和下游 Provider 不一致时，AgentGate 进行格式转换。这是最复杂的路径，包含 Vision 感知路由、Provider 特定处理等。

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
│  ① 认证：验证本地令牌 (ag_local_*)                                      │
│                         ▼                                               │
│  ② 路由匹配：按协议匹配 Route Profile                                  │
│     /v1/responses → Codex Default                                       │
│     /v1/messages  → Claude Code Default                                 │
│                         ▼                                               │
│  ③ 协议转换（公共层）                                                   │
│     Responses API → Chat Completions（图片 input_image → image_url）    │
│     Messages API  → Chat Completions（图片 image → image_url）          │
│                         ▼                                               │
│  ④ Vision 感知路由（智能转移模式）                                      │
│     有图片 → 跳过 supports_vision=false 的 Provider                     │
│     无图片 → 按优先级正常选择                                           │
│                         ▼                                               │
│  ⑤ Provider 特定转换                                                    │
│     DeepSeek   → 剥离图片 + reasoning_content + schema 清理             │
│     KimiCoding → web_search 转换 + thinking 控制                        │
│     Anthropic  → 转为 Claude Messages 格式（图片→source.base64）        │
│     其他       → 直接发送                                               │
│                         ▼                                               │
│  ⑥ 智能转移：429/402/5xx/超时 → cooldown → 尝试下一个 Provider         │
│                         ▼                                               │
│  ⑦ 日志记录 → SQLite                                                   │
│                         ▼                                               │
│  ⑧ 响应反转换：Chat Completions 响应 → 原始协议格式返回给客户端        │
└─────────┬───────────────────────────────┬───────────────────────────────┘
          │                               │
          ▼                               ▼
   ┌──────────────┐               ┌──────────────┐
   │   DeepSeek   │               │  KimiCoding  │  ...
   │   (纯文本)   │               │ (文本+图片)  │
   └──────────────┘               └──────────────┘
```

### 透明代理

客户端协议和下游 Provider 一致时，AgentGate 不做格式转换，只替换地址、凭证和模型名。请求体和响应流完整透传。

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
│  ① 认证：验证本地令牌 (ag_local_*)                                      │
│                         ▼                                               │
│  ② 路由匹配：按协议匹配 Route Profile                                  │
│     /v1/messages          → Claude Code Default                         │
│     /v1/chat/completions  → OpenCode Default                            │
│                         ▼                                               │
│  ③ 透明代理                                                             │
│     替换目标 URL（base_url 或 anthropic_base_url）                      │
│     注入 Provider API Key                                               │
│     映射 model 名称（如 gpt-5.5 → deepseek-v4-flash）                  │
│     请求体原样转发 ──→ 响应流原样回传                                   │
│                         ▼                                               │
│  ④ 日志记录 → SQLite                                                   │
└─────────┬───────────────┬───────────────────┬───────────────────────────┘
          │               │                   │
          ▼               ▼                   ▼
  ┌──────────────┐ ┌──────────────┐   ┌──────────────┐
  │  DeepSeek    │ │   OpenAI     │   │  New API     │  ...
  │  /anthropic  │ │              │   │  / 聚合平台  │
  └──────────────┘ └──────────────┘   └──────────────┘

触发条件：
  • /v1/messages    + Provider 配置了 anthropic_base_url → Messages API 透明代理
  • /v1/chat/completions → Chat Completions 透明代理（所有 Provider 都支持）
```

### 链路示例

**Codex 含图片请求（协议转换 + Vision 感知路由）：**

```
Codex 发送 input_image
  → /v1/responses (Responses API)
  → ① 认证通过
  → ② 匹配 Codex Default Route Profile
  → ③ 协议转换：input_image → image_url（图片保留）
  → ④ Vision 路由：检测到图片 → 跳过 DeepSeek (No Vision) → 选择 KimiCoding (Vision)
  → ⑤ KimiCoding 转换层：无需剥离图片，直接发送
  → ⑥ KimiCoding 返回成功 → 标记健康
  → ⑦ 记录日志
  → ⑧ 转回 Responses API 格式 → 返回给 Codex
```

**Claude Code → DeepSeek（透明代理）：**

```
Claude Code 发送 Messages API 请求
  → /v1/messages
  → ① 认证通过
  → ② 匹配 Claude Code Default Route Profile
  → ③ DeepSeek 配有 anthropic_base_url → 透明代理
  → 替换 URL 为 api.deepseek.com/anthropic + 注入 API Key
  → 请求体原样透传 → SSE 响应流原样回传
  → ④ 记录日志
```

**OpenCode / curl / New API 客户端（透明代理）：**

```
客户端发送 Chat Completions 请求
  → /v1/chat/completions
  → ① 认证通过
  → ② 匹配 Route Profile
  → ③ 透明代理：替换 URL + API Key + model 映射
  → 请求体原样转发 → SSE 响应流原样回传
  → ④ 记录日志
```

## Gateway 路由

| 方法 | 路径                   | 模式         | 说明                         |
| ---- | ---------------------- | ------------ | ---------------------------- |
| GET  | `/health`              | internal     | 健康检查（无需认证）         |
| GET  | `/v1/models`           | internal     | 模型列表                     |
| POST | `/v1/responses`        | 自动         | 有 `responses_base_url` → 透传；Anthropic 类型 → Claude 转换；其他 → Chat Completions 转换 |
| POST | `/v1/chat/completions` | pass-through | Chat Completions 直通        |
| POST | `/v1/messages`         | 自动         | 有 `anthropic_base_url` → 透传；否则 → Chat Completions 转换 |

## 项目结构

```
AgentGate/
├── src/                          # 前端 React
│   ├── app/App.tsx               # 应用入口
│   ├── pages/                    # 页面（概览/服务商/路由/网关/客户端/日志/诊断/设置）
│   ├── components/               # UI 组件
│   ├── lib/                      # API 封装、i18n、工具函数
│   └── types/                    # TypeScript 类型定义
├── src-tauri/                    # 后端 Rust
│   └── src/
│       ├── gateway/              # HTTP 网关（server/routes/SSE/SSE Anthropic/pass-through/failover）
│       ├── protocol/             # 协议类型（Responses/ChatCompletions/Messages/SSE events）
│       ├── transform/            # 协议转换（responses→chat/responses→anthropic/schema清理/tool_calls/reasoning存储/providers）
│       ├── providers/            # Provider 适配器
│       ├── storage/              # SQLite 存储层
│       ├── models/               # 数据模型
│       ├── tools/                # 客户端配置管理（Codex/Claude Code/OpenCode）
│       ├── security/             # 认证与脱敏
│       ├── diagnostics/          # 诊断与自检
│       ├── app/                  # Tauri commands 与应用状态
│       └── errors/               # 统一错误类型
├── scripts/                      # 测试脚本
└── package.json
```

## 安全

- Gateway 默认启用本地令牌认证
- Provider API Key 仅存储在本地 SQLite，不会发送给客户端
- 客户端传入的令牌不会转发给上游 Provider
- 日志和诊断包自动脱敏敏感信息
- Gateway 默认仅监听 `127.0.0.1`，拒绝绑定 `0.0.0.0`
- 令牌文件权限设置为 `0600`（Unix）

## 开发

### 测试脚本

```bash
# 健康检查
./scripts/test-gateway-health.sh

# 认证测试
./scripts/check-gateway-auth.sh

# Responses API 测试
./scripts/test-responses-stream.sh

# Chat Completions 测试
./scripts/test-chat-completions-pass-through.sh
```

## License

MIT
