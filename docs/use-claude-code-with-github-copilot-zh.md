# 用 GitHub Copilot 订阅跑 Claude Code

English: [Use Claude Code with GitHub Copilot through AgentGate](./use-claude-code-with-github-copilot.md)

AgentGate 把 Claude Code 的 Anthropic Messages 入口变成本地模型入口，再把选中的请求路由到 GitHub Copilot 订阅里包含的 Claude 模型。这是一个可选功能，仅用于个人评估；在官方客户端之外使用 Copilot 属于 GitHub 服务条款的灰色地带。

## 什么时候用这个

如果你想：

- 让 Claude Code 使用 Copilot 提供的 Claude 模型，不用额外申请 Anthropic API Key。
- 让 AgentGate 自动把你的 GitHub OAuth token 兑换成 Copilot API 凭证。
- 把工具续写和历史压缩的请求标记成 agent 流量，**不消耗** premium 高级请求额度。
- 在日志里看到每条请求的 `x-initiator` 分类。

## 风险声明

这个功能完全可选。如果你没有添加 `copilot` 类型的 Provider，AgentGate 不会走这条路径。

在官方客户端之外使用 Copilot 订阅，属于服务条款的灰色地带。社区里类似行为的工具存在很久了，但**账号风险无法完全排除**。**不要用重要的公司账号做实验。**

## 快速配置

1. 确认你有一个可用的 GitHub Copilot 订阅。
2. 拿到一个 GitHub OAuth token。如果你登录过 VS Code Copilot，可以在 `~/.config/github-copilot/apps.json` 里找到 `oauth_token`。
3. 打开 AgentGate，进入 **供应商**，添加一个类型为 **GitHub Copilot** 的 Provider。
4. 把 `gho_` 或 `ghu_` 开头的 token 粘贴成 API Key。AgentGate 会自动填好 base URL 和模型列表。
5. 在 **概览** 或 **网关** 启动网关。
6. 打开 **客户端**，应用 Claude Code 配置。
7. 发一条测试消息，在 **日志** 里查 Provider 是 `GitHub Copilot`，以及 `x-initiator` 的分类。

## AgentGate 处理了什么

| 方面 | 行为 |
|---|---|
| 凭证兑换 | GitHub OAuth token 被自动兑换成 Copilot bearer 凭证，并自动续期。 |
| 存储 | Copilot 凭证按 hash 缓存，不以明文 token 形式存储。 |
| Premium 请求分类 | 用户消息标成 user 流量；工具续写和历史压缩标成 agent 流量。 |
| 模型名归一 | Claude Code 的模型名如 `claude-sonnet-4-6` 会归一成 Copilot 期望的形式。 |

## 排查

| 现象 | 检查 |
|---|---|
| Token 兑换失败 | 确认 token 是 `gho_` 或 `ghu_` 开头，并且属于一个有 Copilot 权限的账号。 |
| 模型被拒 | 保存 Provider 后看一下 Copilot Provider 的模型列表。 |
| Premium 请求高于预期 | 打开请求日志，对比 `x-initiator: user` 和 `x-initiator: agent` 的数量。 |
| 想停掉这条路径 | 删除或禁用 Copilot Provider，改用 DeepSeek、MiMo、Anthropic 或其他 Provider。 |

## 相关教程

- [让 Claude Code 使用 DeepSeek](./use-claude-code-with-deepseek-zh.md)
- [让 Codex 使用 DeepSeek](./use-codex-with-deepseek-zh.md)
- [English README](../README.md)
- [中文 README](../README_ZH.md)
