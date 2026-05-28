# Changelog / 更新日志

All notable changes to this project will be documented in this file.

## [1.1.2] - 2026-05-28

### 新增

- **更新安装完成后自动重启** —— `tauri-plugin-process` 接入，`downloadAndInstall` 成功后等 800ms（让 "安装完成，正在重启..." 文案渲染一帧）然后调用 `relaunch()`，用户不再需要手动 quit + 重开。覆盖两个入口：右下角 UpdateChecker 浮窗、Settings 页手动检查更新。

---

## [1.1.1] - 2026-05-27

### 新增

- **主题扩展到 8 套** —— 在原有 Warm Amber 暖琥珀 / Daylight 晴日基础上新增 6 套：
  - 暗色：Slate Steel 钢蓝（accent #38BDF8）/ Forest Pine 松林（#84B062）/ Midnight Violet 紫夜（#A78BFA）
  - 亮色：Linen Cream 米麻（#B66821 陶土）/ Mist Blue 雾蓝（#2563EB）/ Sakura 樱粉（#C44569）
  - 每套都核对 WCAG AA 对比度；shadow 跟随色温调整避免硬黑投影
- **主题选择器升级为色板预览** —— 设置页里 `<select>` 下拉换成 2×4 网格，每个 swatch 实时画出 surface + accent + 文字深浅的迷你卡片，可视化选择

### 修复

- **App 图标重画为暖琥珀色调** —— 旧图标是接近纯黑的深蓝底色，在 macOS 暗色 dock 里几乎看不出形状。新图标米黄圆角矩形底 + 三色原子轨道 + 中央眼瞳，48 个平台变体（icon.icns / .ico / Square*Logo / iOS / Android mipmap）全部刷新
- **`agentgate-serve` 无头 binary 编译失败** —— 1.1.0 内部 API 漂移期间 `server::start()` 返回值变 4 元组、`CreateProviderInput` 新加 `model_capabilities` 字段，CLI binary 没跟上。导致 Docker preflight 挂掉，v1.1.0 tag 已重新指向修复 commit

---

## [1.1.0] - 2026-05-27

