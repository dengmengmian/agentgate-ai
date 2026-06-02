# Changelog / 更新日志

All notable changes to this project will be documented in this file.

## [Unreleased]

围绕「网关精炼层」做一次纵深建设：在透明转发的基础上，按 provider 的实际兼容性约束做请求/响应改写，附带节点测速和模型降级链。**全部能力默认关闭**，开关全关时网关行为与之前完全一致；按需开启时 GUI 可观测到具体改写动作。

### 新增

- **网关精炼层 (Refiner Pipeline)** —— 三个新模块按 ROI 排列开关，默认全部关闭走透明 pass-through，避免破坏现有用户行为：
  - **请求字段过滤 (Body Filter)**：按每个 provider 的 quirks 表剥不支持的请求字段（如 DeepSeek 的 `web_search`、Kimi 的 `web_search_options`），避免常见 400 错误。
  - **推理参数校正 (Thinking Rectifier)**：`thinking.budget_tokens` 在 provider 接受范围内做 clamp（MiMo 1024-32768、Anthropic 1024-64000），`reasoning.effort` 在 provider 接受值集合外归一到 `medium`。
  - **错误响应归一 (Error Mapper)**：把 provider 错误结构改写成客户端协议期望的形态（Anthropic Messages / OpenAI / Gemini 三种 envelope 均支持），并支持每个 provider 配置 `error_code_overrides` 表（如 DeepSeek `insufficient_balance` → 通用 `insufficient_quota`）。
  - 内置 DeepSeek / MiMo / Anthropic / OpenAI / Kimi 默认 quirks 种子；用户可在 provider 编辑页填 `provider_quirks` JSON 覆写（叠加策略：用户列表追加到默认列表）。
- **节点测速** —— Providers 页加「测速」按钮，对所有启用 provider 并行发 `max_tokens=1` 探测请求，记录 connect / TTFB / total 三段延迟。用户手动触发（每次消耗少量 token），结果按延迟排序染色（< 500ms 绿 / 500-1500ms 黄 / > 1500ms 红）。
- **三态熔断器 (Circuit Breaker)** —— 基于现有 `provider_runtime_status` 表做 Closed / Open / HalfOpen 三态封装，方便后续 routes 接入与 GUI 展示。读路径无副作用，写路径仍走原有 `mark_failure / mark_success`。
- **模型降级链 (Degradation Chain)** —— provider 编辑页新增配置 `{"requested_model":["fallback1","fallback2"]}` 的能力，主模型失败时按链顺序尝试同 provider 内的其他模型（接入到 routes 失败重试逻辑是下一步）。
- **精炼日志 (RefinerLog)** —— `request_logs.trace_json` 新增 `body_filter` / `thinking_rectifier` / `error_mapper` / `circuit_breaker` / `degradation` 五个可选字段，每次改写都记录「from → to + reason」便于事后追溯。

### 数据库

- `providers` 表新增 5 列：`provider_quirks`(JSON) / `body_filter_enabled` / `thinking_rectifier_enabled` / `error_mapper_enabled` / `model_degradation_chain`(JSON)。
- 每个 refiner 开关 3 态：`NULL = 跟随全局总闸 / 0 = 强制关 / 1 = 强制开`。全局总闸为 master kill —— 全局关时任何 per-provider opt-in 都不生效。
- `gateway_settings` 表新增 3 列：`body_filter_global` / `thinking_rectifier_global` / `error_mapper_global`，默认全 `0`（关闭）。

### 工程

- 新增 6 个后端模块：`gateway/circuit_breaker.rs`、`gateway/refiner_log.rs`、`gateway/refiners/{body_filter,thinking_rectifier,error_mapper,runtime}.rs`、`diagnostics/speedtest.rs`。
- 新增 2 个 Tauri command：`provider_speedtest` / `provider_speedtest_all`。
- 单元测试 792 → 840（新增 48 个，覆盖 refiners 全部分支 + speedtest 探测形态 + circuit breaker 三态转换 + degradation chain 边界）。
- 前端 `ProviderFormDialog` 高级区新增「网关精炼层」小节（3 个三态开关 + Quirks JSON 编辑器 + 降级链 JSON 编辑器）；`Settings` 通用 tab 新增 3 个全局总闸；`Providers` 页新增「测速」按钮 + `SpeedtestDialog` 组件。

