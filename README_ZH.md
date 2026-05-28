<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">
  <b>面向 Codex / Claude Code / Gemini CLI / OpenCode / AtomCode 的本地 AI 网关</b><br>
  协议转换 · 24 个 Provider 预设 · 智能 Failover · 能力感知路由 · 桌面应用
</p>

<p align="center">
  <a href="https://github.com/dengmengmian/AgentGate/releases"><img src="https://img.shields.io/github/v/release/dengmengmian/AgentGate?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/dengmengmian/AgentGate/stargazers"><img src="https://img.shields.io/github/stars/dengmengmian/AgentGate?style=flat-square" alt="Stars"></a>
  <a href="https://github.com/dengmengmian/AgentGate/releases"><img src="https://img.shields.io/github/downloads/dengmengmian/AgentGate/total?style=flat-square&color=green" alt="Downloads"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/github/license/dengmengmian/AgentGate?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="https://github.com/dengmengmian/AgentGate/releases">下载安装</a> · <a href="#快速开始">快速开始</a>
</p>

---

AgentGate 是面向 AI 编程 Agent 的**本地模型网关**。一个入口连接 Codex、Claude Code、Gemini CLI、OpenCode、AtomCode 五种客户端，内置 24 个 Provider 预设：小米 MiMo、DeepSeek、OpenAI、Anthropic、Kimi、GLM、通义千问、硅基流动、火山引擎等。自动协议转换、智能 failover、按"每个模型"的能力感知路由。

**和手动改配置文件有什么区别？** AgentGate 是桌面应用，图形界面一键切换 Provider，不用碰命令行。支持多 Provider 优先级链——A 挂了自动切 B，含图片的请求自动同 provider 内 swap 到能识图的 model（或跨链 fallback）。所有请求有日志、有统计、可诊断。

## 核心功能

**协议转换与统一入口**

- OpenAI Responses API (`/v1/responses`) → Chat Completions 转换 / Claude Messages 原生转换 / Responses 直通透传，支持 Codex
- Anthropic Messages API (`/v1/messages`) → Chat Completions 转换 / Anthropic 端点直通透传，支持 Claude Code
- Chat Completions (`/v1/chat/completions`) 直通转发
- Anthropic Claude API 原生支持：`tool_use`/`tool_result`、`input_schema`、`thinking.budget_tokens`、SSE 事件流转换
- DeepSeek reasoning_content 思考模式完整支持（不降智）
- 工具调用（function_call）流式拼接与多轮对话

**多模态支持与模型能力矩阵**

- 图片内容在协议转换中完整保留，支持 `input_image`/`image_url` → Chat Completions `image_url` 和 Anthropic `image source` 格式转换
- 模型能力矩阵：每个 model 独立追踪 8 个能力（`text` / `vision` / `audio_in` / `tts` / `video_in` / `reasoning` / `tools` / `web_search`）
- 能力感知 promotion：请求带图时网关自动 swap 到同 provider 内支持 vision 的 model（如 `mimo-v2.5-pro` → `mimo-v2.5`），优先选保留最多原模型能力的候选
- 矩阵也驱动 builtin 工具发送：取消勾选某 model 的 `web_search` 后，网关停止给该 model 发送 web_search builtin，避免上游 400
- 矩阵自动种子：内置 MiMo / DeepSeek / Kimi / Moonshot 识别规则 + 通用 fallback；测试按钮合并连通性检测和矩阵 autofill（保留手动编辑）
- 智能转移模式下，请求按矩阵选可用 model；该 provider 无任何 model 支持时再回退到下一个 provider
- 不支持图片的 model 自动在 provider 特定层剥离图片，避免上游 400/404

**多 Provider 管理**

- **24 个内置 Provider 预设**（选 type 后 base URL / 协议 / Anthropic 端点 / 默认模型自动填好）：
  - **国内**：小米 MiMo、DeepSeek、Kimi/Moonshot、MiniMax、智谱 GLM、通义千问 DashScope、硅基流动 SiliconFlow、火山引擎（豆包）、百川、阶跃星辰 StepFun、零一万物 Yi
  - **海外**：OpenAI、Anthropic（Claude）、Google Gemini、xAI（Grok）、Mistral、Groq、Together、Fireworks、Cerebras、Perplexity、Cohere
  - **聚合器**：OpenRouter
  - **自定义**：任意 OpenAI 兼容接口（vLLM / Ollama / LiteLLM / 本地代理）
