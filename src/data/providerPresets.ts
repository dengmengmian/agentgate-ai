/// 每个 provider 接入时的默认值，供 ProviderFormDialog（手动表单 / quick mode）
/// 和 QuickSetup（首次引导向导）共用。这里是**唯一权威源**：新增 provider 在
/// 这里加一条即可，两侧自动生效。
///
/// 历史上 ProviderFormDialog 和 QuickSetup 各维护了一份，QuickSetup 那份只有
/// 7 个 provider——`detectProviderType` 能识别 mimo / kimi 等，但 QuickSetup
/// 拿到这些 type 时会 undefined → crash。合并到这里顺手把这个潜在 bug 解决。
export interface ProviderPreset {
  baseUrl: string;
  protocols: string[];
  defaultModel: string;
  reasoningModel?: string;
  anthropicBaseUrl?: string;
  responsesBaseUrl?: string;
  extraHeaders?: string;
}

export interface ProviderEndpointUrls {
  baseUrl: string;
  anthropicBaseUrl?: string;
}

export const MIMO_PAYG_ENDPOINTS: ProviderEndpointUrls = {
  baseUrl: "https://api.xiaomimimo.com/v1",
  anthropicBaseUrl: "https://api.xiaomimimo.com/anthropic",
};

export const MIMO_TOKEN_PLAN_ENDPOINTS: ProviderEndpointUrls = {
  baseUrl: "https://token-plan-cn.xiaomimimo.com/v1",
  anthropicBaseUrl: "https://token-plan-cn.xiaomimimo.com/anthropic",
};

const KNOWN_MIMO_ENDPOINTS = new Set(
  [MIMO_PAYG_ENDPOINTS, MIMO_TOKEN_PLAN_ENDPOINTS].flatMap((urls) =>
    [urls.baseUrl, urls.anthropicBaseUrl].filter(Boolean) as string[]
  )
);

export function isMimoProviderType(type: string): boolean {
  const normalized = type.trim().toLowerCase();
  return normalized === "mimo" || normalized === "xiaomi" || normalized.includes("mimo");
}

export function firstApiKey(raw?: string | null): string {
  const value = raw?.trim() ?? "";
  if (!value) return "";
  if (value.startsWith("[")) {
    try {
      const keys = JSON.parse(value) as unknown;
      if (Array.isArray(keys)) {
        return keys.find((key) => typeof key === "string" && key.trim())?.trim() ?? "";
      }
    } catch {
      return value;
    }
  }
  return value;
}

export function getMimoEndpointsForKey(apiKey?: string | null): ProviderEndpointUrls | null {
  const key = firstApiKey(apiKey);
  if (key.startsWith("tp-")) return MIMO_TOKEN_PLAN_ENDPOINTS;
  if (key.startsWith("sk-")) return MIMO_PAYG_ENDPOINTS;
  return null;
}

export function isKnownMimoEndpointUrl(url?: string | null): boolean {
  return KNOWN_MIMO_ENDPOINTS.has(url?.trim() ?? "");
}

export function resolveProviderPresetForKey(
  type: string,
  apiKey?: string | null,
  preset: ProviderPreset | undefined = PROVIDER_PRESETS[type]
): ProviderPreset | undefined {
  if (!preset) return undefined;
  const mimoEndpoints = isMimoProviderType(type) ? getMimoEndpointsForKey(apiKey) : null;
  if (!mimoEndpoints) return preset;
  return {
    ...preset,
    baseUrl: mimoEndpoints.baseUrl,
    anthropicBaseUrl: mimoEndpoints.anthropicBaseUrl,
  };
}

export function resolveKnownProviderEndpoints(
  type: string,
  apiKey?: string | null
): ProviderEndpointUrls | null {
  return isMimoProviderType(type) ? getMimoEndpointsForKey(apiKey) : null;
}

