# Lessons

## 参考项目对比不能照搬

| 项目 | 内容 |
|---|---|
| 问题 | 对比 `/Users/mengmian/Develop/app/other/mimo2codex` 和 `/Users/mengmian/Develop/app/other/cc-switch` 时，不能把参考项目的代码结构或行为原样搬进 AgentGate。 |
| 原因 | AgentGate 的定位是多模型兼容的本地 coding agent workbench，需要把参考项目的新能力落到自己的协议转换、provider 能力、网关事件和 UI/诊断体系里。 |
| 规则 | 参考项目只作为功能语义和边界条件依据；实现前必须先判断 AgentGate 已有模块边界和抽象，优先用本项目现有 transform/gateway/provider/storage 模式承接。 |
| 适用范围 | 多项目对比、功能迁移、provider/tool 协议适配、release 修复和大范围一致性审查。 |

## 成本计算必须按 model 名匹配，不按 provider 实例名

| 项目 | 内容 |
|---|---|
| 问题 | 成本一直算成 0：`get_price` 原本要求 `provider`+`model` 都匹配，但 `request_logs.provider` 存的是 provider 实例名（如 `anthropic_official`），pricing 表的 provider 是类型名（如 `anthropic`），永远对不上。 |
| 规则 | 查价主路径是按 **model 名跨 provider** 匹配（参考 cc-switch 纯 model 匹配）；并对 `vendor/model` 形式去前缀（`z-ai/glm-5`→`glm-5`）。provider 精确匹配只作优先项，不能作为必要条件。 |
| 补价格 | 模型单价数据源是 `provider-catalog/providers/*.json` 的 `models[].pricing`，改后必须 `node scripts/generate-provider-catalog.mjs` 重新生成；`generated_provider_catalog.rs` / `generatedProviderCatalog.ts` 是产物，不可手改。缺失价格优先从 cc-switch 内置表（`src-tauri/src/database/schema.rs` 的 `seed_model_pricing`）取真实值，不捏造。 |
| 重算 | `backfill_costs` 每次启动跑，重算 `cost IS NULL` 的历史日志——补完价格重启 App 即生效。 |