- MiMo 一等公民：5 个聊天模型（`mimo-v2.5-pro` / `mimo-v2-pro` / `mimo-v2.5` / `mimo-v2-omni` / `mimo-v2-flash`）、多轮 `reasoning_content` 回环、`tp-*` key 自动切 Token Plan host、`webSearchEnabled` 友好错误提示
- `[1m]` 长上下文后缀自动注入：Claude Code 透传路径上对 MiMo 和 DeepSeek 的 1M-capable 模型透明追加（用户配置 `mimo-v2.5-pro`，Codex 端正常 128K、CC 端拿到 1M）
- Route Profile 配置多 Provider 优先级链，按协议自动匹配
- 手动切换或智能转移（failover）
- Provider cooldown 和运行时状态追踪
- 请求级 failover：A 失败 → 自动 try B
- 能力感知路由：含图/音频/etc. 请求按矩阵在 provider 内自动 swap model，provider 不可用时跨链 fallback
- 新增 Provider 自动加入所有路由链
- Provider 模型列表自动获取
- 连接稳定性：HTTP client 调优 `pool_idle_timeout` + `tcp_keepalive`，应用层兼容 transient connect/timeout 重试（避免静默后死链接导致失败）

**客户端配置管理**

- Codex 一键配置 + 官方/AgentGate 一键切换（保留对话记录）
- Claude Code 一键配置 + 官方/AgentGate 一键切换
- OpenCode 一键配置写入
- Gateway 本地访问令牌（ag_local_\*）认证

**桌面宠物**

- 9 个原创 SVG 宠物角色：网关机器人、像素猫、史莱姆、CEO、章鱼、麻凡、奎奎、分总、振振
- 宠物根据网关状态做出反应：空闲（轻微浮动）、活跃（请求时弹跳）、错误（摇晃）、睡眠（5 分钟空闲后）
- 气泡通知：网关启停、请求错误自动弹出提示
- AI 聊天：双击宠物打开聊天，使用已配置的 Provider 回复
- 持久记忆：跨会话记住用户名
- 统计气泡：每 30 分钟显示「今日: 128 请求 | $0.42」
- 空闲时宠物身体微微跟随鼠标方向
- 位置保存，重启自动恢复
- 在设置 > 宠物 Tab 管理，或通过系统托盘切换显隐

**快速配置 & 诊断**

- 首次引导：粘贴 API Key → 自动识别 Provider → 选择工具 → 一键完成配置
- 快速添加 Provider：粘贴 API Key 自动识别类型（sk-ant-、deepseek-、gsk\_ 等前缀）
- 连接测试：客户端页面三步状态条（配置 → 网关 → 供应商）
- 侧边栏快速配置页面（有 Provider 后自动隐藏，可在设置中重新开启）

**桌面应用体验**

- 深色主题（暖琥珀色调）和浅色主题（干净中性灰白）
- 设置 > 通用 中切换主题
- 系统托盘常驻，关闭窗口后台运行
- 托盘菜单控制 Gateway 启停 + 宠物显隐
- 开机自启支持
- 请求日志、诊断自检、诊断包导出
- 中英双语界面
- 应用内自动更新

## 截图

| 概览 | 供应商 |
|:---:|:---:|
| ![概览](docs/screenshots/dashboard.png) | ![供应商](docs/screenshots/providers.png) |

| 路由 | 网关 |
|:---:|:---:|
| ![路由](docs/screenshots/routes.png) | ![网关](docs/screenshots/gateway.png) |

| 客户端 | 日志 |
|:---:|:---:|
| ![客户端](docs/screenshots/tools.png) | ![日志](docs/screenshots/logs.png) |

| 诊断 | 设置 |
|:---:|:---:|
| ![诊断](docs/screenshots/diagnostics.png) | ![设置](docs/screenshots/settings.png) |

| 快速配置 | 宠物设置 |
|:---:|:---:|
| ![快速配置](docs/screenshots/quick-setup.png) | ![宠物设置](docs/screenshots/pet-settings.png) |

| 桌面宠物 |
|:---:|
| ![宠物](docs/screenshots/pet.png) |

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

<details>
<summary><b>macOS 首次打开提示"无法验证开发者"？</b>（点击展开）</summary>

应用已做 ad-hoc 签名，不会提示"已损坏"，但 macOS Gatekeeper 会拦截未公证的应用。三种解决方式（任选其一）：

**方式一：系统设置（推荐）**
1. 双击打开 AgentGate，弹出提示后点击 **取消**
2. 打开 **系统设置 → 隐私与安全性**
3. 向下滚动，找到 `"AgentGate" 已被阻止` → 点击 **仍要打开**
4. 再次打开 AgentGate，点击 **打开** 即可

**方式二：右键打开**
1. 在 Finder 中找到 AgentGate.app
2. 按住 **Control** 键点击（或右键）→ 选择 **打开**
3. 弹出提示后点击 **打开**

**方式三：终端命令**
```bash
xattr -d com.apple.quarantine /Applications/AgentGate.app
```

