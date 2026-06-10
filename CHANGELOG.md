# Changelog / 更新日志

## [1.3.7] - 2026-06-10

### 新增

- **配置分享码** —— 客户端配置可一键导出为单行分享码,另一台机器粘贴即导入,免去手敲端口、token、模型映射。
- **模型级上下文窗口配置** —— Provider 编辑页的能力矩阵新增"上下文窗口"列,可为单个模型覆盖上下文窗口(token);留空自动显示并使用内置默认值。

### 改进

- **长历史自压缩默认开启 + 阈值自适应** —— 自动压缩超长对话历史从默认关改为默认开,触发阈值按模型上下文窗口的 85% 自适应(内置 MiMo / DeepSeek 128K,未收录的模型退回 110K),小窗口下保留段预算同步收窄。`AGENTGATE_AUTO_COMPACT=off` 可全关。
- **Codex 远程压缩(实验性,默认关)** —— 接住 Codex CLI 的 `remote_compaction_v2` 协议:Codex 在长上下文时把摘要请求发到网关,网关用当前主供应商生成摘要再以 SSE 返回,避免硬编码 `gpt-5.5-openai-compact` 模型导致的 503。设 `AGENTGATE_CODEX_COMPACT=1` 开启;SSE 协议层兼容性已被镜像 + 真实 `eventsource-stream` 解析两层测试覆盖。
- **首页"今日 Codex 压缩"卡片** —— Dashboard 加一张计数卡,展示当天通过网关完成的 Codex compaction 次数,便于看实验功能是否真的命中。
- **大组件拆分** —— Settings / Tools / Routes 三个超长页面拆成更小子组件,降低后续维护改动半径,对外行为不变。

## [1.3.6] - 2026-06-09

### 新增

- **配置历史可删除** —— 客户端配置历史和全局指令(CLAUDE.md / AGENTS.md)历史都能逐条删除,不再一直堆叠。「初始」快照受保护、不可删,保证随时能回滚到接入 AgentGate 前的原始配置。

### 修复

- **修复 agentgate-serve(CLI / Docker)无法构建** —— 连接池改造漏改了 headless 二进制,导致 CLI / Docker 镜像编不出来。

### 改进

- **数据库换连接池** —— 网关多个并发请求现在可以同时持有独立数据库连接(SQLite WAL 支持多 reader),旧的全局 Mutex 串行瓶颈解除。日常使用无感,QPS 高或日志量大时响应更稳。
- **数据库并发更稳** —— 连接加 busy_timeout,高并发写不再直接报错;数据库版本高于当前应用时明确提示升级而不是硬跑。

## [1.3.5] - 2026-06-08

### 修复

- **带图请求的视觉路由对齐三个入口** —— 此前只有 `/v1/responses` 会在请求带图时跳过不支持视觉的供应商;`/v1/chat/completions` 和 `/v1/messages` 不会,导致带图请求可能被路由到必然失败的供应商。现在三个入口统一:带图时跳过显式不支持视觉的供应商,失败转移场景自动选到支持视觉的候选。只看当前轮次的图片,历史图片不影响路由。
- **usage 统计的边界修正** —— 上游返回 `prompt_tokens` 为 `null` 时,token 统计现在会正确回退到 `input_tokens`,不再记成 0。
- **自动更新 endpoint 修正** —— 之前指向旧仓库名 `AgentGate`,仅靠 GitHub 改名重定向才能访问;现改为真实仓库 `agentgate-ai`,避免重定向失效时静默收不到更新。
- **更新失败不再静默** —— 用户点击「更新」后若安装失败,现会显示「安装更新失败」而不是无声清空;后台自动检查失败也会记录到 console 便于排查。

### 改进

- **网关请求处理收敛(内部重构)** —— 把散落在 `routes.rs` 里的 usage 提取、失败转移候选排序、日志 token 参数收敛为单一来源(新增 `gateway/usage.rs`、`gateway/failover.rs`),减少"改一处漏一处"。对外行为不变。

## [1.3.4] - 2026-06-08

### 新增