### 限制 (Known Limits)

- 当前 commit 完成「数据层 + 模块代码 + GUI 配置面板 + 测速」，**精炼层尚未接入到 `routes.rs` 请求路径**——开启开关后 GUI 能保存配置、能在 trace_json 结构里识别字段，但请求转发本身仍是字节级透明。这一步留到下一个 commit 单独做（涉及 6 个 handler 共 3000+ 行，单独成 commit 风险更小）。
- 模型降级链同理：配置可保存，失败重试逻辑接入留到下一个 commit。

## [1.3.0] - 2026-06-01

围绕「5 分钟跑通」上手体验做一轮闭环，同时把三层能力转换的回归测试基建立起来。

### 新增

- **测试连接结构化诊断** —— 失败时不再裸露 `HTTP 401: <body>`。后端把 13 种典型失败（invalid_key / insufficient_balance / rate_limited / web_search_plugin_disabled / model_not_found / region_blocked / endpoint_not_found / upstream_5xx / dns_failed / network_timeout / connection_refused / tls_error / unknown）分类成结构化诊断，每种一句话原因 + 一句话建议；11 家主流 provider 内置 keys / billing / plugin URL 表，UI 直接给「去重建 key」「查看账户余额」「去开通插件」等一键按钮，跳到对应控制台。
- **快速配置静默检测剪贴板里的 key** —— 进入 Quick Setup 时尝试读一次剪贴板，识别为已知前缀（`sk-ant-` / `tp-` / `xai-` / `gsk_` / `pplx-` / `sk-or-` / `deepseek-` / DeepSeek/Kimi 特征 `sk-`）就在输入框上方弹可关闭 banner，点「填入」直接带进表单 + 自动选好 provider type。剪贴板读不到 / capability 没批 / 内容不像 key → banner 完全不出现，跟没这个功能时一致，不阻断现有流程。
- **应用客户端配置后检测进程** —— Codex / Claude Code / OpenCode / Gemini CLI / AtomCode 任一点「应用配置」成功后，新增 PostApplyDialog 显示配置 path + 进程状态：在跑就给 PID 列表 + 一键复制 `kill <pid>` + 警告（终端会话会被中断、Claude Code 可以 `--resume` 恢复）；未跑就直接告知「下次启动自动生效」。从不自动 kill，只复制命令到剪贴板让用户自己执行。

### 修复

- **`test_provider` 没转发 `extra_headers`** —— Kimi catalog 默认注入 `User-Agent: KimiCLI/1.40.0`，Moonshot 服务端对部分 plan key 做 UA 校验，UA 不对一律 401，看起来像 key 错了。现在测试连接和能力探测路径都正确转发 provider 自定义 header。
- **`test_provider` 没解析多 key 字段** —— `["sk-a","sk-b"]` 这种 JSON 数组形式之前直接当字面量拼进 `Bearer`，必 401。现在和 `detect_provider_cache` 一致地取首个非空 key。
- **编辑 provider 时多 key 不可见** —— 表单总是显示 1 个空槽且 placeholder 只是掩码摘要，用户不知道现有几把 key、改的是哪一把。新增 `get_provider_keys` command 编辑时异步明文回填到每个槽，多 key provider 也能精确改谁改谁。

### 工程

- **三层能力转换离线冒烟测试** —— 新增 5 个 `src-tauri/tests/*_fixture.rs` test binary（mock 上游 + 隔离 in-memory SQLite + 真实 axum 网关），覆盖：
  - **L1 协议转换**：Responses↔Anthropic / Chat↔Anthropic / Messages→Chat fallback
  - **L2 模型映射**：`/v1/responses` `/v1/chat/completions` `/v1/messages` 三端点 + `agentgate` 虚拟模型解析
  - **L3 能力矩阵**：MiMo vision promotion / 图片 strip + notice / `web_search` PAYG 自动降级重试 / thinking-mode reasoning_content 占位；DeepSeek 图片 strip / `reasoning_content` 端到端透传 / Claude 直连 `[1m]` 后缀剥离；Kimi `$web_search` builtin 改写 + 同时 disable thinking / 多轮 tool_call ↔ tool_result 闭环
