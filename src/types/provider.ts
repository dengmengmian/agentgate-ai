// 历史上手抄的 types,现在从 bindings (Rust 端 specta 反射) 再导出。
// 边界 shim：bindings 把 Rust 端 `Option<T>` 映射成 `T | null`,但前端历史
// 代码大量构造 `{ x: undefined }` 形式的 partial。这里 re-export 时用 Partial
// 类型保持兼容(field 可以省略 = undefined)。input 类型走 Partial,output
// 类型(View / Result)是后端给的固定 shape,保留 null。
import type {
  ProviderView as Wide,
  CreateProviderInput as WideCreate,
  UpdateProviderInput as WideUpdate,
  ProviderTestResult,
  ProviderSpeedReport,
} from "@/lib/bindings";

export type ProviderView = Wide;
// 创建/更新输入：所有 `T | null` 改成 `T | null | undefined`,前端可省略字段。
export type CreateProviderInput = {
  [K in keyof WideCreate]?: WideCreate[K] | undefined;
} & Pick<WideCreate, "name" | "provider_type" | "base_url" | "default_model" | "protocol">;
export type UpdateProviderInput = {
  [K in keyof WideUpdate]?: WideUpdate[K] | undefined;
};

export type { ProviderTestResult, ProviderSpeedReport };

// 显示用 lookup table——纯前端常量。
export const CAPABILITY_LABELS: Record<string, string> = {
  text: "文本",
  vision: "视觉",
  audio_in: "音频输入",
  tts: "语音合成",
  video_in: "视频",
  reasoning: "推理",
  tools: "工具",
  web_search: "联网搜索",
};

export const ALL_CAPABILITIES = ["text", "vision", "audio_in", "tts", "video_in", "reasoning", "tools", "web_search"] as const;
export type Capability = (typeof ALL_CAPABILITIES)[number];

// `TestDiagnostic` 在 bindings 里已经有,但历史 import 路径在这里,re-export 一下。
export type { TestDiagnostic } from "@/lib/bindings";

export const PROVIDER_TYPES = [
  // Tier 1: Major providers
  { value: "anthropic", label: "Anthropic (Claude)" },
  { value: "copilot", label: "GitHub Copilot" },
  { value: "deepseek", label: "DeepSeek" },
  { value: "openai", label: "OpenAI" },
  { value: "google_gemini", label: "Google Gemini" },
  { value: "xai", label: "xAI (Grok)" },
  { value: "mistral", label: "Mistral AI" },
  // Tier 2: Inference providers
  { value: "groq", label: "Groq" },
  { value: "together", label: "Together AI" },
  { value: "fireworks", label: "Fireworks AI" },
  { value: "cerebras", label: "Cerebras" },
  { value: "perplexity", label: "Perplexity" },
  { value: "cohere", label: "Cohere" },
  // China providers
  { value: "mimo", label: "MiMo (小米)" },
  { value: "kimi", label: "Kimi (月之暗面)" },
  { value: "minimax", label: "MiniMax" },
  { value: "glm", label: "GLM (智谱)" },
  { value: "dashscope", label: "通义千问 (DashScope)" },
  { value: "siliconflow", label: "硅基流动 (SiliconFlow)" },
  { value: "volcengine", label: "火山引擎 (豆包)" },
  { value: "baichuan", label: "百川 (Baichuan)" },
  { value: "stepfun", label: "阶跃星辰 (StepFun)" },
  { value: "yi", label: "零一万物 (01.AI)" },
  { value: "sensenova", label: "商汤日日新 (SenseNova)" },
  { value: "modelscope", label: "魔搭 (ModelScope)" },
  // Aggregators
  { value: "openrouter", label: "OpenRouter" },
  // Custom
  { value: "custom_openai_compatible", label: "Custom OpenAI Compatible" },
] as const;

export const PROTOCOLS = [
  { value: "openai_chat_completions", label: "OpenAI Chat Completions" },
  { value: "openai_responses", label: "OpenAI Responses" },
  { value: "anthropic_messages", label: "Anthropic Messages" },
] as const;