- **桌面宠物 AI 聊天** —— 双击宠物开聊天框,9 个角色各自有专属语气和反应,记住你说过的事(名字等)跨重启保留;失败显示真实原因(未设主供应商 / API key 缺失 / 调用错误)而不是无关问候。
- **原生右键菜单** —— 切换角色 / 打开网关 · 日志 · 设置 / 清空记忆 / 隐藏宠物,完全脱离宠物窗口,不再因展开挡住下方应用。
- **鼠标穿透模式** —— 让点击穿过宠物到下方应用。右键菜单 / Settings / tray 三处入口同步状态,不会卡在穿透里出不来。
- **单击戳一下** —— 拖拽阈值 4px,真单击触发角色专属反应气泡。

### 改进

- **窗口更小,挡屏更少** —— 默认窗口 140×200(原 220×240),气泡 / 聊天框出现时按需撑高,消失立刻还原。
- **聊天框退场更自然** —— 发完一句自动收起;点窗口外或按 Esc 也能关。
- **气泡更耐看** —— 悬停时暂停消失计时,点击立即关闭。
- **pet_chat 路径处理** —— 用 `smart_append_path` 避免 base_url 已含 `/v1` 时的双拼 404。
- **网关识别 Pet 客户端** —— 日志能区分 AgentGate Pet 发起的请求。

### 性能

- **轮询从 3s 改 10s + 事件驱动** —— Rust 端 gateway 启停时主动 emit,前端 listen 立即刷新;`document.hidden` 时全停轮询和事件监听,后台几乎零消耗。
- **mousemove 合并 + rAF 节流** —— drag-threshold 和 eye-follow 合一个 listener,eye-follow 用 ref 直写 DOM transform,不再每次鼠标动都触发 React render。
- **onMoved 拖窗 debounce 300ms** —— 之前每帧写一次 DB(60Hz),现在停手才写一次。
- **`get_pet_gateway_state` 拆 lite + stats** —— 10s 轮询只走索引查 state + last_error;全表 SUM 聚合放到 30 分钟 stats 气泡触发前才跑。
- **9 个 SVG 包 React.memo** —— 无关状态变化时不再 reconcile SVG 节点。

## [1.3.3] - 2026-06-05

### 新增

- **完整 Markdown 预览** —— 会话对话和全局指令预览接入 GFM 渲染，支持表格、任务列表、代码块等常见 Markdown 内容。
- **请求详情链路与成本** —— 在同一视图集中展示路由、供应商、模型、状态、tokens、成本、fallback 和错误链路摘要。

### 改进

- **日志页更易扫** —— 新增筛选摘要，常用 / 高级筛选分层，同步入口折叠，未记录延迟不再显示 0ms。
- **会话对话更清晰** —— 工具调用 / 工具结果分层展示，工具结果限制高度并保留原始输出格式。

### 修复

- **路由策略统计不再一直为 0** —— 旧网关日志缺少 `route_decision.profile_id` 时，按 route 归到当前默认策略统计。

## [1.3.2] - 2026-06-05

### 新增

- **Claude Desktop 接入**（macOS） —— 把 Claude Desktop 的第三方推理网关指向 AgentGate，一键应用、可历史回滚。
- **会话查看完整对话** —— 日志「会话」视图点开就能看 Claude Code / Codex 的完整聊天记录，并附一键复制的恢复命令。
- **智能路由选模** —— 失败转移可选「最便宜」/「最快」策略，自动按单价或延迟挑供应商。
- **主动健康探测**（默认关） —— 后台定期探活供应商，结果显示在卡片上，不影响实际路由。

### 改进

- **成本统计更准** —— 直通客户端的请求也能算成本；缺价模型标「无价格」，区分「真免费」和「算不出」；成本分解新增「按策略」维度，并过滤掉无 token 的噪音条目。
- **日志筛选增强** —— 模型筛选改成下拉，错误类型补上「网络」「协议转换」，「会话」视图也跟随筛选了。
- **MiniMax 兼容** —— 补齐对严格 API 的字段处理，接入不再容易报 400。

### 修复

- 修复成本一直算成 $0 —— 改为按模型跨供应商匹配价格，并补充内置价格表。
- 网关新增建连超时，避免上游不可达时请求挂死；修正 Gemini 直通在流式中断时被误记为成功的统计偏差。

## [1.3.1] - 2026-06-02

### 新增