- **CI 接入**：新增 `.github/workflows/ci.yml`，PR + main push 自动跑全部 fixture，发现回归立即阻断 merge；不依赖任何 key 或外网请求。`release.yml` preflight 也加跑一遍，发版前再卡一道。Ubuntu runner 顺手装齐 `libwebkit2gtk-4.1-dev` 等系统依赖供 tauri 链接。
- **一键 smoke 脚本**：新增 `scripts/release-smoke.sh`，离线 fixture 默认跑；`AG_RUN_SMOKE_TESTS=1` 时把真实 provider smoke（从本地 SQLite 取 key，永远只在开发机上跑）一起跑。
- **dev-dependency**：`wiremock = "0.6"` + `tempfile = "3"`，只在 `cargo test` 时编译。
- **新增运行时依赖**：`tauri-plugin-clipboard-manager`（剪贴板检测 + 复制 kill 命令用）。

## [1.2.4] - 2026-05-30

### 修复

- **AtomCode / OpenCode 改用 AgentGate 虚拟模型** —— 一键配置不再把当前 provider 的真实模型名固化进客户端。AtomCode 写入 `agentgate`，OpenCode 写入 `openai/agentgate`；网关在请求时把虚拟模型解析成本次智能路由选中的真实模型，避免切换到 DeepSeek 后仍透传 `mimo-v2.5` 这类旧模型名导致 400。
- **原生直通保持透明语义** —— 普通真实模型名仍按原规则处理：命中 Model Mapping 才改写，未命中就原样透传；只有 `agentgate` / `openai/agentgate` 这两个虚拟模型走路由解析。

### 文档

- README / README_ZH 补充 `agentgate` 虚拟模型规则，并强化 5 分钟上手流程、客户端配置说明和常见连接问题排查。

## [1.2.3] - 2026-05-30

### 修复

- **MiMo / DeepSeek Provider 兼容性收敛** —— DeepSeek 默认模型和能力识别收敛到 `deepseek-v4-flash` / `deepseek-v4-pro`，不再把即将下线的 `deepseek-chat` / `deepseek-reasoner` 作为自动配置目标。
- **MiMo Token Plan 区域域名保持一致** —— `sk-*` 按量付费 key 使用开放 API 域名；`tp-*` Token Plan key 使用 `token-plan-{cn|sgp|ams}.xiaomimimo.com`，并在 Chat / Anthropic 端点间保持同一区域，避免 key 与 host 不匹配。
- **MiMo `web_search` 自动降级** —— Token Plan 预先剥离 MiMo 原生 `web_search` builtin；按量付费 key 遇到 Web Search Plugin 未开通错误时剥离并重试一次，避免新手未购买插件时请求直接失败。
- **Claude Code 直连模型不再写 `[1m]`** —— MiMo / DeepSeek 的推荐映射继续使用普通模型 ID，历史 `[1m]` 后缀在 Anthropic 直连发送前会被剥离。
- **能力降级可诊断** —— MiMo / DeepSeek 图片剥离、MiMo `web_search` 自动降级、MCP advisory、tool output 图片省略会统一记录为 `degradation_events`，写入请求日志 trace，便于后续 UI 展示和排查。
- **桌面包不再包含 headless CLI** —— `agentgate-serve` 改为 CLI feature 构建，只在 Docker / 独立 CLI 发布产物中编译，避免 macOS 桌面包签名时把未签名 CLI 当成 app 子程序。
- **Windows 发版检查兼容 CRLF** —— Provider catalog 生成检查对行尾做归一化，避免 Windows runner 将 LF checkout 成 CRLF 后误报 generated catalog 过期。

### 文档

- README / README_ZH 同步说明 MiMo 开放 API 与 Token Plan 域名差异、Token Plan 区域保持、`web_search` 自动降级行为，以及请求日志中的能力降级诊断事件。

## [1.2.2] - 2026-05-29

### 修复

