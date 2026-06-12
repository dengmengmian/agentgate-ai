<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">
  <b>让 Codex、Claude Code、Gemini CLI 跑在更便宜的模型上 —— 还不掉链子。</b><br>
  自动故障切换 · 成本统计 · 一键配置。一个本地网关，接入 26 个 Provider。
</p>

<p align="center">
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/v/release/dengmengmian/agentgate-ai?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/stargazers"><img src="https://img.shields.io/github/stars/dengmengmian/agentgate-ai?style=flat-square" alt="Stars"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/downloads/dengmengmian/agentgate-ai/total?style=flat-square&color=green" alt="Downloads"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/github/license/dengmengmian/agentgate-ai?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="https://github.com/dengmengmian/agentgate-ai/releases">下载安装</a> · <a href="#5-分钟跑通">5 分钟跑通</a> · <a href="./docs/use-codex-with-deepseek.md">Codex + DeepSeek</a> · <a href="./docs/use-claude-code-with-deepseek.md">Claude Code + DeepSeek</a> · <a href="./docs/use-gemini-cli-with-agentgate.md">Gemini CLI</a>
</p>

## 按系统下载

| 你的机器 | 下载 |
|---|---|
| 🍎 macOS — Apple 芯片（M1–M4） | [AgentGate_1.4.1_aarch64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_aarch64.dmg) |
| 🍎 macOS — Intel 芯片 | [AgentGate_1.4.1_x64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_x64.dmg) |
| 🪟 Windows 10 / 11 | [AgentGate_1.4.1_x64-setup.exe](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_x64-setup.exe) |
| 🐧 Linux — Debian / Ubuntu | [AgentGate_1.4.1_amd64.deb](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_amd64.deb) |
| 🐧 Linux — 其他发行版 | [AgentGate_1.4.1_amd64.AppImage](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_amd64.AppImage) |