- **全局指令文件管理** —— 在 AgentGate 内直接编辑 `~/.claude/CLAUDE.md` / `~/.codex/AGENTS.md`，4 个内置模板（极简中文规范 / TDD / 代码评审 / 安全审计）可覆盖或追加。写盘前自动 snapshot，可一键回滚。
- **客户端配置版本史与一键回滚** —— 5 个客户端每次「应用 / 关闭 / 切换」前自动 snapshot 盘上配置，保留初始版本 + 最近 10 条滚动。卡片上新增「历史」按钮，二次确认后一键回滚。
- **一键重启 Codex 桌面应用**（macOS） —— Codex 应用配置后弹窗多一个重启按钮,按 basename 精确匹配只杀桌面 App,不动 CLI。默认手动触发,Windows / Linux 不显示。
- **节点测速** —— Providers 页对所有启用 provider 并行发 1-token 探测,按延迟排序,连接 / TTFB / 总耗时三段染色。手动触发。
- **网关精炼层** —— 「设置 → 通用」全局开关,三项默认关:请求字段过滤(剥不支持字段)、推理参数校正(`thinking.budget_tokens` / `reasoning.effort` 收口到 provider 范围)、错误响应归一(识别中英文上下文超长提示统一标 `context_length_exceeded`)。内置 DeepSeek / MiMo / Anthropic / OpenAI / Kimi 规则。

### 改进

- **客户端页改主从布局** —— 5 张厚卡手风琴改成左侧 260px 列表 + 右侧详情。列表常驻显示三态状态点（已接入 / 已检测 / 未检测）,详情区不再被卡片切碎,选中项 sessionStorage 记忆。

## [1.3.0] - 2026-06-01

### 新增

- **测试连接失败结构化诊断** —— 失败不再裸 `HTTP 401`。13 种典型失败各给一句话原因 + 建议，11 家主流 provider 直接给「去重建 key」「查看账户余额」「去开通插件」等一键按钮，跳到对应控制台。
- **快速配置自动识别剪贴板里的 key** —— 进入快速配置时悄悄读一次剪贴板，识别为已知 key 就在输入框上方弹 banner，点「填入」直接带进表单 + 自动选好 provider type。读不到 / 内容不是 key 就完全不打扰。
- **应用客户端配置后检测进程** —— Codex / Claude Code / OpenCode / Gemini CLI / AtomCode 应用配置成功后，弹窗显示配置 path + 进程状态，正在跑就给 PID + 一键复制 `kill <pid>`。从不自动 kill。

### 修复

- 测试连接没转发 `extra_headers`，Kimi 在 UA 校验下一律 401，看起来像 key 错了。
- 测试连接没解析多 key JSON 数组，`["sk-a","sk-b"]` 当字面量拼 Bearer 必 401。
- 编辑 provider 时表单总显示 1 个空槽，看不到现有几把 key、改的是哪一把；现在编辑时回填全部 key。

## [1.2.4] - 2026-05-30

### 修复

- **AtomCode / OpenCode 一键配置改用 `agentgate` 虚拟模型** —— 不再把当前 provider 的真实模型名固化进客户端，切换 provider 后不会带着旧模型名 400。原生真实模型名仍按 Model Mapping 规则透传。

## [1.2.3] - 2026-05-30

### 修复

- **DeepSeek 默认模型收敛到 v4** —— 不再把即将下线的 `deepseek-chat` / `deepseek-reasoner` 作为自动配置目标。
- **MiMo Token Plan 区域域名保持一致** —— `sk-*` 用开放 API 域名，`tp-*` 用 `token-plan-{cn|sgp|ams}` 并保持同一区域 host。
- **MiMo `web_search` 自动降级** —— Token Plan 预剥离；按量付费遇 Plugin 未开通时剥离后重试一次。
- **Claude Code 直连模型不再写 `[1m]` 后缀** —— 默认推荐映射用普通模型 ID。
- **能力降级可诊断** —— 图片剥离、`web_search` 降级、MCP advisory 等事件写入请求日志的 `degradation_events`。

## [1.2.2] - 2026-05-29

### 修复

- **Windows 桌面宠物白色背景** —— Windows 的 WebView2 控件默认底色不透明，给 pet 窗口加显式深色背景，宠物在 Windows 上不再是个白卡片。macOS 行为不变。

## [1.2.1] - 2026-05-29

### 修复

