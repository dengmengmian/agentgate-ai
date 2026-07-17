/// 每个 provider 接入时的默认值，供 ProviderFormDialog（手动表单 / quick mode）
/// 和 QuickSetup（首次引导向导）共用。这里是**唯一权威源**：新增 provider 在
/// 这里加一条即可，两侧自动生效。
///
/// 历史上 ProviderFormDialog 和 QuickSetup 各维护了一份，QuickSetup 那份只有
/// 7 个 provider——`detectProviderType` 能识别 mimo / kimi 等，但 QuickSetup
/// 拿到这些 type 时会 undefined → crash。合并到这里顺手把这个潜在 bug 解决。
import {
  GENERATED_MIMO_ENDPOINTS,
  GENERATED_PROVIDER_PRESETS,
} from "./generatedProviderCatalog";

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
  ...GENERATED_MIMO_ENDPOINTS.payg,
};

export const MIMO_TOKEN_PLAN_ENDPOINTS: ProviderEndpointUrls = {
  ...GENERATED_MIMO_ENDPOINTS.tokenPlanRegions.cn,
};

export const MIMO_TOKEN_PLAN_ENDPOINTS_BY_REGION: Record<
  string,
  ProviderEndpointUrls
> = Object.fromEntries(
  Object.entries(GENERATED_MIMO_ENDPOINTS.tokenPlanRegions).map(
    ([region, endpoints]) => [region, { ...endpoints }]
  )
);

const KNOWN_MIMO_ENDPOINTS = new Set(
  [
    MIMO_PAYG_ENDPOINTS,
    ...Object.values(MIMO_TOKEN_PLAN_ENDPOINTS_BY_REGION),
  ].flatMap(
    (urls) => [urls.baseUrl, urls.anthropicBaseUrl].filter(Boolean) as string[]
  )
);

export function isMimoProviderType(type: string): boolean {
  const normalized = type.trim().toLowerCase();
  return (
    normalized === "mimo" ||
    normalized === "xiaomi" ||
    normalized.includes("mimo")
  );
}

export function firstApiKey(raw?: string | null): string {
  const value = raw?.trim() ?? "";
  if (!value) return "";
  if (value.startsWith("[")) {
    try {
      const keys = JSON.parse(value) as unknown;
      if (Array.isArray(keys)) {
        return (
          keys.find((key) => typeof key === "string" && key.trim())?.trim() ??
          ""
        );
      }
    } catch {
      return value;
    }
  }
  return value;
}

export function getMimoEndpointsForKey(
  apiKey?: string | null
): ProviderEndpointUrls | null {
  const key = firstApiKey(apiKey);
  if (key.startsWith("tp-")) return MIMO_TOKEN_PLAN_ENDPOINTS;
  if (key.startsWith("sk-")) return MIMO_PAYG_ENDPOINTS;
  return null;
}

export function getMimoEndpointsForKeyAndUrl(
  apiKey?: string | null,
  baseUrl?: string | null,
  anthropicBaseUrl?: string | null
): ProviderEndpointUrls | null {
  const key = firstApiKey(apiKey);
  if (key.startsWith("sk-")) return MIMO_PAYG_ENDPOINTS;
  if (!key.startsWith("tp-")) return null;
  const region = getPreferredMimoTokenPlanRegion(baseUrl, anthropicBaseUrl);
  return (
    MIMO_TOKEN_PLAN_ENDPOINTS_BY_REGION[region] ?? MIMO_TOKEN_PLAN_ENDPOINTS
  );
}

export function isKnownMimoEndpointUrl(url?: string | null): boolean {
  return KNOWN_MIMO_ENDPOINTS.has(url?.trim() ?? "");
}

function getMimoTokenPlanRegion(url?: string | null): string | null {
  const value = url?.trim().replace(/\/$/, "") ?? "";
  for (const [region, endpoints] of Object.entries(
    MIMO_TOKEN_PLAN_ENDPOINTS_BY_REGION
  )) {
    if (value === endpoints.baseUrl || value === endpoints.anthropicBaseUrl) {
      return region;
    }
  }
  return null;
}

function getPreferredMimoTokenPlanRegion(
  baseUrl?: string | null,
  anthropicBaseUrl?: string | null
): string {
  const baseRegion = getMimoTokenPlanRegion(baseUrl);
  const anthropicRegion = getMimoTokenPlanRegion(anthropicBaseUrl);
  if (baseRegion && (baseRegion !== "cn" || !anthropicRegion)) {
    return baseRegion;
  }
  return anthropicRegion ?? baseRegion ?? "cn";
}

