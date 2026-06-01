# Contributing to AgentGate

Thanks for your interest in contributing! AgentGate is a local AI gateway built with Rust + Tauri + React.

## How to Contribute

### Reporting Bugs

- Use the [Bug Report](https://github.com/dengmengmian/AgentGate/issues/new?template=bug_report.yml) template
- Include your OS, AgentGate version, provider, and client
- Redact API keys from any logs you share

### Suggesting Features

- Use the [Feature Request](https://github.com/dengmengmian/AgentGate/issues/new?template=feature_request.yml) template
- Describe the problem you're solving, not just the solution

### Submitting Pull Requests

1. **Open an issue first** to discuss the change before writing code
2. Fork the repo and create a branch from `main`
3. Make your changes
4. Run tests:
   ```bash
   cd src-tauri && cargo test --lib   # 268 unit tests
   bash scripts/test-integration.sh    # 31 integration tests
   npx tsc --noEmit                    # TypeScript check
   ```
5. Open a PR with a clear description of what changed and why

### Smoke tests before releasing

发版前用一条命令把三层能力转换都过一遍：

```bash
# 离线 fixture：5 个 test binary，全 mock 上游，不需要 key
bash scripts/release-smoke.sh

# 加上真实 provider 验证（先在桌面 App 里配好 MiMo / DeepSeek / Kimi）：
AG_RUN_SMOKE_TESTS=1 bash scripts/release-smoke.sh
```

两条腿做什么：

| 腿 | 文件 | 触发 | 覆盖 |
|---|---|---|---|
| 离线 fixture | `tests/{capability,mimo,deepseek,kimi,protocol}_*.rs` | 每次 PR（GitHub CI）+ release preflight + 本地 | L1 协议转换、L2 模型映射 + agentgate 虚拟模型、L3 能力（vision swap / image strip / web_search 降级 / reasoning placeholder / [1m] 剥离） |
| 真实 smoke | `tests/smoke_test.rs` | 本地手动（`AG_RUN_SMOKE_TESTS=1`） | 真实 provider 联调（Responses / Chat / Messages 三端点 + 8.x 协议矩阵） |

真实 smoke 要从本地 SQLite 拿 provider key，永远不会在 GitHub 上跑。env 参数（`AG_SMOKE_ANTHROPIC_PROVIDER_ID` 等）在 `src-tauri/tests/smoke_test.rs` 顶部有说明。

### Adding a New Provider

Adding a provider preset is the easiest way to contribute:

1. Edit `src/components/providers/ProviderFormDialog.tsx` — add to `PROVIDER_PRESETS`
2. Edit `src/types/provider.ts` — add to `PROVIDER_TYPES`
3. Add default pricing in `src-tauri/src/storage/pricing.rs` — `DEFAULTS` array

If the provider needs special handling (like DeepSeek's schema cleaning or Kimi's web_search), add a provider module in `src-tauri/src/transform/providers/`.

### Adding a New Client

1. Create `src-tauri/src/tools/your_client.rs` (follow `opencode.rs` pattern)
2. Register in `src-tauri/src/tools/mod.rs`
3. Add Tauri commands in `src-tauri/src/app/commands.rs`
4. Add frontend card in `src/pages/Tools.tsx`

## Development Setup

```bash
# Prerequisites: Node.js 20+, pnpm 10+, Rust 1.75+
pnpm install
pnpm tauri dev
```

## Code Style

- Rust: `cargo clippy` should pass with no new warnings
- TypeScript: `npx tsc --noEmit` must pass
- No unnecessary abstractions — simple > clever
- Comments only where the logic isn't self-evident

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