export const PROVIDER_PRESETS: Record<string, ProviderPreset> = {
  // Tier 1: Major providers
  anthropic: {
    baseUrl: "https://api.anthropic.com",
    protocols: ["anthropic_messages"],
    defaultModel: "claude-sonnet-4-6",
  },
  deepseek: {
    baseUrl: "https://api.deepseek.com",
    protocols: ["openai_chat_completions"],
    defaultModel: "deepseek-v4-flash",
    reasoningModel: "deepseek-v4-pro",
    anthropicBaseUrl: "https://api.deepseek.com/anthropic",
  },
  openai: {
    baseUrl: "https://api.openai.com",
    protocols: ["openai_chat_completions", "openai_responses"],
    defaultModel: "gpt-4o",
    responsesBaseUrl: "https://api.openai.com",
  },
  google_gemini: {
    baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
    protocols: ["openai_chat_completions"],
    defaultModel: "gemini-2.5-flash",
  },
  xai: {
    baseUrl: "https://api.x.ai",
    protocols: ["openai_chat_completions"],
    defaultModel: "grok-3-latest",
  },
  mistral: {
    baseUrl: "https://api.mistral.ai",
    protocols: ["openai_chat_completions"],
    defaultModel: "mistral-large-latest",
  },
  // Tier 2: Inference providers
  groq: {
    baseUrl: "https://api.groq.com/openai",
    protocols: ["openai_chat_completions"],
    defaultModel: "llama-3.3-70b-versatile",
  },
  together: {
    baseUrl: "https://api.together.xyz",
    protocols: ["openai_chat_completions"],
    defaultModel: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
  },
  fireworks: {
    baseUrl: "https://api.fireworks.ai/inference",
    protocols: ["openai_chat_completions"],
    defaultModel: "accounts/fireworks/models/llama-v3p1-70b-instruct",
  },
  cerebras: {
    baseUrl: "https://api.cerebras.ai",
    protocols: ["openai_chat_completions"],
    defaultModel: "llama-3.3-70b",
  },
  perplexity: {
    baseUrl: "https://api.perplexity.ai",
    protocols: ["openai_chat_completions"],
    defaultModel: "sonar-pro",
  },
  cohere: {
    baseUrl: "https://api.cohere.com/compatibility",
    protocols: ["openai_chat_completions"],
    defaultModel: "command-r-plus",
  },
  // China providers
  mimo: {
    baseUrl: MIMO_PAYG_ENDPOINTS.baseUrl,
    protocols: ["openai_chat_completions"],
    defaultModel: "mimo-v2.5-pro",
    reasoningModel: "mimo-v2.5-pro",
    anthropicBaseUrl: MIMO_PAYG_ENDPOINTS.anthropicBaseUrl,
  },
  kimi: {
    baseUrl: "https://api.moonshot.cn",
    protocols: ["openai_chat_completions"],
    defaultModel: "kimi-k2",
    extraHeaders: '{"User-Agent":"KimiCLI/1.40.0"}',
  },
  minimax: {
    baseUrl: "https://api.minimax.chat",
    protocols: ["openai_chat_completions"],
    defaultModel: "MiniMax-M1",
  },
  glm: {
    baseUrl: "https://open.bigmodel.cn/api/paas/v4/chat/completions",
    protocols: ["openai_chat_completions"],
    defaultModel: "glm-4-plus",
    anthropicBaseUrl: "https://open.bigmodel.cn/api/anthropic",
  },
  dashscope: {
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode",
    protocols: ["openai_chat_completions"],
    defaultModel: "qwen-max",
    anthropicBaseUrl: "https://dashscope.aliyuncs.com/apps/anthropic",
  },
  siliconflow: {
    baseUrl: "https://api.siliconflow.cn",
    protocols: ["openai_chat_completions"],
    defaultModel: "deepseek-ai/DeepSeek-V3",
  },
  volcengine: {
    baseUrl: "https://ark.cn-beijing.volces.com/api/v3/chat/completions",
    protocols: ["openai_chat_completions"],
    defaultModel: "doubao-pro-256k",
  },
  baichuan: {
    baseUrl: "https://api.baichuan-ai.com",
    protocols: ["openai_chat_completions"],
    defaultModel: "Baichuan4",
  },
  stepfun: {
    baseUrl: "https://api.stepfun.com",
    protocols: ["openai_chat_completions"],
    defaultModel: "step-2-16k",
  },
  yi: {
    baseUrl: "https://api.lingyiwanwu.com",
    protocols: ["openai_chat_completions"],
    defaultModel: "yi-large",
  },
  // Aggregators
  openrouter: {
    baseUrl: "https://openrouter.ai/api",
    protocols: ["openai_chat_completions"],
    defaultModel: "deepseek/deepseek-v4-flash",
  },
  // Custom
  custom_openai_compatible: {
    baseUrl: "",
    protocols: ["openai_chat_completions"],
    defaultModel: "",
  },
};