/** Platform (pay-as-you-go) vs Kimi Code (membership) endpoints. */
export const KIMI_PLATFORM_ENDPOINTS: ProviderEndpointUrls = {
  baseUrl: "https://api.moonshot.cn",
  anthropicBaseUrl: "https://api.moonshot.cn/anthropic",
};

export const KIMI_CODE_ENDPOINTS: ProviderEndpointUrls = {
  baseUrl: "https://api.kimi.com/coding/v1",
  anthropicBaseUrl: "https://api.kimi.com/coding",
};

export function isKimiProviderType(type: string): boolean {
  const normalized = type.trim().toLowerCase();
  return (
    normalized === "kimi" ||
    normalized === "moonshot" ||
    normalized.includes("moonshot")
  );
}

/** Kimi Code console keys use the `sk-kimi-` prefix; Platform keys are bare sk-…. */
export function isKimiCodeApiKey(apiKey?: string | null): boolean {
  return firstApiKey(apiKey).startsWith("sk-kimi-");
}

export function getKimiEndpointsForKey(
  apiKey?: string | null
): ProviderEndpointUrls | null {
  if (isKimiCodeApiKey(apiKey)) return KIMI_CODE_ENDPOINTS;
  // Classic Moonshot / Platform key or unknown — keep catalog Platform host.
  if (firstApiKey(apiKey)) return KIMI_PLATFORM_ENDPOINTS;
  return null;
}

export function resolveProviderPresetForKey(
  type: string,
  apiKey?: string | null,
  preset: ProviderPreset | undefined = PROVIDER_PRESETS[type]
): ProviderPreset | undefined {
  if (!preset) return undefined;
  const mimoEndpoints = isMimoProviderType(type)
    ? getMimoEndpointsForKey(apiKey)
    : null;
  if (mimoEndpoints) {
    return {
      ...preset,
      baseUrl: mimoEndpoints.baseUrl,
      anthropicBaseUrl: mimoEndpoints.anthropicBaseUrl,
    };
  }
  if (isKimiProviderType(type)) {
    const kimiEndpoints = getKimiEndpointsForKey(apiKey);
    if (!kimiEndpoints) return preset;
    // Code membership uses short model IDs (k3 / kimi-for-coding…);
    // Platform uses kimi-k3 / kimi-k2.7-code… — catalog defaults stay Platform.
    if (isKimiCodeApiKey(apiKey)) {
      return {
        ...preset,
        baseUrl: kimiEndpoints.baseUrl,
        anthropicBaseUrl: kimiEndpoints.anthropicBaseUrl,
        defaultModel: "k3",
        reasoningModel: "k3",
      };
    }
    return {
      ...preset,
      baseUrl: kimiEndpoints.baseUrl,
      anthropicBaseUrl: kimiEndpoints.anthropicBaseUrl,
    };
  }
  return preset;
}

export function resolveKnownProviderEndpoints(
  type: string,
  apiKey?: string | null,
  baseUrl?: string | null,
  anthropicBaseUrl?: string | null
): ProviderEndpointUrls | null {
  if (isMimoProviderType(type)) {
    return getMimoEndpointsForKeyAndUrl(apiKey, baseUrl, anthropicBaseUrl);
  }
  if (isKimiProviderType(type)) {
    return getKimiEndpointsForKey(apiKey);
  }
  return null;
}

export const PROVIDER_PRESETS: Record<string, ProviderPreset> = {
  ...Object.fromEntries(
    Object.entries(GENERATED_PROVIDER_PRESETS).map(([type, rawPreset]) => {
      const preset = rawPreset as unknown as ProviderPreset;
      return [
        type,
        {
          baseUrl: preset.baseUrl,
          protocols: [...preset.protocols],
          defaultModel: preset.defaultModel,
          ...(preset.reasoningModel
            ? { reasoningModel: preset.reasoningModel }
            : {}),
          ...(preset.anthropicBaseUrl
            ? { anthropicBaseUrl: preset.anthropicBaseUrl }
            : {}),
          ...(preset.responsesBaseUrl
            ? { responsesBaseUrl: preset.responsesBaseUrl }
            : {}),
          ...(preset.extraHeaders ? { extraHeaders: preset.extraHeaders } : {}),
        },
      ];
    })
  ),
};