- **MiMo / DeepSeek 不再自动配置 `[1m]` 长上下文模型** —— 普通模型 ID 已经够用，避免上游返回 `Not supported model`。
- **快速配置测试连接打错 provider** —— 创建后立即设为 active，不再沿用旧路由测试。
- **快速配置补齐模型 + 能力识别** —— 与「添加供应商 → 拉取并识别能力」流程一致。
- **连接测试失败显示具体原因** —— 不再笼统报错。

## [1.2.0] - 2026-05-28

### 新增

- **MiMo 按 key 类型和区域自动选域名** —— `sk-*` 走开放 API，`tp-*` 走 token-plan-{cn|sgp|ams}，避免 key、Chat host、Anthropic host 不匹配 401。
- **MiMo / DeepSeek 推荐模型映射自动补齐** —— 创建 provider、拉模型、应用配置时自动补齐 `gpt-*` → 上游模型 / `claude-*` → 上游模型 的映射。
- **Provider 表单三段重构** —— 基础 → 模型与能力 → 高级（默认折叠）。新手不再被 8+ 字段平铺吓住。
- **新建 provider 后自动挑最新模型** —— 后台自动拉上游模型 → 识别能力 → 按版本/层级选 default + reasoning model，不 hardcode 模型名。
- **Provider 卡片直连协议 chip** —— 「直连 Chat」「直连 Anthropic」一眼看出哪些客户端走直连不需要协议转换。

### 修复

- **原生直连模型解析规则收敛** —— 未命中 Mapping 时保持客户端 model 原样，不再自动回退到 default_model（协议转换路径继续兜底）。
- **测试连接「缓存支持检测」卡死** —— 误配 anthropic_base_url 时要等满 240s+ 才失败；现在 15s 硬上限。前端轮询不再每 10s 重启检测。
- **Codex 配置自检不再误报「未配置」** —— 适配 1.1.0 的「劫持 OpenAI provider」写法，干净的 `auth.json` 不再 warn。

### 重命名

- **服务商 → 供应商** —— 对齐 CC-Switch 通用语，老用户搜索关键词仍可用「服务商」别名。

## [1.1.2] - 2026-05-28

- **更新安装完成后自动重启** —— 不再需要手动 quit + 重开。

## [1.1.1] - 2026-05-27

- **主题扩展到 8 套** —— 暖琥珀、晴日、钢蓝、松林、紫夜、米麻、雾蓝、樱粉。设置页换成 2×4 色板预览。
- **应用图标重画** —— 米黄圆角矩形底 + 三色原子轨道，macOS 暗色 dock 里能看出形状了。
- 修复 `agentgate-serve` CLI 二进制编译失败。

## [1.1.0] - 2026-05-27

围绕 Codex.app IDE 插件兼容 + 小米 MiMo 集成 + 缓存命中率可视化做了一轮大改。

### 新增

- **Codex.app IDE 插件兼容** —— 改用「劫持 OpenAI provider + `requires_openai_auth = true`」配置，IDE 插件 / Browser / Mobile / 配额查询全部可用，对话请求实际走 AgentGate；ChatGPT OAuth tokens 完全保留。
- **压缩请求体支持** —— Codex.app 的 zstd / ChatGPT 桌面客户端的 gzip 不再 500 报「invalid utf-8」。
- **会话亲和（Session Affinity）** —— 上游回 `cached_tokens > 0` 时记录 session → provider 1 小时绑定，同会话后续请求优先复用同 provider，最大化 prompt cache 命中。
- **SSE 首段错误保护** —— 上游 HTTP 200 但首段塞错误事件（quota / ban / rate-limit）时自动切换下个 provider，客户端零感知。
- **缓存 Token 写/读拆分** —— Dashboard 显示「缓存」inline footer，命中率自动计算。
- **Dashboard 时段切换** —— 今天 / 7 天 / 14 天 / 30 天 tab。
- **实时 KPI 页脚** —— Dashboard 底部 6 项指标（活跃连接 / 运行时间 / 累计请求 / Tokens / 费用 / 成功率），5 秒刷新。
- **托盘菜单实时状态** —— 当前 active provider / 今日请求数 / 网关端口，「切换服务商」子菜单一键切换。
- **日志分页** —— 每页 100 条，过滤切换自动回首页。
- **小米 MiMo 一等公民支持** —— 完整 5 个聊天模型，多轮 reasoning_content 回填，Token Plan 区域域名匹配，Web Search Plugin 自动降级，内置定价。
- **每模型能力矩阵** —— 8 种能力（text / vision / audio_in / tts / video_in / reasoning / tools / web_search）每个模型独立勾选。带图请求自动 swap 到支持 vision 的模型；用户取消勾选某 model 的 web_search 即停止下发。
- **Codex `web_search_preview` → MiMo `web_search` 翻译** —— Codex 的联网搜索能力穿透到 MiMo。
- **日志精确客户端识别** —— 按 User-Agent 区分 Codex / Claude Code / OpenCode / Cursor / Cherry Studio / Continue / Cline / Roo Code 等。
- **Dashboard 重做** —— 9 张冗余卡 → 顶部今日 5 个核心指标 + 7 天柱状图。