> 只需操作一次，后续正常打开即可。

</details>

<details>
<summary><b>Windows SmartScreen 警告？</b>（点击展开）</summary>

首次运行可能弹出 SmartScreen 警告：
1. 点击 **更多信息**
2. 点击 **仍要运行**

> 只需操作一次。

</details>

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

## Headless / 服务器模式

无 GUI 运行 AgentGate——适用于服务器、CI、Docker 和团队部署。

```bash
# 添加 Provider
agentgate-serve provider-add -t deepseek -k sk-xxx

# 启动网关
agentgate-serve serve --host 0.0.0.0 --port 9090

# 其他命令
agentgate-serve provider-list          # 列出所有 Provider
agentgate-serve provider-remove NAME   # 删除 Provider
agentgate-serve token                  # 查看访问令牌
agentgate-serve status                 # 查看配置状态
```

**Docker 部署：**

```bash
docker compose up
```

## 使用指南

### 1. 添加 Provider

启动 AgentGate → **供应商** → **添加供应商**

**快速通道（推荐）——粘 API Key 即可：**

1. 顶部输入框粘任意 Provider 的 API key
2. AgentGate 按前缀（`sk-ant-` / `sk-` / `deepseek-` / `gsk_` / …）自动识别 Provider 类型
3. 点 **创建** —— name / base URL / 协议 / default 模型 / reasoning 模型 / 能力矩阵全部自动填好。完事。

**手动模式 —— 三段式，只 Section A 必填：**

| Section | 字段 | 说明 |
|---|---|---|
| **基础** | 类型 · 名称 · API Key（custom 类型才显式露 Base URL） | 选 type 后剩下全部按 preset 自动填 |
| **模型与能力** | 默认模型 · 推理模型 · `拉取并识别能力` 按钮 · 能力矩阵折叠开关 | 创建 Provider 后**后台自动跑**：拉模型列表 → 按名字 seed 矩阵 → 挑最新非 mini 作 default、最新推理系作 reasoning，**不用手点** |
| **高级** *（默认折叠，"通常无需修改"）* | 协议+对应 URL 合并视图（Chat / Responses / Anthropic 各自一行）· 额外请求头 · 超时 · 自动 cache 控制 · 模型映射 | 协议每勾一个，下面就显示对应 URL——一眼看清"这个上游同时支持哪些原生入口" |

**模型映射** 放在高级最底部是有原因的：**通常无需配置**。未识别的客户端模型名会自动 fallback 到 default model。只有当你想让同一 Provider 对不同请求模型名分流到不同上游模型时才用。

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

- 模型拉取 + 能力识别**后台自动跑**，无需手动操作
- 点击 **测试连接** 验证配置——会弹出单个 dialog 显示 3 步实时进度：**连接 + 鉴权** → **能力矩阵 autofill** → **缓存支持检测**（非 Anthropic Provider 自动跳过）

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

### 6. 配置 Gemini CLI

**客户端** → **Gemini CLI** → **应用配置**

AgentGate 把 Gemini CLI 配置指向本地网关的 `/v1beta/...` 路由（Gemini 兼容入口）。一键切换回官方。

### 7. 配置 AtomCode

**客户端** → **AtomCode** → **应用配置**

AtomCode 把上游配置指向 AgentGate，切换模式与其他客户端一致。

### 8. 直接调用 API

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

### 9. 多 Provider 与智能转移

**路由** 页面可以配置 Route Profile：

1. 每种协议自动创建默认路由（Codex / Claude Code / OpenCode）
2. 添加多个 Provider 到 Provider Chain，调整优先级
3. 切换模式：手动 / 智能转移
4. 智能转移模式下，429/402/5xx/超时 错误会自动尝试下一个 Provider

### 10. 能力感知路由（多模态 & 推理）

AgentGate 按"每个 model"追踪 8 维能力，请求带图/带音频/带工具时网关会自动挑能跑的 model。

**模型能力矩阵：**

`text` · `vision` · `audio_in` · `tts` · `video_in` · `reasoning` · `tools` · `web_search`

**配置步骤：**

1. 添加 Provider——创建后**后台自动**拉取模型列表、按模型名识别能力、按 heuristic 挑出最新非 mini 模型作 default、最新推理系作 reasoning_model
2. **供应商** 页卡片上能看到能力图标 + 直连协议 chip（`直连 Chat` / `直连 Anthropic` / …）
3. 想重新探测：编辑 Provider → 点 **拉取并识别能力**
4. **路由** 页将模式切换为 **智能转移**

**工作原理：**

