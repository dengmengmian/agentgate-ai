//! 全局指令文件（`~/.claude/CLAUDE.md` / `~/.codex/AGENTS.md`）的内置模板。
//!
//! - 模板是**只读静态资源**。不会随用户操作变化，也不进 DB。
//! - 模板正文是 Markdown 纯文本，由 [`crate::tools::instructions`] 在 apply 时
//!   按 overwrite / append 写入目标文件。
//! - `scopes` 表示该模板适用于哪些 scope：`"claude"` / `"codex"` / `"all"`。
//!   前端按当前 scope 过滤模板列表。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InstructionsTemplate {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// "claude" / "codex" / "all"
    pub scopes: &'static [&'static str],
    pub content: &'static str,
}

const MINIMAL_ZH: &str = r#"## 核心原则

1. 第一性原理：先判断真实目标，方向错就直接说，给更优路径。
2. 极简沟通：中文输出、结论先行、语言直接，不复读背景。
3. 简单优先：能小改不大改、能直观不抽象。
4. 只改必要范围：每一处改动都能对应到本次需求。
5. 暴露问题：发现风险、冲突、不可验证点，直说，不要糊。
6. 不擅自开分支、不擅自重构、不擅自删除已有代码。
7. 改后自检：是否改多、是否复杂化、是否漏验证、是否有更简方案。

## 工程规范

- 不捏造数据；生产代码禁止 Mock；Mock 只用于测试或本地调试。
- 不写用户没要求的功能、不做单次使用的抽象、不重构无关代码。
- 自己造成的 unused import / variable / 孤儿代码必须清理。
- 静默吞错、假成功、猜测式补丁、掩盖根因的兜底都禁止。

## 输出规范

1. 结论先行。
2. 对比、评审、任务拆解优先用 Markdown 表格。
3. 简单问题直接答，不强行表格、不流水账、不用 P0/P1/P2。
"#;

const TDD: &str = r#"## TDD 模式

每个改动按以下顺序：

1. **写测试**：先写一个最小可失败的测试，明确预期行为。
2. **跑红**：确认测试因为目标行为缺失而失败，不是因为编译或环境错。
3. **写实现**：写最小代码让测试通过，不超出当前测试范围。
4. **跑绿**：所有测试通过，不只是新写的那一个。
5. **重构**：在测试保护下整理代码，每次小步、每次跑测试。

## 边界

- 不允许跳过红→绿步骤，不允许「先写实现回头补测试」。
- 不要为了凑覆盖率写无意义断言。
- UI / 端到端流程允许用手动验证替代单测，但要在 PR 里说明。
- 修 bug 必须先用一个能复现的测试锁住，再修。
"#;

const CODE_REVIEW: &str = r#"## 代码评审模式

阅读 diff 时优先按以下顺序：

1. **正确性**：逻辑、边界、并发、错误路径是否正确。
2. **安全**：输入校验、注入、权限、密钥泄露。
3. **性能**：是否有 N+1、不必要的循环、阻塞调用。
4. **可读性**：命名、结构、注释是否帮助下一个读者。
5. **风格**：放最后，能格式化解决的不要单开评论。

## 反馈方式

- 区分 **必改 / 建议 / 疑问**，不要一律「建议」。
- 必改：指出问题 + 给出修复方向，不要只说「这样不好」。
- 建议：解释 trade-off，让作者自己决定。
- 疑问：表明自己不确定，不要假装是必改。

## 禁止

- 不要逐行复述代码做了什么。
- 不要在评审里夹带个人偏好（tabs vs spaces 之类）。
- 不要要求作者做超出 PR 范围的重构。
"#;

const SECURITY_AUDIT: &str = r#"## 安全审计模式

每次改动按以下清单过一遍：

| 类别 | 检查项 |
| --- | --- |
| 输入 | 所有外部输入（HTTP、文件、env、CLI args）是否校验？是否有长度 / 类型 / 范围限制？ |
| 注入 | SQL / 命令 / 模板 / HTML / 正则是否使用了参数化或转义？ |
| 认证 | 是否存在未鉴权入口？token / session 是否有过期与撤销机制？ |
| 授权 | 资源访问是否按 owner / role 做了二次校验，而不是只靠前端隐藏？ |
| 密钥 | 是否有 hardcoded key / token / 证书？日志 / 错误信息会泄露密钥吗？ |
| 加密 | 是否使用了被淘汰算法（MD5/SHA1/DES）？随机数源是否安全？ |
| 依赖 | 引入的库是否有已知 CVE？版本是否锁定？ |
| 错误 | 错误信息是否泄露内部结构（stack trace、SQL 原文、文件路径）？ |

## 报告方式

- 风险按 **高 / 中 / 低** 分类，每条说明：影响 / 复现 / 修复建议。
- 不要堆砌「最佳实践」清单，只列出本次代码里**真实存在**的问题。
"#;

pub const TEMPLATES: &[InstructionsTemplate] = &[
    InstructionsTemplate {
        id: "minimal-zh",
        title: "极简中文规范",
        description: "结论先行、最小改动、不擅自重构的中文工作规范。",
        scopes: &["all"],
        content: MINIMAL_ZH,
    },
    InstructionsTemplate {
        id: "tdd",
        title: "TDD 模式",
        description: "强制先写测试、再写实现、再重构的工作流。",
        scopes: &["all"],
        content: TDD,
    },
    InstructionsTemplate {
        id: "code-review",
        title: "代码评审模式",
        description: "按正确性 / 安全 / 性能 / 可读性 / 风格顺序评审，区分必改/建议/疑问。",
        scopes: &["all"],
        content: CODE_REVIEW,
    },
    InstructionsTemplate {
        id: "security-audit",
        title: "安全审计模式",
        description: "按输入、注入、认证、授权、密钥、加密、依赖、错误信息清单审计。",
        scopes: &["all"],
        content: SECURITY_AUDIT,
    },
];

pub fn find(id: &str) -> Option<&'static InstructionsTemplate> {
    TEMPLATES.iter().find(|t| t.id == id)
}
