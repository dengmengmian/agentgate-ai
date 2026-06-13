<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">
  <b>让 Codex、Claude Code、Gemini CLI、OpenCode、AtomCode 通过一个本地模型网关运行。</b><br>
  Agent 协议兼容 · 客户端一键配置 · 带成本可见性的多 Provider 路由。
</p>

<p align="center">
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/v/release/dengmengmian/agentgate-ai?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/stargazers"><img src="https://img.shields.io/github/stars/dengmengmian/agentgate-ai?style=flat-square" alt="Stars"></a>
  <a href="https://github.com/dengmengmian/agentgate-ai/releases"><img src="https://img.shields.io/github/downloads/dengmengmian/agentgate-ai/total?style=flat-square&color=green" alt="Downloads"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/github/license/dengmengmian/agentgate-ai?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="https://github.com/dengmengmian/agentgate-ai/releases">下载安装</a> · <a href="#5-分钟跑通">5 分钟跑通</a> · <a href="./docs/full-reference.md">完整参考</a>
</p>

<p align="center">
  <img src="docs/demo-header-v2.gif" width="800" alt="AgentGate 通过本地网关把编程 Agent 路由到更便宜的 Provider，并实时统计成本">
</p>

## 下载

| 你的机器 | 下载 |
|---|---|
| macOS Apple 芯片 | [AgentGate_1.4.1_aarch64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_aarch64.dmg) |
| macOS Intel 芯片 | [AgentGate_1.4.1_x64.dmg](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_x64.dmg) |
| Windows 10 / 11 | [AgentGate_1.4.1_x64-setup.exe](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_x64-setup.exe) |
| Debian / Ubuntu | [AgentGate_1.4.1_amd64.deb](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_amd64.deb) |
| 其他 Linux 发行版 | [AgentGate_1.4.1_amd64.AppImage](https://github.com/dengmengmian/agentgate-ai/releases/download/v1.4.1/AgentGate_1.4.1_amd64.AppImage) |

无界面 CLI（`agentgate-serve`）压缩包和历史版本在 [Releases](https://github.com/dengmengmian/agentgate-ai/releases) 页面。

## 为什么用 AgentGate

| Agent 原生兼容 | 客户端一键配置 | 成本可见的多 Provider 路由 |
|:---|:---|:---|
| 连接 Codex Responses、Claude Messages、Gemini、Chat Completions，让编程 Agent 能使用更多 Provider。 | 不手改本地配置文件，也能应用和还原 Codex / Claude Code / OpenCode / Gemini CLI / AtomCode 配置。 | 在 26 个 Provider 间路由，支持故障转移、能力检查、延迟/价格策略和逐请求成本统计。 |

AgentGate 不是托管 API 分发平台，而是给编程 Agent 用户准备的本地桌面工作台：保住客户端特有行为，同时获得 Provider 选择权。

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
| 让 Codex 使用 DeepSeek 或 MiMo | 把 Codex Responses API 请求转换到兼容的上游格式，并处理模型映射。 |
| 保留 Codex Desktop 插件能力 | 保持官方 OpenAI 登录态和 provider 识别路径，同时让模型请求走 AgentGate。 |
| 让 Claude Code 使用 DeepSeek / MiMo / Copilot | 支持 Anthropic 兼容直通、模型名映射，以及可选 GitHub Copilot Provider。 |
| 避免 Provider 挂掉或额度卡住 | 按状态码、错误关键词、超时和冷却状态尝试故障转移 Provider。 |
| 按模型和客户端看花费 | 记录请求日志、token、延迟、路由决策和预估成本。 |

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