- 创建 Provider 后：网关调上游 `/models`、按模型名 pattern 种能力矩阵、挑最新 default + reasoning 模型
- 请求带图时：
  - 先尝试在**同一 provider 内** swap 到支持 vision 的兄弟模型（如 `mimo-v2.5-pro` → `mimo-v2.5`）
  - 同 provider 没有 vision 模型时，跨链 fallback 到下一个 provider
- 能力矩阵也驱动 builtin 工具发送：取消某 model 的 `web_search` 后，网关停止给该 model 发送 `web_search` builtin（避免上游 400）
- promotion 优先选保留原模型最多其他能力的候选；`supported_models` 顺序作 tiebreak
- 整个 provider 没有任何 vision 模型时，在 provider 转换层剥离图片，纯文本走兜底（避免上游 400/404）

**示例场景：**

```
Codex 发送含图片的请求
  → 网关检测到请求含图
  → MiMo 矩阵: mimo-v2.5-pro = text, mimo-v2.5 = text + vision
  → 同 provider 内 promotion: mimo-v2.5-pro → mimo-v2.5
  → 请求带完整图片 + 文本送出
```

### 11. 诊断

**诊断** 页面：

- **运行自检** — 检查 Gateway、Provider、配置、数据库状态
- **导出诊断包** — 生成脱敏诊断信息用于排查问题

## 支持的 Provider

标了 **专属处理** 的 Provider 在 `src-tauri/src/transform/providers/` 下有专门转换代码。其余走通用 Chat Completions / Anthropic 透传路径，开箱即用。

| Provider | 类型 | 原生协议 | 专属处理 |
|---|---|---|---|
| 小米 MiMo | `mimo` | Chat + Anthropic | 多轮 `reasoning_content` 回环、`tp-*` host 自动切换、思考态剥 temperature、tool_choice 非 auto 剥除、omni web_search 剥除、CC 路径 `[1m]` 后缀注入、web_search builtin 按矩阵翻译、`webSearchEnabled` 友好提示 |
| DeepSeek | `deepseek` | Chat + Anthropic | 图片剥离（纯文本模型）、reasoning 注入、schema 清洗、消息重排、CC 路径 v4-pro `[1m]` 后缀 |
| Anthropic（Claude） | `anthropic` | Anthropic Messages（原生） | `tool_use`/`tool_result`、`input_schema`、thinking budget、原生 cache_control |
| OpenAI | `openai` | Chat + Responses | 无（Responses 透传或 Chat 转换） |
| Google Gemini | `google_gemini` | Chat（兼容端点） | 无 |
| Kimi / Moonshot | `kimi` | Chat | `web_search` → `builtin_function`/`$web_search`、thinking 控制 |
| MiniMax | `minimax` | Chat | 去 reasoning_effort / response_format、`<think>` 提取 |
| 智谱 GLM | `glm` | Chat + Anthropic | 通用 |
| 通义千问 DashScope | `dashscope` | Chat + Anthropic | 通用 |
| 硅基流动 SiliconFlow | `siliconflow` | Chat | 通用 |
| 火山引擎（豆包） | `volcengine` | Chat | 通用 |
| 百川 | `baichuan` | Chat | 通用 |
| 阶跃星辰 StepFun | `stepfun` | Chat | 通用 |
| 零一万物 Yi | `yi` | Chat | 通用 |
| xAI（Grok） | `xai` | Chat | 通用 |
| Mistral | `mistral` | Chat | 通用 |
| Groq | `groq` | Chat | 通用 |
| Together | `together` | Chat | 通用 |
| Fireworks | `fireworks` | Chat | 通用 |
| Cerebras | `cerebras` | Chat | 通用 |
| Perplexity | `perplexity` | Chat | 通用 |
| Cohere | `cohere` | Chat（兼容端点） | 通用 |
| OpenRouter | `openrouter` | Chat | 无 |
| 自定义 | `custom_openai_compatible` | Chat | 无（Base URL 用户自己填） |

> Vision / reasoning / tools / web_search 等能力是**按每个 model**追踪的，不在 provider 层。详见上文 *能力感知路由*。

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

客户端协议和下游 Provider 不一致时，AgentGate 进行格式转换。这是最复杂的路径，包含能力感知路由、Provider 特定处理等。

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
│  ④ 能力感知路由（智能转移模式）                                         │
│     有图片 → 同 provider 内 promotion 到 vision model 或跨链 fallback   │
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

**Codex 含图片请求（协议转换 + 能力感知路由）：**

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
│   ├── pages/                    # 页面（概览/快速配置/供应商/路由/网关/客户端/日志/诊断/设置）
│   ├── components/               # UI 组件（布局/通用/仪表盘/供应商/日志/工具/引导）
│   ├── pet/                      # 桌面宠物系统（PetApp/气泡/问候语/9 个宠物 SVG 组件）
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