> 无界面 CLI（`agentgate-serve`）压缩包和历史版本：[Releases](https://github.com/dengmengmian/agentgate-ai/releases)

---

AgentGate 是面向 AI 编程 Agent 的**本地模型网关**。它把 Codex、Claude Code、Gemini CLI、OpenCode、AtomCode 统一接到一个本地入口，再转发到小米 MiMo、DeepSeek、OpenAI、Anthropic、GitHub Copilot、Kimi、GLM、通义千问、硅基流动、火山引擎等 26 个 Provider。

> **让你的编程 Agent 跑在更便宜的模型上，省了多少实时看得见。** 把 Codex / Claude Code / Gemini CLI 接到 DeepSeek、MiMo、GLM、Kimi，成本仪表盘按模型、客户端、路由策略清楚算出每一笔花销。

![成本仪表盘](docs/screenshots/dashboard.png)

AgentGate 不止是代理，而是一个建立在三大支柱上的**智能网关**：

| 🧠 智能路由 | 🔁 故障自愈 | 💰 成本仪表盘 |
|:---|:---|:---|
| 每个请求选对模型——按能力（视觉 / 工具 / 推理）、按价格、或按延迟 | 自动故障转移、熔断、冷却，某家限流或挂了请求照常跑 | 按请求记成本，可按模型、客户端、路由策略和时间区间拆解 |

它解决的不是"再包一层代理"这么简单，而是这些实际问题：

- Codex 发的是 Responses API，但很多模型只支持 Chat Completions 或 Anthropic Messages。
- Codex 桌面端的插件和账号能力依赖官方 OpenAI 登录态与 provider 识别路径，很多第三方 API 代理会破坏这层语义。
- Claude Code 想直连 DeepSeek / MiMo 的 Anthropic 兼容接口，但配置、模型名和端点容易混。
- 同一个 provider 里不同 model 能力不同，图片、tools、web_search 发错模型就会 400。
- 多个 provider / 多个 key 要能自动切换、失败重试、记录日志和成本。
- 不想每次切模型都手动改 `~/.codex/config.toml`、`~/.claude/settings.json`。

AgentGate 的定位是：**本地统一入口 + 协议转换 + 原生直连 + 智能路由 + 图形化配置**——再加 **⚡ 零手改配置**：客户端一键应用 / 还原，不用再抠 `config.toml` / `settings.json`。

## 和同类怎么选

LLM 代理有很多好工具。AgentGate 的定位是**桌面端的编程 Agent**——它是唯一专注于伺候 Codex / Claude Code / Gemini CLI、用图形界面而非服务器来跑的那个。

| 工具 | 它最擅长 | AgentGate 的差异 |
|---|---|---|
| **普通代理** | 改一个 base URL | 保住 Codex / ChatGPT 登录态和插件，做协议转换，按能力路由 |
| **claude-code-router** | 把 Claude Code（CLI）路由到别的模型 | 还覆盖 Codex Responses API、Gemini CLI、OpenCode，并带图形界面和成本仪表盘 |
| **one-api / new-api** | 服务器上做多用户 API 分发与计费 | 本地优先、单用户、无账号体系；内置客户端一键配置 |
| **LiteLLM** | 给自己应用用的 Python SDK / 代理，接 100+ 模型 | 是面向编程 Agent 的桌面网关，不是库——零代码、图形化 |

> 定位是大致划分，各工具也在快速演进——按自己工作流选。要运营共享 API 服务，one-api / LiteLLM 更合适；要天天泡在 Codex / Claude Code 里，这个就是为你做的。

## 常见使用场景

教程：[Codex 桌面端插件兼容](./docs/use-codex-desktop-with-third-party-api-and-plugins.md) · [Codex + DeepSeek](./docs/use-codex-with-deepseek.md) · [Codex + 小米 MiMo](./docs/use-codex-with-mimo.md) · [Claude Code + DeepSeek](./docs/use-claude-code-with-deepseek.md) · [Claude Code + GitHub Copilot](./docs/use-claude-code-with-github-copilot.md) · [Gemini CLI](./docs/use-gemini-cli-with-agentgate.md) · [OpenCode](./docs/use-opencode-with-agentgate.md)

| 目标 | AgentGate 做什么 |
|---|---|
| 让 Codex 使用 DeepSeek | 把 Codex 的 OpenAI Responses API 请求转换到 DeepSeek 兼容的 Chat Completions 或 Anthropic 兼容端点。 |
| 让 Codex 使用小米 MiMo | 通过本地网关把 Codex 路由到 MiMo 模型，自动处理模型映射、reasoning 和能力矩阵。 |
| 用 GitHub Copilot 订阅跑 Claude Code | GitHub token 自动换 Copilot 凭证，工具续写 / 压缩请求标记为 agent 续写，不重复消耗 premium 额度。见 [专门章节](#用-github-copilot-订阅跑-claude-code--codex)。 |
| Codex 接小窗口模型跑超长会话 | 历史超过模型上下文窗口时网关自动摘要压缩中段历史（保留 system 与最近轮原文），128K 窗口也能撑住 300K+ token 的长会话。 |
| 让 Codex 桌面端用第三方 API 且保留插件能力 | 让 Codex 桌面端继续走官方 OpenAI 登录态和 provider 识别路径，插件和账号相关能力可继续工作，对话模型请求则路由到 AgentGate。 |
| 让 Claude Code 使用 DeepSeek / MiMo | 通过 Anthropic 兼容直通和模型映射连接 DeepSeek / MiMo。 |
| 在多个 Provider 间切换 Codex | 一个本地入口切换 DeepSeek、MiMo、OpenAI、Kimi、GLM、通义千问等，无需手改配置文件。 |

## 5 分钟跑通

1. 从 [Releases](../../releases) 下载并安装 AgentGate。
2. 打开应用，进入 **快速配置** 或 **供应商**，粘贴你的 Provider API Key。
3. AgentGate 会自动识别已知 key 前缀并填好 base URL、协议、默认模型和能力矩阵；无法唯一识别时手动选择 Provider 类型即可。
4. 在 **概览** 或 **网关** 页面点击 **启动网关**，默认监听 `127.0.0.1:9090`。注意：`1420` 是开发界面端口，不是客户端 API 网关。
5. 进入 **客户端** 页面，对 Codex / Claude Code / OpenCode / Gemini CLI / AtomCode 点击 **应用配置**。
6. 回到对应客户端发送一句话测试。需要恢复官方配置时，点 **切换到官方**。

新手通常只需要做这 6 步。模型映射、协议端点、能力矩阵都可以先不碰；AgentGate 会按 Provider 预设和测试结果自动补齐。

<a id="用-github-copilot-订阅跑-claude-code--codex"></a>

## 可选玩法：用 GitHub Copilot 订阅跑 Claude Code / Codex

有 Copilot 订阅（Pro / Business）就能让 Claude Code 跑在订阅包含的 Claude 模型上，**不用单独买 Anthropic API**。AgentGate 处理三件事：

1. **凭证自动交换**：你只需提供一个 GitHub OAuth token（`gho_` 开头），网关自动换取并续期 Copilot API 凭证，token 不落明文。
2. **premium 额度优化**：Agent 工作流里大量请求是工具结果续写和历史压缩——AgentGate 自动把这类请求标记为 `x-initiator: agent`，**只有你真正发出的消息消耗 premium 请求额度**，一条带 10 次工具往返的指令只计 1 次。
3. **模型名归一化**：Claude Code 发的 `claude-sonnet-4-6` 自动转成 Copilot 端点接受的 `claude-sonnet-4.6`，不用配映射。

**步骤：**

1. 拿 GitHub token：已登录 VS Code Copilot 的话直接读 `~/.config/github-copilot/apps.json` 里的 `oauth_token`。
2. AgentGate → 供应商 → 新建，类型选 **GitHub Copilot**，API Key 粘贴 `gho_` token（base URL、模型列表自动填好）。
3. 客户端页给 Claude Code 应用配置，开聊。日志页能看到每个请求的 `x-initiator` 分类。

> ⚠️ **风险声明**：在官方客户端之外使用 Copilot 订阅属于 GitHub 服务条款的灰色地带。同类社区工具长期存在、目前未见大规模处罚，但**理论上存在账号风险，请自行评估**——建议避免在重要的企业账号上使用。该功能完全可选，不添加 copilot 类型的 Provider 就不会涉及。

## 三种工作模式

| 模式 | 什么时候发生 | 模型名怎么处理 | 典型场景 |
|---|---|---|---|
| **协议转换** | 客户端协议和上游协议不同 | 命中 Model Mapping 就改写；否则用 provider 默认模型兜底 | Codex Responses → DeepSeek / MiMo Chat |
| **原生直连** | 客户端协议和上游原生入口一致 | 未命中 Model Mapping 时保持请求里的 `model` 原样；虚拟模型 `agentgate` 会解析成本次路由选中的真实模型 | OpenCode / curl → Chat Completions |
| **直连 + 模型映射** | 协议一致，但客户端模型名不是上游模型名 | 按 Model Mapping 改写 | Claude Code `claude-*` → DeepSeek / MiMo model |

一句话判断：**先看协议是否一致；协议一致就透明转发，协议不一致才转换。模型映射只是改名规则，不等于一定转换。** 一键配置的客户端会使用 `agentgate` 这个虚拟模型名，含义是"让 AgentGate 按当前路由选择真实 provider model"。

## 核心功能

**协议转换与统一入口**

- OpenAI Responses API (`/v1/responses`) → Chat Completions 转换 / Claude Messages 原生转换 / Responses 直通透传，支持 Codex
- Anthropic Messages API (`/v1/messages`) → Chat Completions 转换 / Anthropic 端点直通透传，支持 Claude Code
- Chat Completions (`/v1/chat/completions`) 直通转发
- Anthropic Claude API 原生支持：`tool_use`/`tool_result`、`input_schema`、`thinking.budget_tokens`、SSE 事件流转换
- DeepSeek reasoning_content 思考模式完整支持（不降智）
- 工具调用（function_call）流式拼接与多轮对话
- **长历史自压缩**：历史超过模型上下文窗口时（按 catalog 内置窗口 ×85% 自适应，可按模型覆盖），网关自动摘要中段历史、保留 system 和最近轮原文——Codex 接 128K 小窗口模型也能跑超长会话，不再撞 400
- **思考质量优先**：转换到 Claude 系模型时支持 thinking 就开（新模型 adaptive / 旧模型 budget 自动选型），并守住 budget/采样参数/强制工具调用三类 Anthropic 约束防 400
- **Prompt cache 自动注入**：Anthropic 方向（转换与直通都覆盖）自动在 tools / system / 历史断点注入 `cache_control`，预算感知不超 4 断点上限——长对话省的钱直接体现在成本仪表盘的缓存命中里

**多模态支持与模型能力矩阵**

- 图片内容在协议转换中完整保留，支持 `input_image`/`image_url` → Chat Completions `image_url` 和 Anthropic `image source` 格式转换
- 模型能力矩阵：每个 model 独立追踪 8 个能力（`text` / `vision` / `audio_in` / `tts` / `video_in` / `reasoning` / `tools` / `web_search`）
- 能力感知 promotion：请求带图时网关自动 swap 到同 provider 内支持 vision 的 model（如 `mimo-v2.5-pro` → `mimo-v2.5`），优先选保留最多原模型能力的候选
- 矩阵也驱动 builtin 工具发送：取消勾选某 model 的 `web_search` 后，网关停止给该 model 发送 web_search builtin，避免上游 400
- 矩阵自动种子：内置 MiMo / DeepSeek / Kimi / Moonshot 识别规则 + 通用 fallback；测试按钮合并连通性检测和矩阵 autofill（保留手动编辑）
- 智能转移模式下，请求按矩阵选可用 model；该 provider 无任何 model 支持时再回退到下一个 provider
- 不支持图片的 model 自动在 provider 特定层剥离图片，避免上游 400/404

**网关精炼层 Refiner（可选，默认全关）**

- 默认网关字节级原样转发请求/响应，不打开就不改写任何内容
- 三个独立精炼器，各有一个设置页全局总闸，并支持按 provider 单独覆写为强制关（只能关，不能强开）：
  - **请求字段过滤** —— 按 provider quirks 剥掉它不支持的请求字段，避免 400
  - **推理参数校正** —— 把 `thinking.budget_tokens` / `reasoning.effort` 归一到该 provider 接受的形态和范围
  - **错误响应归一** —— 把上游错误结构改写成客户端（Codex / Claude Code / Gemini CLI）期望的形态
- provider quirks 来自内置默认值，可按 provider 覆写；每次精炼动作都记录到请求日志的 `trace_json`

**多 Provider 管理**

- **26 个内置 Provider 预设**（选 type 后 base URL / 协议 / Anthropic 端点 / 默认模型自动填好）：
  - **国内**：小米 MiMo、DeepSeek、Kimi/Moonshot、MiniMax、智谱 GLM、通义千问 DashScope、硅基流动 SiliconFlow、火山引擎（豆包）、百川、阶跃星辰 StepFun、商汤 SenseNova、魔搭 ModelScope、零一万物 Yi
  - **海外**：OpenAI、Anthropic（Claude）、GitHub Copilot、Google Gemini、xAI（Grok）、Mistral、Groq、Together、Fireworks、Cerebras、Perplexity、Cohere
  - **聚合器**：OpenRouter
  - **自定义**：任意 OpenAI 兼容接口（vLLM / Ollama / LiteLLM / 本地代理）
- MiMo 一等公民：5 个聊天模型（`mimo-v2.5-pro` / `mimo-v2-pro` / `mimo-v2.5` / `mimo-v2-omni` / `mimo-v2-flash`）、多轮 `reasoning_content` 回环、`sk-*` / `tp-*` key 自动切到对应开放 API 或 Token Plan host、Token Plan 区域域名自动保持（`cn` / `sgp` / `ams`）、付费 `web_search` 未开通时自动降级重试
- Claude Code 直连 MiMo / DeepSeek 默认使用普通模型 ID；AgentGate 不再自动配置 `[1m]` 后缀模型。
- Route Profile 配置多 Provider 优先级链，按协议自动匹配
- 手动切换或智能转移（failover）；每条 failover 路由可选选模策略：优先级（默认）/ 最便宜（按单价）/ 最快（按近期延迟）
- Provider cooldown 和运行时状态追踪
- 请求级 failover：A 失败 → 自动 try B
- 能力感知路由：含图/音频/etc. 请求按矩阵在 provider 内自动 swap model，provider 不可用时跨链 fallback
- 新增 Provider 自动加入所有路由链
- Provider 模型列表自动获取
- 连接稳定性：HTTP client 调优 `pool_idle_timeout` + `tcp_keepalive`，应用层兼容 transient connect/timeout 重试（避免静默后死链接导致失败）

**客户端配置管理**

- Codex 一键配置 + 官方/AgentGate 一键切换（保留对话记录）
- Codex 桌面端兼容：模型请求可路由到第三方 API，同时保留官方 OpenAI provider 路径、登录态和插件/账号能力兼容性
- Claude Code 一键配置 + 官方/AgentGate 一键切换
- OpenCode 一键配置写入
- Claude Desktop（macOS / Windows）：把它的第三方推理网关指向 AgentGate，一键应用 + 历史回滚
- 全局指令文件管理：在 AgentGate 内直接编辑 `~/.claude/CLAUDE.md` / `~/.codex/AGENTS.md`，内置 6 个模板按用途分组（通用 / 编码 / 评审 / 调试 / 安全 / 文档）可覆盖或追加，写盘前自动 snapshot 可一键回滚，并支持 JSON 备份/恢复
- MCP 服务器管理：在一个面板里读取、新增、编辑、删除、同步 Codex 与 Claude Code 的 MCP 配置；列表不展示 env value；支持 JSON 导入/导出，默认不含密钥
- 本地技能（Skills）管理：列出、启用/禁用、删除 `~/.claude/skills` 与 `~/.codex/skills` 下的技能；支持从本地 `.zip` 导入（zip-slip 防护、不联网下载）和 JSON 备份/恢复
- Gateway 本地访问令牌（ag_local_\*）认证

**桌面增强**

- 系统托盘常驻，关闭窗口后网关继续后台运行
- 开机自启、应用内自动更新、中英双语界面、8 套内置主题
- 可选桌面宠物：显示网关状态、请求统计和错误气泡，也可以在设置中关闭

**快速配置 & 诊断**

- 首次引导：粘贴 API Key → 自动识别 Provider → 选择工具 → 一键完成配置
- 快速添加 Provider：粘贴 API Key，已知前缀自动识别；无法唯一识别时手动选择
- 连接测试：客户端页面三步状态条（配置 → 网关 → 供应商）
- 侧边栏快速配置页面（有 Provider 后自动隐藏，可在设置中重新开启）

**诊断与可观测**

- 请求日志、token 统计、成本估算、Provider 运行状态
- 成本分解：按模型 / 按客户端 / 按路由策略，可选时间区间（7/30 天）
- Provider 异常态在卡片可见：冷却 / 连续失败 / 配额耗尽，一键重置
- 主动健康探测（可选，默认关，探测消耗少量 token 所以需显式开启）：定期对每个 provider 发最小探测，结果显示在卡片；开启后还为"最快"路由策略提供冷启动延迟兜底（无近期请求记录的 provider 不再盲排末尾）
- 诊断自检和脱敏诊断包导出
- 能力降级事件记录：图片剥离、web_search 降级、MCP advisory、tool output 图片省略

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

| 全局指令 | MCP 服务器 |
|:---:|:---:|
| ![全局指令](docs/screenshots/instructions.png) | ![MCP 服务器](docs/screenshots/mcp.png) |

| 技能 | 快速配置 |
|:---:|:---:|
| ![技能](docs/screenshots/skills.png) | ![快速配置](docs/screenshots/quick-setup.png) |

| 宠物设置 | 桌面宠物 |
|:---:|:---:|
| ![宠物设置](docs/screenshots/pet-settings.png) | ![宠物](docs/screenshots/pet.png) |

## 技术栈

| 层          | 技术                                    |
| ----------- | --------------------------------------- |
| 桌面框架    | Tauri v2                                |
| 前端        | React 19 + TypeScript + Tailwind CSS v4 |
| 后端        | Rust + Tokio + Axum                     |
| 数据库      | SQLite (rusqlite, WAL 模式)             |
| HTTP 客户端 | reqwest                                 |

## 安装与构建

### 下载安装

从 [Releases](../../releases) 页面下载对应平台的安装包。

| 平台 | 格式 |
|---|---|
| macOS (Apple Silicon) | `.dmg` (aarch64) |
| macOS (Intel) | `.dmg` (x86_64) |
| Windows | `.exe` |
| Linux | `.AppImage` / `.deb` |

> **平台支持说明**：核心网关（协议转换、路由、故障自愈、成本仪表盘、客户端配置一键应用/还原）三平台均可用。便利功能（应用配置后自动重启 Codex、Claude Desktop 集成、运行中客户端检测）支持 macOS 和 Windows（Windows 实现较新，遇到问题请提 Issue）；Linux 上应用配置后请手动重启客户端。欢迎贡献。

<details>
<summary><b>macOS 首次打开提示"无法验证开发者"？</b>（点击展开）</summary>

如果 macOS Gatekeeper 拦截应用，可以用下面任一方式打开：

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

大多数人只需要上面的 [5 分钟跑通](#5-分钟跑通)。展开看完整参考。

<details>
<summary><b>完整使用指南 —— Provider、客户端、API 调用、失败转移、能力路由、诊断</b></summary>

### 1. 添加 Provider

启动 AgentGate → **供应商** → **添加供应商**

**快速通道（推荐）——粘 API Key 即可：**

1. 顶部输入框粘贴 Provider API key
2. AgentGate 会按已知前缀（`sk-ant-` / `deepseek-` / `gsk_` / …）自动识别 Provider；如果前缀不唯一，手动选择类型即可
3. 点 **创建** —— name / base URL / 协议 / default 模型 / reasoning 模型 / 能力矩阵全部自动填好。完事。

**手动模式 —— 三段式，只 Section A 必填：**

| Section | 字段 | 说明 |
|---|---|---|
| **基础** | 类型 · 名称 · API Key（custom 类型才显式露 Base URL） | 选 type 后剩下全部按 preset 自动填 |
| **模型与能力** | 默认模型 · 推理模型 · `拉取并识别能力` 按钮 · 能力矩阵折叠开关 | 创建 Provider 后**后台自动跑**：拉模型列表 → 按名字 seed 矩阵 → 挑最新非 mini 作 default、最新推理系作 reasoning，**不用手点** |
| **高级** *（默认折叠，"通常无需修改"）* | 协议+对应 URL 合并视图（Chat / Responses / Anthropic 各自一行）· 额外请求头 · 超时 · 自动 cache 控制 · 模型映射 | 协议每勾一个，下面就显示对应 URL——一眼看清"这个上游同时支持哪些原生入口" |

**模型映射** 放在高级最底部是有原因的：**通常无需配置**。创建 Provider、拉取模型、测试 Provider、应用 Codex / Claude Code 配置时，AgentGate 会为 MiMo / DeepSeek 自动补齐推荐映射，且不覆盖已有映射。原生直通未命中映射时会保留 `model` 原样；但客户端传 `agentgate` 虚拟模型时，会解析成本次路由选中的真实模型。协议转换会优先使用映射，未配置时用 `default_model` 兜底，以兼容 Codex / Claude Code / Gemini CLI 这类客户端。

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

AgentGate 会写入 `~/.config/opencode/opencode.json`，配置 OpenAI 兼容 Provider 指向本地 Gateway。配置里使用 `openai/agentgate` 虚拟模型，这样以后在 AgentGate 里切换 Provider，不需要再手改 OpenCode。

### 6. 配置 Gemini CLI

**客户端** → **Gemini CLI** → **应用配置**

AgentGate 把 Gemini CLI 配置指向本地网关的 `/v1beta/...` 路由（Gemini 兼容入口）。一键切换回官方。

### 7. 配置 AtomCode

**客户端** → **AtomCode** → **应用配置**

AtomCode 把上游配置指向 AgentGate，切换模式与其他客户端一致。配置里使用 `agentgate` 虚拟模型，真实的 DeepSeek / MiMo / 其他 provider model 由网关在请求时解析。

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

**客户端提示“网络连接失败”？**

优先检查网关端口是否启动：

```bash
curl http://127.0.0.1:9090/health
```

- 如果连接失败：回到 AgentGate 的 **概览 / 网关 / 客户端** 页面，点击 **启动网关**。
- 如果能返回健康状态，再看 **客户端** 页面里的三步测试：配置 → 网关 → 供应商。
- `http://localhost:1420` 只是开发时的前端页面；Codex / Claude Code / OpenCode / Gemini CLI / AtomCode 调用的是 `http://127.0.0.1:9090`。

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
- 请求日志的 `trace_json` 会记录 `degradation_events`：当 AgentGate 剥离不受支持的图片、原生 web_search、MCP connector、tool output 图片时，可用于后续 UI 展示和问题排查。

</details>

## 支持的 Provider

标了 **专属处理** 的 Provider 在 `src-tauri/src/transform/providers/` 下有专门转换代码。其余走通用 Chat Completions / Anthropic 透传路径，开箱即用。

<!-- PROVIDER_CATALOG_TABLE:START -->
| Provider | 类型 | 原生协议 | 专属处理 |
|---|---|---|---|
| 小米 MiMo | `mimo` | Chat + Anthropic | 多轮 `reasoning_content` 回环、`tp-*` host 按区域自动切换、思考态剥 temperature、tool_choice 非 auto 剥除、omni web_search 剥除、web_search builtin 按矩阵翻译、Web Search Plugin 自动降级 / 重试 |
| DeepSeek | `deepseek` | Chat + Anthropic | 图片剥离并注入可解释提示、DeepSeek V4 thinking 历史 reasoning 回填、schema 清洗、消息重排 |
| Anthropic（Claude） | `anthropic` | Anthropic | `tool_use`/`tool_result`、`input_schema`、thinking budget、原生 cache_control |
| GitHub Copilot | `copilot` | Chat + Anthropic | GitHub token → Copilot bearer 交换、`x-initiator` 计费分类、Claude 模型 dash→dot 归一化 |
| OpenAI | `openai` | Chat + Responses | 无（Responses 透传或 Chat 转换） |
| Google Gemini | `google_gemini` | Chat | 无 |
| Kimi / Moonshot | `kimi` | Chat | `web_search` → `builtin_function`/`$web_search`、thinking 控制 |
| MiniMax | `minimax` | Chat | 去 reasoning_effort / response_format、`<think>` 提取 |
| 智谱 GLM | `glm` | Chat | 通用 |
| 通义千问 DashScope | `dashscope` | Chat | 通用 |
| 硅基流动 SiliconFlow | `siliconflow` | Chat | 通用 |
| 火山引擎（豆包） | `volcengine` | Chat | 通用 |
| 百川 | `baichuan` | Chat | 通用 |
| 阶跃星辰 StepFun | `stepfun` | Chat | 通用 |
| 商汤日日新 SenseNova | `sensenova` | Chat | 清理 strict:null / response_format / 非 function 工具,合并 system 消息 |
| 零一万物 Yi | `yi` | Chat | 通用 |
| 魔搭 ModelScope | `modelscope` | Chat | 通用 |
| xAI（Grok） | `xai` | Chat | 通用 |
| Mistral | `mistral` | Chat | 通用 |
| Groq | `groq` | Chat | 通用 |
| Together | `together` | Chat | 通用 |
| Fireworks | `fireworks` | Chat | 通用 |
| Cerebras | `cerebras` | Chat | 通用 |
| Perplexity | `perplexity` | Chat | 通用 |
| Cohere | `cohere` | Chat | 通用 |
| OpenRouter | `openrouter` | Chat | 无 |
| 自定义 | `custom_openai_compatible` | Chat | 无（Base URL 用户自己填） |
<!-- PROVIDER_CATALOG_TABLE:END -->

> Vision / reasoning / tools / web_search 等能力是**按每个 model**追踪的，不在 provider 层。详见上文 *能力感知路由*。

## 架构与内部原理

<details>
<summary><b>数据链路、请求模式与 Gateway 路由</b></summary>

### 数据链路

AgentGate 把"协议是否转换"和"模型名是否改写"分开看。常见有三种请求模式：

> **如何区分？** 先看客户端协议是否匹配上游原生入口。匹配就不做协议转换；模型名是另一层规则：命中 Model Mapping 时仍然会改写 `model`。

| 客户端 | 发送协议 | 下游 Provider | AgentGate 模式 | 触发条件 |
|---|---|---|---|---|
| Codex | Responses API | Chat Completions | 协议转换 | 默认（无特殊 URL） |
| Codex | Responses API | Claude Messages API | 协议转换 | `provider_type` 为 `anthropic` |
| Codex | Responses API | Responses API | 原生直连 | 配置了 `responses_base_url` |
| Claude Code | Messages API | Chat Completions | 协议转换 | 无 `anthropic_base_url` |
| Claude Code | Messages API | Anthropic 兼容端点 | 原生直连 + 模型映射 | 配置了 `anthropic_base_url`，且 `claude-*` 映射到上游模型 |
| OpenCode | Chat Completions | Chat Completions | 原生直连 | 同协议且模型名就是上游模型名 |
| curl / New API 等 | Chat Completions | Chat Completions | 原生直连 | 同协议且模型名就是上游模型名 |

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

### 原生直连

客户端协议和下游 Provider 一致时，AgentGate 不做格式转换，只替换目标地址和凭证。模型名遵循一条规则：Model Mapping 命中就改写；未命中就保留客户端请求里的 `model`；如果请求模型是 `agentgate` 或 `openai/agentgate`，则解析成本次路由选中的真实模型；客户端没传 `model` 时才使用 provider 默认模型。

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
│  ③ 原生直连                                                             │
│     替换目标 URL（base_url 或 anthropic_base_url）                      │
│     注入 Provider API Key                                               │
│     可选模型映射（claude-* → mimo/deepseek 模型）                      │
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
  • /v1/messages    + Provider 配置了 anthropic_base_url → Messages API 原生直连
  • /v1/chat/completions → Chat Completions 原生直连（所有 Provider 都支持）
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

**Claude Code → DeepSeek（原生直连 + 模型映射）：**

```
Claude Code 发送 Messages API 请求
  → /v1/messages
  → ① 认证通过
  → ② 匹配 Claude Code Default Route Profile
  → ③ DeepSeek 配有 anthropic_base_url → 原生直连
  → 模型映射：claude-sonnet-4-6 → deepseek-v4-pro
  → 替换 URL 为 api.deepseek.com/anthropic + 注入 API Key
  → 请求体原样透传 → SSE 响应流原样回传
  → ④ 记录日志
```

**OpenCode / curl / New API 客户端（原生直连）：**

```
客户端发送 Chat Completions 请求
  → /v1/chat/completions
  → ① 认证通过
  → ② 匹配 Route Profile
  → ③ 原生直连：替换 URL + API Key；model 未命中映射时保持原样
  → 请求体原样转发 → SSE 响应流原样回传
  → ④ 记录日志
```

### Gateway 路由

| 方法 | 路径                   | 模式         | 说明                         |
| ---- | ---------------------- | ------------ | ---------------------------- |
| GET  | `/health`              | internal     | 健康检查（无需认证）         |
| GET  | `/v1/models`           | internal     | 模型列表                     |
| POST | `/v1/responses`        | 自动         | 有 `responses_base_url` → 透传；Anthropic 类型 → Claude 转换；其他 → Chat Completions 转换 |
| POST | `/v1/chat/completions` | pass-through | Chat Completions 直通        |
| POST | `/v1/messages`         | 自动         | 有 `anthropic_base_url` → 透传；否则 → Chat Completions 转换 |

</details>

## 项目结构

```
AgentGate/
├── provider-catalog/             # Provider / model 默认数据的单一事实源
├── src/                          # 前端 React
│   ├── app/App.tsx               # 应用入口
│   ├── pages/                    # 页面（概览/快速配置/供应商/路由/网关/客户端/日志/诊断/全局指令/MCP/技能/设置）
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
│       ├── tools/                # 客户端配置 + MCP / 技能 / 全局指令管理
│       ├── security/             # 认证与脱敏
│       ├── diagnostics/          # 诊断与自检
│       ├── app/                  # Tauri commands 与应用状态
│       └── errors/               # 统一错误类型
├── scripts/                      # 测试与生成脚本
└── package.json
```

## 安全

- Gateway 默认启用本地令牌认证（恒定时间比较，防时序攻击）
- Provider API Key 仅存储在本地 SQLite，不会发送给客户端
- 客户端传入的令牌不会转发给上游 Provider
- 请求日志与诊断包自动脱敏：`sk-` 系 key、Bearer、`x-api-key`、`api_key` 字段、Gemini 风格 `?key=` 查询参数全覆盖
- 桌面应用仅监听 `127.0.0.1`；Headless 模式（`agentgate-serve`）需显式 `--host` 才会对外，并内置 Host / Origin 校验防 DNS rebinding（域名访问需 `AGENTGATE_ALLOWED_HOSTS` 白名单放行）
- 令牌文件权限设置为 `0600`（Unix）

## 常见问题

**我的 API Key 安全吗？AgentGate 会不会上传数据？**
Key 只存在你本机的 SQLite 文件里，不会发给客户端，也不会发给任何 AgentGate 服务器——AgentGate 根本没有后端。你的 Key 只会发给你自己配置的上游 Provider。桌面应用只监听 `127.0.0.1`；只有 Headless 模式显式传 `--host 0.0.0.0` 才会对外，且自带 Host / Origin 校验。

**用 Copilot 订阅跑 Claude Code 会不会被封号？**
这属于 GitHub 服务条款的灰色地带：同类社区工具长期存在、目前未见大规模处罚，但风险无法排除，请自行评估（详见 [Copilot 章节](#用-github-copilot-订阅跑-claude-code--codex) 的风险声明）。不添加 copilot 类型的 Provider 就完全不涉及此功能。

**会不会把我的 Codex / ChatGPT 登录或 Codex 桌面端插件搞坏？**
不会。AgentGate 让 Codex 保持在官方 OpenAI 登录态的 provider 路径上，登录态、插件、Browser / Computer-Use / Mobile、配额查询都正常，同时把对话请求路由到第三方模型。随时点**切换到官方**还原原始配置，对话记录保留。

**能离线 / 在服务器上不带界面跑吗？**
能。Headless 模式（`agentgate-serve`）和 Docker 都不需要窗口，见 [Headless / 服务器模式](#headless--服务器模式)。

**还要手改 `config.toml` / `settings.json` 吗？**
不用。**应用配置**一键写好并备份原文件，**切换到官方**一键回滚。

**客户端报"网络连接失败"怎么办？**
先确认网关在跑：`curl http://127.0.0.1:9090/health`。连不上就在应用里启动网关。注意 `localhost:1420` 只是开发界面，客户端走的是 `127.0.0.1:9090`。

**一个请求到底打到了哪个模型？**
打开**日志**——每条请求都显示客户端、路由、实际选中的 provider、模型、状态和成本。

## 开发

### Provider Catalog

内置 Provider / Model 数据统一维护在 `provider-catalog/providers/*.json`。改完 catalog 后运行：

```bash
pnpm provider:catalog:generate
pnpm provider:catalog:check
pnpm provider:catalog:sync:check
```

`provider:catalog:sync:check` 会在存在对应 API Key 环境变量时调用供应商 `/models` 接口；没有密钥的供应商会跳过。上游不再返回 catalog 中的模型时会失败；加 `--strict` 时也会因为发现上游新模型而失败。显式刷新某个供应商：

```bash
pnpm provider:catalog:sync -- --provider deepseek --update
pnpm provider:catalog:generate
```

生成的 TS / Rust 文件会驱动 Provider 预设、endpoint 默认值、能力矩阵 seed、价格默认值、支持模型 gate 和推荐映射策略。Provider 特定运行时行为仍然留在代码模块里，不放进 catalog。

上游模型同步刻意设计成本地维护步骤，不放进 GitHub 发版 workflow。Release CI 只检查生成产物是否与 catalog 一致。

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

## 社区与支持

- 🐛 **发现 bug 或想要功能？** 提 [Issue](https://github.com/dengmengmian/agentgate-ai/issues)——网关问题请附上脱敏诊断包(诊断 → 导出)。
- 💡 **提问与想法：** 到 [Discussions](https://github.com/dengmengmian/agentgate-ai/discussions) 交流安装问题、Provider 请求、工作流建议和成功案例。
- 🧩 **想参与贡献？** 从 [CONTRIBUTING.md](./CONTRIBUTING.md)、[PR 模板](./.github/pull_request_template.md) 和 `good first issue` 开始。
- ⭐ **如果 AgentGate 帮你省了钱或时间,点个 star** —— 既帮更多人发现它,也帮我们决定下一步做什么。

三大支柱——智能路由、故障自愈、成本仪表盘——都已落地。后面做什么由 Issue 和 Discussion 驱动,缺什么告诉我们。

## Star History

<a href="https://star-history.com/#dengmengmian/agentgate-ai&Date">
  <img src="https://api.star-history.com/svg?repos=dengmengmian/agentgate-ai&type=Date" alt="Star History Chart" width="600">
</a>

## License

MIT