### 修复

- **MiMo 多轮历史图片穿透** —— Codex 会话早期带过图，后续纯文本请求不再 404。
- **DeepSeek API key 识别** —— `sk-` + 32 位 hex 不再被错认 OpenAI。
- **Vision 探针对 MiMo 误报** —— 4xx 都视为不支持（之前只识别 400）。
- **网络抖动自动重试** —— 池静默死连接 / connect 失败带 backoff 重试。

## [1.0.0] - 2026-05-20

正式发布。

## [0.8.x] - 2026-05-18 ~ 2026-05-20

- 桌面宠物在屏幕外坐标恢复时拉回可见区域。
- macOS / Docker release 流程修复（`latest.json` 缺平台、CLI 子程序签名、Dockerfile 依赖与上下文）。
- 首次安装空日志表不再报数据库错误。
- 桌面包不再包含 headless CLI（CLI 改为独立产物）。

## [0.5.x] - 2026-05-18

- 隔离 release 各平台 CLI 构建输出。
- 清理启动时的示例请求日志（`req-seed-*`）。
- macOS Dock/Finder 启动时托盘菜单正确识别中文语言。
- 独立 `agentgate-serve` CLI 发布产物开始随版本一起发。

## [0.5.0] - 2026-05-18

### 新增

- **Gemini 原生格式支持** —— Codex → Gemini 双向转换，Gemini CLI 可通过 AgentGate 接 DeepSeek / Kimi 等任意 provider。
- **任务级智能路由** —— 按输入长度 / 图片 / 工具 / 系统关键词自动选 provider 和模型。
- **Headless 服务模式** —— `agentgate-serve` CLI 二进制 + Docker。
- 日志时间显示日期（MM-DD HH:MM:SS）。

## [0.4.0] - 2026-05-18

### 新增

- **费用追踪** —— 22 个内置模型价格，每条请求自动计算费用，仪表盘展示总费用 / 今日 / 平均，设置页价格表可内联编辑或自定义覆盖。
- **多账号轮转** —— 同 provider 支持多 API Key（JSON 数组），自动 round-robin。
- **Prompt Cache 自动注入** —— Codex → Anthropic 转换路径自动给 system / tools / 最后 assistant 打 `cache_control`。
- **缓存能力自动探测** —— 测试连接时发两次相同请求，看 `cache_read_input_tokens > 0`。
- **Provider 健康面板** —— 卡片内嵌 1h / 24h 成功率、平均延迟、P95 延迟、请求数。
- **请求重试** —— 429 / 500 / 502 / 503 自动退避重试，尊重 `Retry-After`，每次重试换 Key。

### 修复

- 费用不计算（provider 名大小写不敏感匹配 + 启动时回填历史）。
- kimi-for-coding 价格缺失。
- Gemini CLI / AtomCode 硬编码文案改 i18n。

## [0.3.0] - 2026-05-18

### 新增

- **23 个 provider 预设**（原 7 个）—— 新增 Google Gemini / xAI / Mistral / Groq / Together / Fireworks / Cerebras / Perplexity / Cohere / 智谱 GLM / 通义千问 / 硅基流动 / 火山引擎 / 百川 / 阶跃星辰 / 零一万物。
- **Gemini CLI / AtomCode 客户端一键配置** —— 写入对应配置文件，支持切换官方 / AgentGate。

### 修复

- Anthropic SSE 流错误 / 空内容结束时给客户端发 `response.failed`。
- 兼容 `\r\n\r\n` SSE 分帧和 `event:X` 无空格格式。
- 识图后请求不切回原 provider（vision 路由只看最后一条）。
- Gemini CLI 配置不生效（环境变量写到 `.env`、auth type 设为 `gemini-api-key`）。
- AtomCode 报「未配置活跃 Provider」（补 `default_provider` 顶层字段）。