- **Windows 桌面宠物白色背景** —— Tauri 的 WebView2 控件在 Windows 上默认底色为白色，即使设置了 `transparent(true)` + CSS `background: transparent !important` 也无法让控件本身透明，导致宠物窗口在 Windows 上显示为一个白色卡片。给 pet 窗口 builder 加上 `.background_color(Color(0, 0, 0, 0))`，让 WebView2 底色也走透明通道。macOS 行为不变。

## [1.2.1] - 2026-05-29

### 修复

- **MiMo / DeepSeek 不再自动配置 `[1m]` 模型** —— Claude Code 推荐映射默认使用普通模型 ID，历史自动生成的 `[1m]` 映射会在后续保存 / 应用配置时修正；Anthropic 直连发送前也会剥离 MiMo / DeepSeek 的旧 `[1m]` 后缀，避免上游返回 `Not supported model`。
- **快速配置改为测试本轮创建的供应商** —— 快速配置创建 Provider 后会立即设为 active，最后的连接测试不再沿用旧的 Chat 默认路由，避免用户贴了小米 key 却实际打到 DeepSeek。
- **快速配置补齐模型和能力识别** —— 与“添加供应商”的“拉取并识别能力”保持一致：自动拉取模型、写入 `supported_models`、识别 `model_capabilities`，并选择 default / reasoning model。
- **连接测试展示失败详情** —— 快速配置 / 首次向导的“测试连接”失败时显示后端错误，例如余额不足、鉴权失败或网关健康检查失败。

## [1.2.0] - 2026-05-28

围绕"新手第一次配 Provider 不知道填啥"这条主线做了一轮大改。

### 新增

- **MiMo 按 key 类型和 Token Plan 区域自动选域名** —— `sk-*` 按量付费 key 使用 `api.xiaomimimo.com`，`tp-*` Token Plan key 使用 `token-plan-{cn|sgp|ams}.xiaomimimo.com`；如果用户已经粘贴了 `sgp` / `ams` 订阅域名，GUI 快速添加、首次向导、手动表单、后端 create/update、headless `provider-add` 都会保持同一区域，避免 key、Chat host、Anthropic host 不匹配导致 401。
- **MiMo / DeepSeek 推荐模型映射自动补齐** —— 创建 Provider、拉取模型、测试连接成功、应用 Codex / Claude Code 配置时自动补齐缺失映射。Codex 的 `gpt-*` 自动映射到对应上游模型；Claude Code 的 `claude-*` 默认映射到普通 MiMo / DeepSeek 模型，不再自动配置 `[1m]` 后缀。
- **Provider 表单 3 段重构** —— 新手填表卡点：旧表单一上来 8+ 字段平铺加能力矩阵默认展开，跟用户脑里的"选个 Provider 粘 Key 就完事"模型不匹配。新结构：
  - Section A 基础：只露 type / name / api key（custom 类型才显式露 base_url）
  - Section B 模型与能力：合并"拉取模型"+"自动识别能力"为一个按钮，能力矩阵默认折叠
  - Section C 高级（默认折叠，标"通常无需修改"）：协议 + 各协议对应 URL 合并成一个 list，一眼看清"这家上游同时支持哪些原生入口"；Model Mapping 挪到最底部加"通常无需配置"提示
- **新建 Provider 后自动挑最新模型** —— 创建完后台跑：拉上游 `/models` → seed 能力矩阵 → 按 pickModels heuristic（`src/lib/modelHeuristics.ts`）挑出最新非 mini 作 default、最新 reasoning 系作 reasoning_model。用户填完 name + key 点创建什么都不用动，UI 自动刷出最新模型。heuristic 不 hardcode 模型名，靠 tierRank（主力/mini/preview）+ 版本号（过滤 8 位日期）+ reasoning pattern，扛模型迭代
- **ProviderCard 直连协议 chip** —— 卡片上加"直连 Chat" / "直连 Anthropic" 绿色 chip，直接告诉用户哪些客户端跟这个 Provider 走直连不需要协议转换
- **激进 applyPreset** —— 选 type 后总是覆盖 name 和 auto_cache_control（anthropic 系自动 ON、其他自动 OFF），创建场景隐藏 enabled / timeout 字段（用合理默认）

### 修复