This release lands the full Xiaomi MiMo integration, brings the Codex.app
IDE plugin back to life under AgentGate (the "hijack OpenAI provider +
`requires_openai_auth`" pattern), adds session affinity / SSE bootstrap
detection / cache-token observability, and consolidates the dashboard
into a tighter 5-block layout. The gateway also now decodes gzip / zstd /
brotli request bodies so modern HTTP clients (Codex.app, ChatGPT desktop)
don't 500 on us.

### Added / 新增

- **Codex.app IDE plugin compatibility / Codex.app IDE 插件兼容**
  - 改用「劫持 OpenAI provider + `requires_openai_auth = true`」配置写法：`model_provider = "OpenAI"` + `[model_providers.OpenAI]` 把 `base_url` 指向本地网关。Codex.app 把它当成官方 provider，IDE 插件 / Browser / Computer-Use / Mobile / 配额查询全部可用，同时对话请求实际走 AgentGate。
  - `auth.json` 完全不再被改写 —— ChatGPT OAuth tokens / `auth_mode = "chatgpt"` 全程保留
  - 旧版本污染 `auth.json` 的安全网：检测到 `{"OPENAI_API_KEY":"ag_local_...", no tokens}` 时自动从备份恢复
  - 工具卡上新增「原生模式 / 代理模式」状态说明 + 切换按钮
- **Compressed request body support / 压缩请求体支持**
  - 网关 4 个 POST handler（`/v1/responses` / `/v1/chat/completions` / `/v1/messages` / `/v1beta/...`）改用 `Bytes` extractor + `body_decode` 模块
  - 支持 `gzip` / `x-gzip` / `deflate` / `br` / `zstd` / `identity`，链式 encoding 与 quality factor 都能解析
  - 不再因 Codex.app 默认 zstd 压缩或 ChatGPT 桌面客户端默认 gzip 而 500 报 "invalid utf-8 sequence"
- **Session affinity / 会话亲和**
  - 上游回 `cached_tokens > 0` 时记录 `(session_id → provider_id)` 1 小时绑定
  - 同会话后续请求优先复用同 provider，最大化 prompt cache 命中
  - Session ID 是 `first_user_message + sorted_tool_names` 的稳定指纹，跨多轮不变
  - 识别四种 cache 字段格式：Anthropic / OpenAI Responses / Chat Completions / 中国厂商 bare 字段
- **SSE bootstrap detection / SSE 流首段保护**
  - 上游 HTTP 200 但首段 SSE 帧里塞 `event: error` 或 `data: {"error":...}` 时（quota / ban / rate-limit），首 16KB 窗口内捕获并失败转移到下一候选 provider
  - 客户端看到的是「另一个 provider 的正常流」，零感知
  - 覆盖 Chat Completions / Anthropic / Gemini / 纯 pass-through 4 条流式路径
- **Cache token Write / Read split / 缓存 token 拆分**
  - `request_logs` 表加 `cache_write_tokens` + `cache_read_tokens` 两列
  - Dashboard 今日条下方新增「缓存」inline footer，命中率自动计算
  - 让 session_affinity 的省钱效果可视化
- **Dashboard date-range tabs / 时段切换**
  - 今天 / 7天 / 14天 / 30天 tab 切趋势图
  - 30 天密集视图自适应：bar 宽度收窄、label 间隔抽稀、tokens 副标签隐藏
  - 后端 `get_stats_for_range(days)` 支持任意窗口
- **Runtime KPI footer / 全局 KPI 页脚**
  - Dashboard 底部新增 6 项实时指标：活跃连接 / 运行时间 / 累计请求 / 累计 Tokens / 累计费用 / 累计成功率
  - 5s 轮询，counter 全来自网关内存 + 一次 COUNT 查询，几乎零开销
- **Tray menu live status / 托盘菜单实时状态**
  - macOS 菜单栏图标点开显示：当前 active provider / 今日请求数 / 网关运行端口
  - 「切换服务商」子菜单一键切换 active provider，✓ 标记当前选中
  - 30 秒周期自刷新，gateway start / stop / 切换 active 时即时更新
- **Logs pagination / 日志分页**
  - 每页 100 条，分页栏显示「第 N / M 页 · 显示 a–b 条」
  - 关键词 / 状态过滤切换时自动回到第一页
  - 新 `count_request_logs` Tauri 命令共享 list 的 WHERE 子句，filter 语义一致
- **mimo2codex 转换层移植 / Conversion layer port**
  - encrypted_content 跨轮传递（出站 `response.output_item.done` 带 reasoning item，Codex 回传时还原 `reasoning_content`，跨进程重启不丢思考链）
  - Orphan tool_call 自动占位补全（避免上游 400 "missing tool message"）
  - Web search annotations 流式透传（`response.output_text.annotation.added` 事件 + 最终 done 内嵌 citations）
  - Context overflow 统一文案（trait 默认实现 + MimoProvider override）
  - Tool args delta 已存在的细粒度推流验证

- **Xiaomi MiMo as first-class provider / 小米 MiMo provider 一等公民支持**
  - 完整 5 个聊天模型：`mimo-v2.5-pro` / `mimo-v2-pro` / `mimo-v2.5` / `mimo-v2-omni` / `mimo-v2-flash`
  - 多轮 `reasoning_content` 回填，避免思考模式下 tool_calls 跨轮丢失 reasoning 触发 400
  - `mimo-v2.5-pro` / `mimo-v2.5` 思考态自动剥 `temperature`（上游强制 1.0）
  - `mimo-v2-omni` 自动剥 `web_search` builtin（该模型不支持联网搜索）
  - `tool_choice` 非 `auto` 值客户端剥除
  - 友好错误提示：`webSearchEnabled is false` 400 → 提示用户去开通 Web Search Plugin
  - Token-Plan host 自动切换：粘贴 `tp-*` key 时自动改用 `token-plan-cn.xiaomimimo.com`
  - 内置 MiMo 海外定价（v2.5-pro / v2.5 / v2-pro / v2-omni / v2-flash + TTS）
- **Per-model capability matrix / 模型能力矩阵**
  - 新增 `model_capabilities` JSON 字段：`{"model": ["text", "vision", "reasoning", ...]}`
  - 8 种能力：text / vision / audio_in / tts / video_in / reasoning / tools / web_search
  - Capability-aware promotion：请求带图自动 swap 到支持 vision 的模型（带图请求路由到 `mimo-v2.5` 而非 `mimo-v2.5-pro`）
  - Promotion 排序：选保留最多原模型能力的候选（v2.5 比 v2-omni 保留 reasoning+web_search 优先）
  - Matrix 也驱动 web_search 是否发送：用户在矩阵取消勾选即停止给该 model 发送 web_search builtin
  - Provider 表单加能力矩阵编辑器，"自动识别"按钮按 model 名规则种子填充（MiMo / DeepSeek / Kimi / Moonshot / 通用 fallback）
  - 测试按钮升级：连通性 + 自动识别合并填充缺失行（保留用户手动编辑）
  - Provider 卡片 / Routes 页改用图标行（👁🎤🔊🎞🧠🌐）替代单一 `supports_vision` 徽章
- **`[1m]` suffix auto-injection / 长上下文后缀自动注入**
  - Claude Code 透传路径上对 MiMo 和 DeepSeek 的 1M-context 模型自动追加 `[1m]`
  - Codex（OpenAI）路径完全不动 —— 用户配置 `mimo-v2.5-pro`，CC 端拿到 1M 上下文，Codex 端正常 128K
  - 已写 `[1m]` 的用户配置 / 不支持 1M 的模型（Flash / Omni）原样保留
- **Codex `web_search_preview` → MiMo `web_search` 翻译**
  - Codex 用户的联网搜索能力穿透到 MiMo 上游
  - 受 `model_capabilities` 矩阵控制：模型行未勾 web_search 则不翻译，避免上游 400
- **Accurate client labels in logs / 日志精确客户端识别**
  - User-Agent 推断客户端：Codex / Claude Code / OpenCode / AtomCode / Kimi CLI / Cursor / Cherry Studio / Continue / Cline / Roo Code / Python SDK / Node SDK / curl 等
  - 未匹配的 UA 显示其第一个 token，便于发现新客户端
  - 之前所有 `/v1/chat/completions` 透传路由都被打成 "Codex"，现在按路由 + UA 准确归类
- **Dashboard redesign / 概览页重布**
  - 9 张卡（含冗余数学如 Total = In + Out）→ 顶部一行"今日 strip"5 个核心指标
  - 7 天图表重做：单粗柱代替双窄柱、左侧 y 轴 0 / peak/2 / peak 三刻度对齐网格、错误堆栈在柱底红色、hover 显示明细 tooltip
  - Top providers 加横向 bar ranking，过滤 `unknown` 伪 provider
  - 累计 / 工具状态收为单行
- **Error suggestion display / 错误建议展示**
  - `enhance_error` 钩子接入 `ProviderTransform`，错误日志详情多一个绿色"建议"卡，actionable 提示与原始错误分离

### Changed / 改动

- **Connection stability / 连接稳定性**
  - HTTP client 加 `pool_idle_timeout(30s)` + `tcp_keepalive(20s)`：避免静默后 keep-alive 池递死连接导致 `PASS_THROUGH_REQUEST_FAILED`
  - 网络层 transient 错误（connect / timeout / request）自动重试 1 次，带 backoff，覆盖 pass_through 4 处和 adapter 6 处 send 调用
- **Vision probe accuracy / Vision 探针修正**
  - 旧逻辑只把 400 视为"不支持"，但 MiMo 返回 404，被误判为支持
  - 新逻辑：任何 4xx 都视为不支持，401/403/5xx 标为不确定不写库
- **Provider card layout / Provider 卡片重布**
  - 字符级换行修复（StatusBadge 加 `whitespace-nowrap`）、底层布局加 `min-w-0` + `truncate`，长 URL 不再压扁右侧 badge
  - 单行紧凑布局（icon · 名称 · 能力图标行）替代 8 字段网格，"详情"折叠展示协议 / 推理模型等次要字段
- **Pricing improvements / 定价改进**
  - 模型 ID 带 `[1m]` 等 qualifier 时自动 fallback 到 base-id 价格匹配
- **Capability icons unified component / 统一能力图标组件**
  - 新 `CapabilityIcons` 在 Providers 和 Routes 页共享，behavior 一致

### Fixed / 修复

- **MiMo / Codex 多轮历史图片穿透**：Codex 每轮回放整段会话，若早期一轮带图，后续即使纯文字也带 `image_url` → MiMo 路由到图片 endpoint 报 404。修复：`request_contains_images` 改扫全历史触发 capability promotion；MiMo `finalize_request` 对非 vision 模型剥除历史 `image_url`
- **DeepSeek API key 识别**：`sk-` + 32 位小写 hex 是 DeepSeek key 的精确形态，但旧 prefix-only 检测把它错认为 OpenAI。集中到 `lib/keyDetection.ts` 统一 18 条规则
- **Dashboard 30 天图溢出**：bar gap / 宽度 / label 密度按 bar 数量动态调节；趋势图永久全宽，热门 provider 改横向 inline 条
- **Dashboard 标题字符串撞名**：i18n 字典里写死的「每日请求（7 天）」叠加动态后缀变成「每日请求（7 天）· 30 天」，改成纯「每日请求」
- **路由页冗余 KPI footer**：Dashboard 已经有 KPI footer，Routes 页删除以避免重复
- **网关页布局碎片化**：3 个过度装饰卡 → 状态条 / 配置 / 路由参考 3 个精简块；删除与 Topbar 重复的 host:port + 运行中徽章
- 日志硬编码"Codex"/"/v1/responses"导致 Claude Code 和 chat completions 透传错误归类
- Vision 探针对 MiMo 等返回非-400 错误码的上游误报"支持视觉"

### Tests / 测试

- Rust 单元测试 508 → **581 全绿**（+73 covering body_decode / session_affinity / sse_bootstrap / codex hijack-OpenAI / cache token extraction / range stats / tray tooltip / image detection 等）
- Vitest 56 → **75**（+19 covering keyDetection + 既有覆盖）
- 集成 smoke test `release_preflight_smoke` 扩展：新增 responses_strict / anthropic_messages / multi_turn 三段，10/10 通过真实 MiMo / DeepSeek 上游

---

## [1.0.0] - 2026-05-20

### Release / 发布

- 发布 AgentGate 1.0.0 正式版

---

## [0.8.4] - 2026-05-20

### Fixes / 修复

- 修复桌面宠物恢复到屏幕外坐标时看起来没有显示的问题
- 修复通过设置页或托盘重新显示宠物时不会自动拉回可见区域的问题
- 修复 release 合并版 `latest.json` 发布脚本在解析 changelog 版本标题时报正则错误的问题

---

## [0.8.3] - 2026-05-19

### Fixes / 修复

- 修复前端缺少 `updater:default` capability 导致检查更新可能被拦截的问题
- 修复 macOS release 没有生成 app updater 包，导致 `latest.json` 缺少 `darwin-*` 平台的问题

### CI / 发布

- release 流程改为构建完成后统一发布合并版 `latest.json`，避免多平台 matrix 互相覆盖

---

## [0.8.2] - 2026-05-19

### Fixes / 修复

- 修复 `agentgate-serve` 未适配网关启动返回值导致 CLI 多平台构建失败的问题
- 修复 Docker/默认 CLI 启动未读取 `AGENTGATE_HOST` 和 `AGENTGATE_PORT` 的问题

### CI / 发布

- 新增 Docker release preflight，发版前构建并启动 `agentgate-serve`，检查 `/health`
- 修复 Dockerfile 的 Rust 版本、Linux/Tauri 依赖和构建上下文体积问题

---

## [0.8.1] - 2026-05-19

### Fixes / 修复

- 修复首次安装后概览页在空请求日志表上计算统计时报数据库错误的问题
- 修复 Provider 健康统计在没有请求日志时可能报数据库错误的问题

---

## [0.5.10] - 2026-05-19

### CI / 发布

- 隔离各平台 CLI 构建输出目录，避免 macOS 交叉编译时复用错误架构的 build script 缓存

---

## [0.5.9] - 2026-05-19

### Fixes / 修复

- 移除启动时自动插入的示例请求日志，并清理旧版 `req-seed-*` 假数据
- 修复 macOS 从 Dock/Finder 启动时托盘菜单没有识别中文系统语言的问题

---

## [0.5.8] - 2026-05-18

### CI / 发布

- 修复独立 Linux CLI 产物构建缺少 GTK/GDK 系统依赖的问题

---

## [0.5.7] - 2026-05-18

### CI / 发布

- 增加独立 `agentgate-serve` CLI 发布产物，GUI 安装包继续只包含桌面应用

---

## [0.5.6] - 2026-05-18

### CI / 发布

- 将 headless CLI 源文件移出 Cargo/Tauri 自动发现目录，避免 Linux AppImage/deb 打包寻找未构建的 `serve` 二进制

---

## [0.5.5] - 2026-05-18

### CI / 发布

- macOS Release 仅打包 GUI 主程序，不再自动发现和打包 headless CLI 二进制

---

## [0.5.4] - 2026-05-18

### CI / 发布

- 重新生成 Tauri updater 签名密钥并更新内置 public key

---

## [0.5.3] - 2026-05-18

### CI / 发布

- 修复 macOS x86_64 Release 中 `agentgate-serve` 子程序未签名导致 `.app` 签名失败的问题

---

## [0.5.2] - 2026-05-18

### CI / 发布

- 修复 macOS notarization 使用 App Store Connect API key 文件路径的问题

---

## [0.5.1] - 2026-05-18

### CI / 发布

- 修复 macOS Release 使用 Developer ID 证书签名和 notarization 的配置
- 更新 GitHub Actions 到 Node.js 24 runtime，消除 Node.js 20 deprecation 警告

---

## [0.5.0] - 2026-05-18

### Features / 功能

- **Gemini 原生格式支持** — Codex → Gemini API 转换（contents/functionCall/functionResponse/generationConfig），Gemini CLI → Chat Completions 双向转换，Gemini CLI 可通过 AgentGate 连接 DeepSeek/Kimi 等任意 Provider
- **任务级智能路由** — 按请求特征（输入长度、图片、工具、系统关键词）自动选 Provider/模型，预设场景（图片/推理/后台/长文本/工具密集），支持多选组合
- **Headless 服务模式** — `agentgate-serve` CLI 二进制，完整子命令管理（provider-add/list/remove、token、status、serve），Docker 部署支持，环境变量配置
- **日志时间显示日期** — MM-DD HH:MM:SS 格式
- **Gemini 模型映射建议** — gemini-2.5-flash、gemini-2.5-pro、gemini-3-pro-preview

### Tests / 测试

- 单元测试 289 个，集成测试 43 个（含 Gemini 入口 5 个 + CLI headless 7 个）
- Gemini 转换：responses_to_gemini 7 个 + gemini_to_chat 6 个

---

## [0.4.0] - 2026-05-18

### Features / 功能

- **费用追踪** — 22 个内置模型价格，每条请求自动计算费用（token × 价格），仪表盘展示总费用/今日费用/平均费用，设置页价格管理表格（内联编辑 + 自定义覆盖）
- **多账号轮转** — 同一 Provider 支持多个 API Key（JSON 数组存储），请求自动 round-robin 轮转，UI 多 Key 列表管理
- **Prompt Cache 注入** — 对 Codex → Anthropic 转换路径自动注入 `cache_control: {type: "ephemeral"}`（system 末尾 + tools 末尾 + 最后 assistant 消息），Provider 配置开关（推荐/实验性标签）
- **Cache 能力自动探测** — 测试连接时发 2 次相同请求检查 `cache_read_input_tokens > 0`，Provider 卡片显示"支持缓存/不支持缓存"
- **Provider 健康面板** — 卡片内嵌 1h/24h 成功率（绿/黄/红圆点）、平均延迟、P95 延迟、请求数
- **请求重试** — 429/500/502/503 自动退避重试（1s→2s，最多 2 次），尊重 Retry-After，每次重试自动换 Key
- **macOS/Windows 安装指引** — 可折叠分步指引（3 种 macOS 方式 + Windows SmartScreen）

### Bug Fixes / 修复

- **修复费用不计算** — Provider 名大小写不敏感匹配 + 启动时自动回填历史请求费用
- **修复 kimi-for-coding 价格缺失** — 加入内置默认
- **修复 Gemini CLI/AtomCode 硬编码文案** — 全部改用 i18n 双语

### Tests / 测试

- 单元测试 268 个（pricing 7, multi-key 5, cache 6, retry 2, 其他 248）
- 集成测试 31 个：深度验证转换格式、DB 日志、token/cost 记录、SSE 事件生命周期、pass-through、pricing 表

---

## [0.3.0] - 2026-05-18

### Features / 功能

- **23 个 Provider 预设**（原 7 个）— 新增 Google Gemini、xAI、Mistral、Groq、Together、Fireworks、Cerebras、Perplexity、Cohere、智谱 GLM、通义千问、硅基流动、火山引擎、百川、阶跃星辰、零一万物，选择类型自动填充 Base URL 和默认模型
- **Gemini CLI 客户端支持** — 写入 `~/.gemini/.env`（GEMINI_API_KEY + GOOGLE_GEMINI_BASE_URL）+ `settings.json`（model + auth type），支持一键切换官方/AgentGate
- **AtomCode 客户端支持** — 写入 `~/.atomcode/config.toml`（default_provider + providers 段），支持一键切换
- **客户端总计 6 个**：Codex、Claude Code、OpenCode、Gemini CLI、AtomCode（新增 2 个）
- **README SEO 重做** — 徽章（版本/Stars/下载量/License）、价值主张重写、快速导航链接、GitHub Topics 17 个、repo description 更新

### Bug Fixes / 修复

- **修复 Anthropic SSE 流断开不通知客户端** — 流错误、Claude API 错误、空内容流结束三种场景均发送 `response.failed` 事件
- **修复 SSE 帧解析** — 兼容 `\r\n\r\n` 分帧和 `event:X`（无空格）格式
- **修复识图后请求不切回原 Provider** — Vision 感知路由只检查最后一条用户消息
- **修复 Gemini CLI 配置不生效** — 环境变量写入 `.env` 文件（非 settings.json 的 env 字段）；auth type 设为 `gemini-api-key`（OAuth 模式忽略 .env）
- **修复 AtomCode 报"未配置活跃 Provider"** — 加入 `default_provider` 顶层字段
- **修复工具参数静默替换无日志** — 无效 JSON 参数替换为 `{}` 时打印警告日志
- **统一 URL 构建逻辑** — `adapter.rs` 和 `route_decision.rs` 共用 `smart_append_path()`

---

## [0.2.2] - 2026-05-18

### Features / 功能

- **23 个 Provider 预设**（原 7 个）— 新增 Google Gemini、xAI (Grok)、Mistral AI、Groq、Together AI、Fireworks AI、Cerebras、Perplexity、Cohere、智谱 GLM、通义千问、硅基流动、火山引擎、百川、阶跃星辰、零一万物，选择类型自动填充 Base URL 和默认模型

### Bug Fixes / 修复

- **修复 Anthropic SSE 流断开不通知客户端** — 流错误、Claude API 错误、空内容流结束三种场景均发送 `response.failed` 事件
- **修复 SSE 帧解析** — 兼容 `\r\n\r\n` 分帧和 `event:X`（无空格）格式
- **修复工具参数静默替换无日志** — 无效 JSON 参数替换为 `{}` 时打印警告日志
- **统一 URL 构建逻辑** — `adapter.rs` 和 `route_decision.rs` 共用 `smart_append_path()`，消除重复代码

---

## [0.2.1] - 2026-05-18

### Bug Fixes / 修复

- **修复识图后请求不切回原 Provider** — Vision 感知路由只检查最后一条用户消息是否含图片，不再扫描历史消息，修复识图请求后后续纯文本请求一直走 KimiCoding 的问题

---

## [0.2.0] - 2026-05-15

### Features / 功能

- **模型映射下拉优化** — 映射目标模型和客户端模型选择统一使用自定义 ModelCombo 组件，支持下拉选择 + 手动输入
- **macOS ad-hoc 签名** — CI 构建 macOS 包时自动 ad-hoc 签名，不再提示"应用已损坏"
- **macOS 自动更新支持** — `createUpdaterArtifacts` 改为 `true`，生成 `.app.tar.gz` 更新包

### Bug Fixes / 修复

- **修复 ModelCombo 下拉只显示一个模型** — 无过滤词时显示全部模型
- **修复深色主题下拉不可见** — 下拉背景、边框、高亮色调整，与深色主题统一

---

## [0.1.9] - 2026-05-15

### Features / 功能

- **Claude Messages API 原生支持** — 当 `provider_type` 为 `anthropic` 时，Codex 请求自动转换为 Claude 原生 Messages API 格式，完整支持 `tool_use`/`tool_result`、`input_schema`、`thinking.budget_tokens`，专用 Claude SSE 处理器
- **URL 驱动的路由机制** — 新增 `responses_base_url` 字段，填写后 Codex 请求直接透传到上游 Responses API 端点；路由逻辑基于 URL 字段判断：`responses_base_url` 有值 → 透传，`provider_type` 为 anthropic → Claude 转换，其他 → Chat Completions 转换
- **智能 URL 拼接** — 自动识别 `/messages`、`/responses` 后缀，填完整 URL 或 base URL 均可
- **Provider 模块化重构** — 将 DeepSeek/Kimi/MiniMax/Anthropic 专有逻辑拆分为独立模块，`ProviderTransform` trait 实现可扩展的 provider 专有处理
- **`local_shell` 工具转换** — Codex 内置 `local_shell` 工具转换为标准 `shell` function tool
- **Tool output 数组展平** — `ContentPart[]` 数组提取文本、图片替换为占位说明
- **Provider 类型新增** — 下拉新增 `Anthropic (Claude)` 和 `MiniMax`
- **协议标签显示** — Provider 卡片协议改为友好标签（替代原始 JSON 字符串）
- **Responses API 端点配置** — Provider 高级设置新增输入框，Provider 预设 OpenAI 自动填充

### Bug Fixes / 修复

- **修复 DeepSeek 发送无意义 `thinking` 字段** — 移除 MiMo 专属的 `thinking: {type: "disabled"}`，DeepSeek 直接忽略该字段
- **修复非流式空 choices 导致 Codex 挂起** — upstream 返回 `choices: []` 时生成占位消息
- **修复 Tool call ID 超长** — 截断至 64 字符（Responses API 规范限制）
- **修复 `anthropic_base_url` 误触发 Claude 转换** — `/v1/responses` 路由仅在 `provider_type` 为 anthropic 时走 Claude 转换，`anthropic_base_url` 仅影响 `/v1/messages` 透传
- **修复 pass-through 对非 OpenAI provider 返回 404** — 移除自动 pass-through 检测，改为显式 URL 字段控制

### Docs / 文档

- README（中英文）更新：Claude 原生转换说明、`responses_base_url` 配置、Provider 转换方式与专属处理、数据链路触发条件

---

## [0.1.8] - 2026-05-14

### Features / 功能

- **Provider 模块化重构** — 将 DeepSeek/Kimi/MiniMax 逻辑拆分到 `transform/providers/` 目录，`ProviderTransform` trait 分发
- **`local_shell` 工具转换** — Codex 内置工具转换为标准 function tool
- **Tool output 数组展平** — 提取文本部分，图片替换为占位说明

---

## [0.1.7] - 2026-05-14

### Bug Fixes / 修复

- **修复 macOS 关闭窗口后点 Dock 图标无法重新打开** — 使用 `RunEvent::Reopen` 监听 Dock 点击，重新显示并聚焦窗口

---

## [0.1.6] - 2025-05-14

### Bug Fixes / 修复

- **修复 SSE 流式文本空白被误删导致 markdown 不渲染** — `split_think_tags` 对无 `<think>` 标签的 delta chunk 不再 trim，保留换行和空格，修复 Codex 桌面端 markdown 表格/标题显示为原始文本的问题

### Tests / 测试

- 新增 6 个 `split_think_tags` 空白保留测试，覆盖 markdown 表格、标题、前后换行等关键场景（193 → 199）

### Docs / 文档

- 更新 providers、logs 截图

---

## [0.1.5] - 2025-05-14

### Bug Fixes / 修复

- **修复中文内容导致网关 panic** — 所有字符串截断函数（`truncate_str`、`truncate`）改用 `is_char_boundary` 安全截断，消除多字节字符边界 panic
- **修复 tool output 截断导致 Codex 崩溃** — 移除网关层 4000 字节截断限制，tool output 原样透传给上游模型
- **修复错误响应格式** — 错误 JSON 添加 `type` 字段，符合 OpenAI API 规范，客户端不再显示 "Unknown error"
- **修复 Auth 返回 500** — `GATEWAY_AUTH_MISSING` / `GATEWAY_AUTH_INVALID` 正确返回 401
- **修复 Mutex 中毒后日志永久丢失** — 所有 DB 锁操作改用 `lock_db()` 恢复 poisoned Mutex
- **修复 SSE 事件日志溢出** — `events_size` 严格守住 1MB 上限
- **修复 `sanitize_body` 重复扫描** — `sk-****` 替换后正确跳过已处理内容
- **修复 `split_think_tags` 只处理首个块** — 改为循环提取所有 `<think>` 块
- **修复 Provider 删除不级联** — 删除 Provider 时同步清理 route_profile_providers，消除孤儿数据
- **修复 `reasoning_store` 哈希碰撞风险** — 改用双哈希 + 长度作为 key
- **修复 Settings 页 `installing` 状态声明顺序** — 消除 temporal dead zone 错误
- **修复 Dashboard 轮询无取消守卫** — 组件卸载后不再写状态
- **修复 Gateway 端口无校验** — 保存前验证端口范围 1-65535
- **修复 ConfirmDialog "Cancel" 硬编码英文** — 改用 i18n

### Performance / 性能

- **`get_stats` 查询优化** — 从 14 条独立 SQL 合并为 3 条（1 聚合 + 1 GROUP BY + 1 Top），减少锁持有时间
- **添加 `request_logs.timestamp` 索引** — 加速按时间的统计查询

### Improvements / 优化

- **自检逻辑优化** — 未配置 AgentGate 的客户端（Codex/Claude Code）跳过检查，不再报 warning
- **API 密钥显示优化** — 遮罩改为固定长度 `sk-1****cdef`，不再撑满整行
- **日志页添加刷新按钮**
- **`formatTimestamp` 支持 locale 参数** — 中文环境使用中文格式
- **Routes/ProviderForm 改用受控 select** — 替换 `document.getElementById` 反模式

### Code Quality / 代码质量

- 清理 35+ 处死代码（`select`、`ConfigBackup`、`ResponsesResponse`、`ProviderAttempt` 等）
- 编译零 warning、零 error
- 新增 23 个单元测试（170 → 193），覆盖所有核心修复
- 新增集成测试脚本 `scripts/test-integration.sh`（12 项端到端测试）

---

## [0.1.4] - 2025-05-14

### Features / 功能

- **Claude Code 一键切换** — 与 Codex 相同的 save/restore 机制，保存原始 settings.json，切换到官方时自动恢复
- **OpenCode 一键配置** — 新增 `tools/opencode.rs` 模块，写入 `~/.config/opencode/opencode.json`，修正配置文件路径
- **路由系统完善** — 按协议自动创建 3 个默认路由（Codex / Claude Code / OpenCode），新增服务商自动加入所有路由链
- **路由 UI 增强** — 新增「创建配置」按钮和表单，支持 inline 重命名，删除按钮始终可见
- **导航重构** — 侧边栏按用户操作流程重排：概览 → 服务商 → 路由 → 网关 → 客户端 → 日志 → 诊断 → 设置

### Improvements / 优化

- 「仪表盘」→「概览」，「工具」→「客户端」，命名更符合用户认知
- 移除路由中未使用的 `client_type` 和 `max_retries` 字段
- 侧边栏移除装饰性星标，「设为默认」仅在同协议多配置时显示
- 路由页选中状态在操作后保持不变（修复跳回第一个的 bug）
- 移除所有 Codex/Claude Code 备份列表 UI 和相关死代码
- 协议显示为可读标签（如 "OpenAI Responses (Codex)"）

### Docs / 文档

- 更新全部 6 张截图为最新 UI
- README 中英文同步更新：导航命名、使用指南顺序、新增 OpenCode 配置章节

---

## [0.1.3] - 2025-05-14

### Features / 功能

- **Codex 配置一键切换** — 新增「切换到官方 / 切换到 AgentGate」按钮，整体保存和恢复 `config.toml` + `auth.json`，切换后 Codex 立即生效
- **保留官方会话** — 应用 AgentGate 配置时自动保存原始 Codex 官方配置（含 OAuth tokens），切回官方后对话记录不丢失
- **多语言切换文案** — 切换按钮、状态提示、污染警告均支持中英双语
- **污染检测与警告** — 检测 auth.json 中 OPENAI_API_KEY 被旧版覆盖的情况，显示黄色警告提示用户修复

### Improvements / 优化

- Codex 配置 apply 改为全量替换（不再 TOML 合并），逻辑更简洁可靠
- 移除 Codex 备份历史列表，统一由切换机制管理配置恢复
- `generate_codex_config` 命令统一调用 `codex::generate_snippet()`，修复模型名不一致 bug（gpt-5 → gpt-5.5）

### Cleanup / 清理

- 移除未使用的 Codex 备份/恢复/预览命令（`backup_codex_config`、`list_codex_backups`、`restore_codex_backup`、`preview_codex_config`）
- 移除死代码：`update_toml_content`、`create_backup`、`ConfigPreview`、`BackupResult`（codex 模块）
- 移除前端死导出 `previewClaudeCodeConfig`、死类型 `ClaudeCodeConfigPreview`
- 移除未使用的 i18n key（`tools.preview`、`tools.refresh`）

---

## [0.1.2] - 2025-05-13

### Improvements / 优化

- 版本号统一从 Tauri API 运行时读取，不再硬编码
- 检查更新失败时静默处理，不再弹出错误提示
- Codex 默认模型更新为 `gpt-5.5`
- 工具页面新增备份历史列表和一键恢复功能

---

## [0.1.1] - 2025-05-13

### Features / 功能

- **应用图标** — 全新轨道中心 Logo，替换默认 Tauri 图标
- **应用内自动更新** — 集成 tauri-plugin-updater，启动时自动检查 GitHub Releases 新版本
- **设置页检查更新** — 关于区域新增「检查更新」按钮和 GitHub 链接
- **工具配置备份列表** — Codex 和 Claude Code 配置支持查看备份历史和一键恢复
- **CI/CD 自动发版** — GitHub Actions 多平台构建（macOS ARM/Intel、Windows、Linux），push tag 自动发布
- **中英双语 README** — README.md（中文）+ README_EN.md（英文），含截图和安装说明

### Bug Fixes / 修复

- 修复 Codex 配置默认模型从 `gpt-5` 更新为 `gpt-5.5`
- 修复 `.gitignore` 中 `logs` 规则误排除 `src/components/logs/` 导致 CI 构建失败
- 修复侧边栏 Logo 图标与新品牌风格不一致
- 精简关于页面，移除技术栈细节，只保留版本号和协议

---

## [0.1.0] - 2025-05-13

First public release.

### Features / 功能

- **协议转换网关** — 支持 OpenAI Responses API、Anthropic Messages API、Chat Completions 三种协议的统一转换与转发
- **多 Provider 管理** — 支持 DeepSeek、OpenAI、OpenRouter、Kimi 及自定义 OpenAI 兼容接口
- **Route Profile 路由配置** — 多 Provider 优先级链，支持手动切换与自动故障转移（failover）
- **DeepSeek 深度兼容** — reasoning_content 思考模式完整支持、SSE 规范兼容、5 项专项修复
- **Codex 完整适配** — 通过 codex-app-transfer 审计，关闭 12 项兼容性差距
- **Anthropic Messages API 原生透传** — 支持 Claude Code 直连原生 Anthropic 接口
- **工具一键配置** — Codex（config.toml + auth.json）、Claude Code（settings.json）、OpenCode 一键写入
- **模型映射** — 客户端模型名称到 Provider 模型名称的自定义映射
- **Provider 额外请求头** — 支持 Kimi web_search 等 Provider 特有功能
- **Reasoning effort 传递** — 支持 low/medium/high 推理强度透传，自动标准化 xhigh/max/auto/none 等变体
- **Token 用量统计** — 仪表盘展示输入/输出 Token、每日消耗图表
- **仪表盘自动刷新** — 每 5 秒自动更新状态数据
- **多轮对话** — previous_response_id session store 支持
- **本地令牌认证** — ag_local_* 令牌，自动生成，权限 0600
- **请求日志** — 完整记录原始请求、转换请求、上游响应，支持脱敏
- **诊断自检** — Gateway、Provider、配置、数据库全面检查，支持导出诊断包
- **系统托盘** — 关闭窗口后台运行，托盘菜单控制启停
- **开机自启** — auto_start 开关，app 启动时自动启动 Gateway
- **中英双语界面**
- **配置自动备份与恢复**

### Bug Fixes / 修复

- 修复 Provider 活跃状态在 providers/route_profiles/gateway_settings 间同步问题
- 修复 Messages 路由在无匹配 route profile 时回退到 openai_responses 的逻辑
- 修复 SSE 解析器不支持 `data:` 无空格格式（Kimi 兼容）
- 修复 Codex reasoning effort `xhigh` 映射为 `high`（Kimi 兼容）
- 修复 failover 选择器在 manual 模式下忽略 active_provider_id
- 修复 Tauri v2 command 参数需要 camelCase
- 修复 Provider 表单对话框内容溢出
- 修复日志计数文本不换行
- 修复 Token 统计在数据为 0 时不显示
