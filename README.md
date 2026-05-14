<p align="center">
  <img src="docs/logo.svg" width="128" height="128" alt="AgentGate Logo">
</p>

<h1 align="center">AgentGate</h1>

<p align="center">Local gateway for AI coding agents.</p>

<p align="center">
  <a href="./README_EN.md">English</a>
</p>

AgentGate 是面向 AI 编程 Agent 的本地模型网关与 Provider 切换工具。它为 Codex、Claude Code、OpenCode 等 CLI 工具提供统一的本地入口，支持协议转换、多 Provider 切换、自动故障转移和请求日志。

## 核心功能

**协议转换与统一入口**

- OpenAI Responses API (`/v1/responses`) → Chat Completions 转换，支持 Codex
- Anthropic Messages API (`/v1/messages`) → Chat Completions 转换，支持 Claude Code
- Chat Completions (`/v1/chat/completions`) 直通转发
- DeepSeek reasoning_content 思考模式完整支持（不降智）
- 工具调用（function_call）流式拼接与多轮对话

**多 Provider 管理**

- 支持 DeepSeek、OpenAI、OpenRouter、Kimi、自定义 OpenAI 兼容接口
- Route Profile 配置多 Provider 优先级链，按协议自动匹配
- 手动切换或自动故障转移（failover）
- Provider cooldown 和运行时状态追踪
- 请求级 failover：A 失败 → 自动 try B
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

填写：

- 名称：如 `DeepSeek`
- 类型：`deepseek`
- Base URL：`https://api.deepseek.com`
- API Key：你的 DeepSeek API Key
- 默认模型：`deepseek-v4-pro`

保存后点击 **获取模型** 自动加载可用模型列表。

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

所有接口（除 `/health`）需要携带认证：

```bash
TOKEN=$(cat ~/.agentgate/token)
```

**Chat Completions（直通转发）**

```bash
curl -X POST http://127.0.0.1:9090/v1/chat/completions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-pro","messages":[{"role":"user","content":"你好"}]}'
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

### 7. 多 Provider 与故障转移

**路由** 页面可以配置 Route Profile：

1. 每种协议自动创建默认路由（Codex / Claude Code / OpenCode）
2. 添加多个 Provider 到 Provider Chain，调整优先级
3. 切换模式：手动 / 故障转移
4. 故障转移模式下，429/402/5xx/超时 错误会自动尝试下一个 Provider

### 8. 诊断

**诊断** 页面：

- **运行自检** — 检查 Gateway、Provider、配置、数据库状态
- **导出诊断包** — 生成脱敏诊断信息用于排查问题

## 支持的 Provider

| Provider   | 类型                       | 协议                    |
| ---------- | -------------------------- | ----------------------- |
| DeepSeek   | `deepseek`                 | OpenAI Chat Completions |
| OpenAI     | `openai`                   | OpenAI Chat Completions |
| OpenRouter | `openrouter`               | OpenAI Chat Completions |
| Kimi       | `kimi`                     | OpenAI Chat Completions |
| 自定义     | `custom_openai_compatible` | OpenAI Chat Completions |

## Gateway 路由

| 方法 | 路径                   | 模式         | 说明                         |
| ---- | ---------------------- | ------------ | ---------------------------- |
| GET  | `/health`              | internal     | 健康检查（无需认证）         |
| GET  | `/v1/models`           | internal     | 模型列表                     |
| POST | `/v1/responses`        | transform    | Responses → Chat Completions |
| POST | `/v1/chat/completions` | pass-through | Chat Completions 直通        |
| POST | `/v1/messages`         | transform    | Messages → Chat Completions  |

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
│       ├── gateway/              # HTTP 网关（server/routes/SSE/pass-through/failover）
│       ├── protocol/             # 协议类型（Responses/ChatCompletions/Messages/SSE events）
│       ├── transform/            # 协议转换（responses→chat/schema清理/tool_calls/reasoning存储）
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
