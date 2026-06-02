# Lessons

## 参考项目对比不能照搬

| 项目 | 内容 |
|---|---|
| 问题 | 对比 `/Users/mengmian/Develop/app/other/mimo2codex` 和 `/Users/mengmian/Develop/app/other/cc-switch` 时，不能把参考项目的代码结构或行为原样搬进 AgentGate。 |
| 原因 | AgentGate 的定位是多模型兼容的本地 coding agent workbench，需要把参考项目的新能力落到自己的协议转换、provider 能力、网关事件和 UI/诊断体系里。 |
| 规则 | 参考项目只作为功能语义和边界条件依据；实现前必须先判断 AgentGate 已有模块边界和抽象，优先用本项目现有 transform/gateway/provider/storage 模式承接。 |
| 适用范围 | 多项目对比、功能迁移、provider/tool 协议适配、release 修复和大范围一致性审查。 |
