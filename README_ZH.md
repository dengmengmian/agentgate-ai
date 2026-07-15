<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">
  <b>一个本地入口，统一管理你的 AI 模型请求。</b><br>
  AgentGate 是面向 AI 应用和客户端的本地 AI 网关，支持 Codex、Claude Code、Gemini CLI、OpenCode、AtomCode，也可接入兼容 OpenAI、Anthropic、Gemini 协议的应用。它把每一次请求路由到你选择的模型，在供应商故障时自动切换，本地完整追踪每一次请求，打通不同协议与任意 OpenAI 兼容供应商之间的差异，并在 macOS 和 Windows 上防止长时间 AI 任务被系统自动休眠打断。
</p>

<p align="center">
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/v/release/dengmengmian/agentgate-ai?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/stargazers"><img src="https://img.shields.io/github/stars/dengmengmian/agentgate-ai?style=flat-square&cacheSeconds=3600" alt="Stars"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/downloads/dengmengmian/agentgate-ai/total?style=flat-square&color=green&cacheSeconds=3600" alt="Downloads"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="https://github.com/dengmengmian/agentgate-ai/releases">下载安装</a> · <a href="#5-分钟跑通">5 分钟跑通</a> · <a href="./docs/full-reference.md">完整参考</a> · <a href="https://github.com/dengmengmian/agentgate-ai/discussions">💬 社区讨论</a>
</p>

<p align="center">
  GitHub：<a href="https://github.com/dengmengmian/agentgate-ai">dengmengmian/agentgate-ai</a>
</p>

<p align="center">
  <img src="docs/demo-header-v2.gif" width="800" alt="AgentGate 在本地网关截获 Claude Code、Codex、Gemini CLI 的请求——转换 / 直连 / 路由 / 故障转移到 26 家上游，每条请求都在本地可追踪">
</p>

> **v1.5.0 新增防休眠：** AgentGate 在 macOS 和 Windows 运行期间默认阻止系统自动休眠，显示器仍可正常关闭。也可开启“请求智能控制”，仅在 AI 生成请求进行中及可配置冷却期内保持唤醒；设置页和系统托盘都能快速切换。Linux 本版可使用托盘菜单，但暂不支持防休眠。[查看 v1.5.0 更新说明](./docs/release-notes/1.5.0.md)。

## 下载

| 你的机器 | 下载 |
|---|---|
| macOS Apple 芯片 | [AgentGate_1.5.0_aarch64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.5.0/AgentGate_1.5.0_aarch64.dmg) |
| macOS Intel 芯片 | [AgentGate_1.5.0_x64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.5.0/AgentGate_1.5.0_x64.dmg) |
| Windows 10 / 11 | [AgentGate_1.5.0_x64-setup.exe](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.5.0/AgentGate_1.5.0_x64-setup.exe) |
| Debian / Ubuntu | [AgentGate_1.5.0_amd64.deb](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.5.0/AgentGate_1.5.0_amd64.deb) |
| 其他 Linux 发行版 | [AgentGate_1.5.0_amd64.AppImage](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.5.0/AgentGate_1.5.0_amd64.AppImage) |

