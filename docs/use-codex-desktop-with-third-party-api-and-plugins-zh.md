# 让 Codex Desktop 使用第三方 API 并保留插件能力

English: [Use Codex Desktop with third-party APIs and plugins](./use-codex-desktop-with-third-party-api-and-plugins.md)

AgentGate 把 Codex Desktop 原本基于 OpenAI 鉴权的官方模型入口，变成一个本地 AgentGate 入口。Codex Desktop 保留它的插件和账号功能依赖的 Provider 路径，模型请求由本地接管，再路由到 DeepSeek、小米 MiMo、OpenAI、Kimi、GLM、DashScope 等上游 Provider。

## 为什么这件事重要

很多代理方案把 Codex 当成普通的 OpenAI 兼容客户端来处理。这样做普通对话够用，但 Codex Desktop 里那些依赖**官方 OpenAI Provider 形态、登录态、账号能力**的部分可能会失效。

AgentGate 走的是另一条路：**保留客户端看起来像官方的入口，把实际的模型入口搬到本地**。

```text
Codex Desktop
  -> Codex 配置里的 OpenAI Provider 入口
  -> 本地 AgentGate base URL
  -> DeepSeek / 小米 MiMo / OpenAI / Kimi / GLM / DashScope / 其他 Provider
```

Codex Desktop 看到的还是一个 OpenAI 风格的 Provider 入口，但模型请求被 AgentGate 在本地接住，再路由到你选的上游。

## 什么能继续用

| 方面 | AgentGate 保留了什么 |
|---|---|
| Codex Desktop 登录 | 保留官方 `auth.json` 里的账号 token，不用本地 API Key 覆盖它。 |
| 插件和账号功能 | 保留 Codex Desktop 走 OpenAI 鉴权的 Provider 路径，插件和账号功能依赖的就是这条路径。 |
| 本地模型入口 | 模型请求先经过 AgentGate，再转换或直连到 DeepSeek、MiMo、OpenAI、Kimi、GLM、DashScope 或其他配置的 Provider。 |
| 一键还原 | 保存原 Codex 配置的快照，可以从 AgentGate 界面一键切回官方。 |

插件能不能用还取决于 Codex Desktop、你登录的账号、以及上游功能本身。AgentGate 的作用是**不去破坏那条官方路径**，同时让模型请求本地可控、可追踪。

## 快速配置

1. 从 [Releases](../../releases) 下载 AgentGate 并打开应用。
2. 添加至少一个 Provider，比如 DeepSeek 或小米 MiMo。
3. 在 **概览** 或 **网关** 启动网关。默认端点是 `http://127.0.0.1:9090`。
4. 打开 **客户端**，在 Codex 卡片上点 **应用配置**。
5. Codex Desktop 的账号保持登录状态。
6. 在 Codex Desktop 里发一条消息，看 AgentGate 的请求日志确认选用的 Provider。

## 工作原理

AgentGate 的做法是：

- Codex 配置里仍然使用 OpenAI Provider 入口。
- `base_url` 指向本地 AgentGate 网关。
- `auth.json` 里的 ChatGPT / OpenAI 登录态保留，不被本地 token 覆盖。
- 本地访问 token 放在 Codex 配置里，由 AgentGate 网关校验。
- 真正的模型请求由 AgentGate 路由到 DeepSeek、小米 MiMo 或其他 Provider。

结果是：Codex Desktop 可以继续保持它熟悉的官方 Provider 语义，同时你可以在 AgentGate 里切换第三方 API。

## 跟普通代理的区别

| 普通代理 | AgentGate |
|---|---|
| 通常把 Codex 当成普通 OpenAI 兼容客户端。 | 保留 Codex Desktop 的 OpenAI Provider 路径，只改本地 base URL。 |
| 可能要把 auth 替换成代理的 API Key。 | 保留官方账号 token，本地网关 token 放在配置里。 |
| 通常只针对一个 Provider。 | 支持 Route Profile、故障转移、模型映射和多 Provider。 |
| 切回官方常常要手动改。 | 一键切回官方 Codex 配置。 |

## 相关教程

- [让 Codex 使用 DeepSeek](./use-codex-with-deepseek-zh.md)
- [让 Codex 使用小米 MiMo](./use-codex-with-mimo-zh.md)
- [English README](../README.md)
- [中文 README](../README_ZH.md)