- **原生直连模型解析规则收敛** —— 原生直连路径不再把“未命中映射”自动 fallback 到 `default_model`：Model Mapping 命中才改写模型名，未命中保持客户端 `model` 原样；只有协议转换路径继续使用 `default_model` 兜底。请求日志 trace 也区分 `native_pass_through` 和 `native_pass_through_model_mapping`，避免把“原生直连”和“协议转换”混在一起。
- **测试连接 "缓存支持检测" 卡死** —— 两个独立 bug 叠加：
  1. 后端 `detect_provider_cache` 用 `provider.timeout_seconds`（默认 120s+）作 HTTP timeout，跑两次请求。OpenAI 系 provider 误配 `anthropic_base_url` 时要等满 240s+ 才失败。改用 `min(provider.timeout_seconds, 15)` 硬上限
  2. 前端 `TestConnectionDialog` 的 `useEffect` 依赖了 `onClose` / `onSuccess` 闭包，父组件 `usePolling` 每 10s 重渲染产生新引用 → useEffect cleanup 重启 → 测试从头跑一遍。改用 `useRef` 持有最新回调，useEffect 只依赖 `provider?.id`
- **Codex Config Check 适配 1.1.0 劫持 OpenAI 设计** —— 自检有两条规则没跟上 1.1.0 Codex 集成重做：
  - `model_provider` 检查只认 `"agentgate"`，但新设计写的是 `"OpenAI"`（+ `requires_openai_auth = true` 劫持 OpenAI provider，让 IDE 插件 / Browser / Mobile / 配额查询都能用）。改成双模式都 OK。顺手干掉 `{:?}` debug 格式泄漏（UI 之前显示 `Some("OpenAI")` 而不是 `OpenAI`）
  - `auth.json` 检查盯着已废弃的 `has_agentgate_auth` 信号——新设计保留 ChatGPT OAuth tokens 不动，AgentGate token 改放 `config.toml` 的 `experimental_bearer_token`。改成判 `openai_key_polluted`（同时有 `ag_local_` 和 OAuth tokens 的脏状态才该 warn），干净态/不存在态都算 OK

### 重命名

- **服务商 → 供应商** —— 对齐 CC-Switch（72k stars）通用语，迁来用户零学习成本。Sidebar 注释 / CommandPalette / ErrorExplanationCard / tray.rs / i18n 全文 50+ 处替换；CommandPalette 关键词同时保留"服务商"作搜索别名兼容老用户。数据库字段、API 契约、变量名（`Provider*`）一概没动

### 文档

- **README + README_ZH 全面同步** —— 按"新手第一性原理"视角全面审计：
  - tagline: "23+ provider" → "24"，"vision-aware" → "capability-aware"
  - Multi-Provider Management 段：7 个 provider 列表 → 24 个 preset 按"国内 / 海外 / 聚合 / 自定义"分组
  - Vision-Aware Routing 整节重写为 Capability-Aware Routing（旧版还在讲二值 `supports_vision`，跟落地的 per-model 8 维矩阵脱节）
  - Add a Provider 段：旧的 8 字段平铺表 → 新的"快速通道 + 三段式手动模式"，跟前端表单重构对齐
  - Supported Providers 表：8 条 → 24 条全列；删 Vision 列（已是 per-model 不该列在 provider 层）
  - 加 Configure Gemini CLI / AtomCode 步骤
- **10 张截图刷新到最新 UI**（dashboard / providers / tools / gateway / routes / logs / diagnostics / settings / pet-settings / quick-setup），含三段式表单、供应商命名、直连 chip

---

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
  - Web Search Plugin 自动降级：Token Plan 请求预先剥离 MiMo 原生 `web_search` builtin；按量付费 key 遇到 `webSearchEnabled is false` 时剥离并重试一次，进程内记忆该 key 的不可用状态
  - Token-Plan host 按区域自动切换：粘贴 `tp-*` key 时默认使用 `token-plan-cn.xiaomimimo.com`；已配置 `sgp` / `ams` 时保持同一区域的 Chat 和 Anthropic host
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
- **`[1m]` suffix handling / 长上下文后缀处理**
  - MiMo / DeepSeek Claude Code 路径默认使用普通模型 ID；历史 `[1m]` 自动推荐映射会在后续保存 / 应用配置时修正回普通模型
  - Codex（OpenAI）路径完全不动
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