**Windows 安装提示：** Edge/Chrome 可能提示「通常不会下载」，SmartScreen 可能提示「Windows 已保护你的电脑」。这是预期行为：安装包目前**未做 Authenticode 代码签名**。业界没有像 Let’s Encrypt 那样「免费且被 Windows 默认信任」的代码签名证书（免费证书只管 HTTPS 网站，不能签 `.exe`），开源项目常见先发未签名包——**不是病毒报错**。请只从 [GitHub Releases](https://github.com/dengmengmian/agentgate-ai/releases) 下载；浏览器里选 **保留 / 仍要保留**，运行时点 **更多信息** → **仍要运行**。

macOS 也可以用 Homebrew 安装：

```bash
brew install --cask dengmengmian/tap/agentgate
```

无界面 CLI（`agentgate-serve`）压缩包和历史版本在 [Releases](https://github.com/dengmengmian/agentgate-ai/releases) 页面。

## 为什么用 AgentGate

| 官方体验不变 | 模型路由归你管 | 每次请求看得见 |
|:---|:---|:---|
| 保留 Codex / Claude Code / Gemini CLI / OpenCode / AtomCode 原有使用方式，并支持一键恢复官方配置。 | 官方客户端请求先进入 AgentGate，再由本地决定协议转换、原生直连、Provider 和模型。 | 每次请求的路由、转换、上游错误、Token、成本、延迟和失败转移都在本地可追踪。 |

AgentGate 不是托管 API 分发平台，也不是普通代理。它是 AI 模型请求的本地入口——每一次模型请求都先进入 AgentGate，再由你决定路由、转换、故障转移还是原生直连。

## 5 分钟跑通

1. 下载并安装 AgentGate。
2. 打开 **快速配置** 或 **供应商**，粘贴你的 Provider API Key。
3. 在 **概览** 或 **网关** 点击 **启动网关**。默认端点是 `127.0.0.1:9090`。
4. 进入 **客户端**，对 Codex、Claude Code、OpenCode、Gemini CLI 或 AtomCode 点击 **应用配置**。
5. 回到对应客户端发一句话测试。需要恢复官方配置时，用 **切换到官方** 或配置历史回滚。

Provider 预设会填好常见 base URL、协议、默认模型和能力矩阵。新手通常不用先碰模型映射或高级端点字段。

## 常见用途

| 目标 | AgentGate 做什么 |
|---|---|
| 让 Codex 使用 DeepSeek 或 MiMo | Codex 继续发送官方 Responses API 请求，AgentGate 再转换或直连到你选的上游。 |
| 保留 Codex Desktop 插件能力 | 保持官方 OpenAI 登录态和 provider 识别路径，同时让模型请求走 AgentGate。 |
| 让 Claude Code 使用 DeepSeek / MiMo / Copilot | 支持 Anthropic 兼容直通、模型名映射，以及可选 GitHub Copilot Provider。 |
| 避免 Provider 挂掉或额度卡住 | 按状态码、错误关键词、超时和冷却状态尝试故障转移 Provider。 |
| 防止长时间 AI 任务被系统休眠打断 | macOS 和 Windows 默认防休眠，也可按 AI 请求智能控制，并通过系统托盘快速切换。 |
| 看清每一次请求 | 记录原始/转换后请求、路由决策、上游错误、Token、延迟和预估成本。 |

<details>
<summary>支持的 Provider</summary>

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

</details>

## 教程

- [Codex Desktop 插件兼容](./docs/use-codex-desktop-with-third-party-api-and-plugins.md)
- [Codex + DeepSeek](./docs/use-codex-with-deepseek.md)
- [Codex + 小米 MiMo](./docs/use-codex-with-mimo.md)
- [Claude Code + DeepSeek](./docs/use-claude-code-with-deepseek.md)
- [Claude Code + GitHub Copilot](./docs/use-claude-code-with-github-copilot.md)
- [Gemini CLI](./docs/use-gemini-cli-with-agentgate.md)
- [OpenCode](./docs/use-opencode-with-agentgate.md)
- [完整参考](./docs/full-reference.md)

## 截图

| Dashboard | Providers |
|---|---|
| ![Dashboard](docs/screenshots/dashboard.png) | ![Providers](docs/screenshots/providers.png) |

| Routes | Logs |
|---|---|
| ![Routes](docs/screenshots/routes.png) | ![Logs](docs/screenshots/logs.png) |

## 注意

- GitHub Copilot Provider 是可选功能。在官方客户端之外使用 Copilot 订阅可能存在账号风险，启用前请阅读专门教程。
- 网关端点是 `127.0.0.1:9090`；`localhost:1420` 只是开发 UI 端口。
- AgentGate 是本地优先、单用户工具。如果你要运营共享 API 服务或计费平台，one-api、new-api、LiteLLM 可能更合适。

## 开发

```bash
pnpm install
pnpm tauri dev
```

常用检查：

```bash
pnpm test
pnpm build
cd src-tauri && cargo test
```

## 社区

- 问题和安装帮助：[Discussions](https://github.com/dengmengmian/agentgate-ai/discussions)
- Bug 和 Provider 请求：[Issues](https://github.com/dengmengmian/agentgate-ai/issues)
- 贡献指南：[CONTRIBUTING.md](./CONTRIBUTING.md)

## License

MIT
