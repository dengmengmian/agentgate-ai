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
  protocol: string;
  timeout_seconds: number;
  status: string;
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
  protocol?: string;
  timeout_seconds?: number;
  enabled?: boolean;
}

export interface ProviderTestResult {
  success: boolean;
  status: string;
  message: string;
  latency_ms: number | null;
}

export const PROVIDER_TYPES = [
  { value: "deepseek", label: "DeepSeek" },
  { value: "openai", label: "OpenAI" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "kimi", label: "Kimi" },
  { value: "custom_openai_compatible", label: "Custom OpenAI Compatible" },
] as const;

export const PROTOCOLS = [
  { value: "openai_chat_completions", label: "OpenAI Chat Completions" },
  { value: "openai_responses", label: "OpenAI Responses" },
  { value: "anthropic_messages", label: "Anthropic Messages" },
] as const;