## [0.2.x] - 2026-05-15 ~ 2026-05-18

- 模型映射下拉组件支持下拉选择 + 手动输入，深色主题可见。
- macOS ad-hoc 签名 + 自动更新支持（`.app.tar.gz`）。

## [0.1.9] - 2026-05-15

### 新增

- **Claude Messages API 原生支持** —— `provider_type=anthropic` 时 Codex 请求转 Claude 原生 Messages 格式，完整支持 `tool_use` / `tool_result` / `input_schema` / `thinking.budget_tokens`。
- **URL 驱动的路由机制** —— 新增 `responses_base_url`，有值就透传到上游 Responses API 端点；按字段判断走透传 / Claude 转换 / Chat 转换。
- **智能 URL 拼接** —— 填完整 URL 或 base URL 都识别。
- **`local_shell` 工具转换** —— Codex 内置工具转标准 `shell` function。
- **Provider 类型增加 Anthropic / MiniMax**，协议在卡片显示友好标签。

### 修复

- DeepSeek 不再收到无意义的 `thinking` 字段。
- 非流式空 `choices` 不再让 Codex 挂起。
- Tool call ID 截断至 64 字符（Responses API 限制）。
- `anthropic_base_url` 不再误触发 Claude 转换。

## [0.1.5] - 2025-05-14

### 修复

- **中文内容导致网关 panic** —— 所有字符串截断改用 `is_char_boundary` 安全截断。
- **Tool output 截断导致 Codex 崩溃** —— 移除 4000 字节截断限制，tool output 原样透传。
- **错误响应格式** —— 添加 `type` 字段，客户端不再显示「Unknown error」。
- **认证 5xx 错** —— `GATEWAY_AUTH_*` 正确返回 401。
- **SSE 事件日志溢出** —— 严格守住 1MB 上限。
- **删除 provider 不级联** —— 同步清理 route_profile_providers。
- **API Key 显示遮罩** —— 固定长度 `sk-1****cdef`。

### 性能

- `get_stats` 查询从 14 条 SQL 合并为 3 条。
- 添加 `request_logs.timestamp` 索引。

## [0.1.4] - 2025-05-14

### 新增

- **Claude Code 一键切换** —— 与 Codex 同样的 save / restore 机制。
- **OpenCode 一键配置** —— 写入 `~/.config/opencode/opencode.json`。
- **路由系统** —— 按协议自动建 3 个默认路由，新增 provider 自动加入所有路由链；UI 支持创建、inline 重命名。
- **导航重构** —— 概览 → 服务商 → 路由 → 网关 → 客户端 → 日志 → 诊断 → 设置。

## [0.1.3] - 2025-05-14

### 新增

- **Codex 配置一键切换** —— 「切换到官方 / 切换到 AgentGate」整体保存恢复 `config.toml` + `auth.json`。
- **保留官方会话** —— 应用 AgentGate 时保留原始 OAuth tokens，切回官方对话记录不丢。
- **污染检测警告** —— 检测到 `OPENAI_API_KEY` 被旧版覆盖时弹黄色警告。

## [0.1.2] - 2025-05-13

- 版本号从 Tauri API 运行时读取，不再硬编码。
- 检查更新失败静默处理。
- Codex 默认模型更新到 `gpt-5.5`。
- 工具页加备份历史 + 一键恢复。

## [0.1.1] - 2025-05-13

- 应用图标、应用内自动更新、设置页检查更新按钮、CI/CD 多平台自动发版、中英双语 README。

## [0.1.0] - 2025-05-13

自用开源成公开版本。

- 协议转换网关（OpenAI Responses / Anthropic Messages / Chat Completions 互转）。
- 多 provider 管理（DeepSeek / OpenAI / OpenRouter / Kimi / 自定义）。
- Route Profile 路由配置（多 provider 优先级 + failover）。
- 工具一键配置（Codex / Claude Code / OpenCode）。
- 模型映射、自定义请求头、reasoning effort 透传。
- Token 用量、费用统计、Dashboard 自动刷新。
- 请求日志 + 诊断自检 + 导出诊断包。
- 系统托盘、开机自启、中英双语、配置自动备份。
