export interface ProviderView {
  id: string;
  name: string;
  provider_type: string;
  base_url: string;
  masked_api_key: string | null;
  default_model: string;
  reasoning_model: string | null;
  supported_models: string | null;
  model_mapping: string | null;
  extra_headers: string | null;
  anthropic_base_url: string | null;
  responses_base_url: string | null;
  protocol: string;
  timeout_seconds: number;
  status: string;
  supports_vision: boolean | null;
  auto_cache_control: boolean | null;
  supports_cache: boolean | null;
  model_capabilities: string | null;  // JSON: {"model_id": ["text","vision",...]}
  // Refiner pipeline (default 全部关，需要全局总闸打开才生效，详见 Settings 页)
  provider_quirks: string | null;            // JSON: ProviderQuirks
  body_filter_enabled: number | null;        // null = 跟随全局, 0 = 强制关, 1 = 强制开
  thinking_rectifier_enabled: number | null;
  error_mapper_enabled: number | null;
  model_degradation_chain: string | null;    // JSON: {"requested":["fallback1","fallback2"]}
  enabled: boolean;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateProviderInput {
  name: string;
  provider_type: string;
  base_url: string;
  api_key?: string;
  default_model: string;
  reasoning_model?: string;
  supported_models?: string;
  model_mapping?: string;
  extra_headers?: string;
  anthropic_base_url?: string;
  responses_base_url?: string;
  auto_cache_control?: boolean;
  model_capabilities?: string;
  provider_quirks?: string;
  body_filter_enabled?: number | null;
  thinking_rectifier_enabled?: number | null;
  error_mapper_enabled?: number | null;
  model_degradation_chain?: string;
  protocol: string;
  timeout_seconds?: number;
  enabled?: boolean;
}

export interface UpdateProviderInput {
  name?: string;
  provider_type?: string;
  base_url?: string;
  api_key?: string;
  default_model?: string;
  reasoning_model?: string;
  supported_models?: string;
  model_mapping?: string;
  extra_headers?: string;
  anthropic_base_url?: string;
  responses_base_url?: string;
  auto_cache_control?: boolean;
  model_capabilities?: string;
  provider_quirks?: string;
  body_filter_enabled?: number | null;
  thinking_rectifier_enabled?: number | null;
  error_mapper_enabled?: number | null;
  model_degradation_chain?: string;
  protocol?: string;
  timeout_seconds?: number;
  enabled?: boolean;
}

/// Speedtest 报告项。手动触发，每个 provider 一份。
export interface ProviderSpeedReport {
  provider_id: string;
  provider_name: string;
  endpoint: string;
  status_code: number | null;
  connect_ms: number | null;
  ttfb_ms: number | null;
  total_ms: number;
  success: boolean;
  error: string | null;
}

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

export interface TestDiagnostic {
  /// Stable machine-readable code (invalid_api_key / insufficient_balance / ...).
  code: string;
  /// One-line plain-language reason.
  title: string;
  /// One-line actionable suggestion.
  hint: string;
  /// Optional URL the user can open to fix the issue.
  action_url?: string;
  /// Localized label for the action button.
  action_label?: string;
  /// Original HTTP/network error string (kept for power users).
  raw: string;
}

export interface ProviderTestResult {
  success: boolean;
  status: string;
  message: string;
  latency_ms: number | null;
  supports_vision: boolean | null;
  /// Present only on failure paths; older backends omit it (treated as undefined).
  diagnostic?: TestDiagnostic;
}

export const PROVIDER_TYPES = [
  // Tier 1: Major providers
  { value: "anthropic", label: "Anthropic (Claude)" },
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
