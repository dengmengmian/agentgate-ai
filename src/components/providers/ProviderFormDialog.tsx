import { useState, useEffect } from "react";
import { X, RefreshCcw, Loader2, Sparkles } from "lucide-react";
import { PROVIDER_TYPES, PROTOCOLS, ALL_CAPABILITIES, CAPABILITY_LABELS } from "@/types/provider";
import { useI18n } from "@/lib/i18n";
import { toast } from "@/components/common/Toast";
import * as api from "@/lib/api";
import type {
  ProviderView,
  CreateProviderInput,
  UpdateProviderInput,
} from "@/types/provider";

interface ProviderFormDialogProps {
  open: boolean;
  provider?: ProviderView | null;
  onSubmit: (data: CreateProviderInput | UpdateProviderInput) => void;
  onClose: () => void;
}

export function ProviderFormDialog({
  open,
  provider,
  onSubmit,
  onClose,
}: ProviderFormDialogProps) {
  const { t } = useI18n();
  const isEdit = !!provider;

  const [name, setName] = useState("");
  const [providerType, setProviderType] = useState("deepseek");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKeys, setApiKeys] = useState<string[]>([""]);
  const [defaultModel, setDefaultModel] = useState("");
  const [reasoningModel, setReasoningModel] = useState("");
  const [supportedModels, setSupportedModels] = useState("");
  const [modelCapabilities, setModelCapabilities] = useState<Record<string, string[]>>({});
  const [seedingCaps, setSeedingCaps] = useState(false);
  const [modelMapping, setModelMapping] = useState<Record<string, string>>({});
  const [extraHeaders, setExtraHeaders] = useState("");
  const [anthropicBaseUrl, setAnthropicBaseUrl] = useState("");
  const [responsesBaseUrl, setResponsesBaseUrl] = useState("");
  const [autoCacheControl, setAutoCacheControl] = useState(true);
  const [protocols, setProtocols] = useState<string[]>(["openai_chat_completions"]);
  const [timeoutSeconds, setTimeoutSeconds] = useState("120");
  const [enabled, setEnabled] = useState(true);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [fetchingModels, setFetchingModels] = useState(false);
  const [newMappingClient, setNewMappingClient] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [quickMode, setQuickMode] = useState(true);
  const [quickKey, setQuickKey] = useState("");
  const [detectedType, setDetectedType] = useState<string | null>(null);

  // Presets per provider type
  const PROVIDER_PRESETS: Record<string, { baseUrl: string; protocols: string[]; defaultModel: string; reasoningModel?: string; anthropicBaseUrl?: string; responsesBaseUrl?: string; extraHeaders?: string }> = {
    // Tier 1: Major providers
    anthropic: { baseUrl: "https://api.anthropic.com", protocols: ["anthropic_messages"], defaultModel: "claude-sonnet-4-6" },
    deepseek: { baseUrl: "https://api.deepseek.com", protocols: ["openai_chat_completions"], defaultModel: "deepseek-v4-flash", reasoningModel: "deepseek-v4-pro", anthropicBaseUrl: "https://api.deepseek.com/anthropic" },
    openai: { baseUrl: "https://api.openai.com", protocols: ["openai_chat_completions", "openai_responses"], defaultModel: "gpt-4o", responsesBaseUrl: "https://api.openai.com" },
    google_gemini: { baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions", protocols: ["openai_chat_completions"], defaultModel: "gemini-2.5-flash" },
    xai: { baseUrl: "https://api.x.ai", protocols: ["openai_chat_completions"], defaultModel: "grok-3-latest" },
    mistral: { baseUrl: "https://api.mistral.ai", protocols: ["openai_chat_completions"], defaultModel: "mistral-large-latest" },
    // Tier 2: Inference providers
    groq: { baseUrl: "https://api.groq.com/openai", protocols: ["openai_chat_completions"], defaultModel: "llama-3.3-70b-versatile" },
    together: { baseUrl: "https://api.together.xyz", protocols: ["openai_chat_completions"], defaultModel: "meta-llama/Llama-3.3-70B-Instruct-Turbo" },
    fireworks: { baseUrl: "https://api.fireworks.ai/inference", protocols: ["openai_chat_completions"], defaultModel: "accounts/fireworks/models/llama-v3p1-70b-instruct" },
    cerebras: { baseUrl: "https://api.cerebras.ai", protocols: ["openai_chat_completions"], defaultModel: "llama-3.3-70b" },
    perplexity: { baseUrl: "https://api.perplexity.ai", protocols: ["openai_chat_completions"], defaultModel: "sonar-pro" },
    cohere: { baseUrl: "https://api.cohere.com/compatibility", protocols: ["openai_chat_completions"], defaultModel: "command-r-plus" },
    // China providers
    mimo: { baseUrl: "https://api.xiaomimimo.com/v1", protocols: ["openai_chat_completions"], defaultModel: "mimo-v2.5-pro", reasoningModel: "mimo-v2.5-pro", anthropicBaseUrl: "https://api.xiaomimimo.com/anthropic" },
    kimi: { baseUrl: "https://api.moonshot.cn", protocols: ["openai_chat_completions"], defaultModel: "kimi-k2", extraHeaders: '{"User-Agent":"KimiCLI/1.40.0"}' },
    minimax: { baseUrl: "https://api.minimax.chat", protocols: ["openai_chat_completions"], defaultModel: "MiniMax-M1" },
    glm: { baseUrl: "https://open.bigmodel.cn/api/paas/v4/chat/completions", protocols: ["openai_chat_completions"], defaultModel: "glm-4-plus", anthropicBaseUrl: "https://open.bigmodel.cn/api/anthropic" },
    dashscope: { baseUrl: "https://dashscope.aliyuncs.com/compatible-mode", protocols: ["openai_chat_completions"], defaultModel: "qwen-max", anthropicBaseUrl: "https://dashscope.aliyuncs.com/apps/anthropic" },
    siliconflow: { baseUrl: "https://api.siliconflow.cn", protocols: ["openai_chat_completions"], defaultModel: "deepseek-ai/DeepSeek-V3" },
    volcengine: { baseUrl: "https://ark.cn-beijing.volces.com/api/v3/chat/completions", protocols: ["openai_chat_completions"], defaultModel: "doubao-pro-256k" },
    baichuan: { baseUrl: "https://api.baichuan-ai.com", protocols: ["openai_chat_completions"], defaultModel: "Baichuan4" },
    stepfun: { baseUrl: "https://api.stepfun.com", protocols: ["openai_chat_completions"], defaultModel: "step-2-16k" },
    yi: { baseUrl: "https://api.lingyiwanwu.com", protocols: ["openai_chat_completions"], defaultModel: "yi-large" },
    // Aggregators
    openrouter: { baseUrl: "https://openrouter.ai/api", protocols: ["openai_chat_completions"], defaultModel: "deepseek/deepseek-v4-flash" },
    // Custom
    custom_openai_compatible: { baseUrl: "", protocols: ["openai_chat_completions"], defaultModel: "" },
  };

  // MiMo runs two host pairs depending on the key tier:
  //   sk-* (按量付费) → api.xiaomimimo.com
  //   tp-* (Token Plan) → token-plan-cn.xiaomimimo.com
  // Cross-host requests 401, so we swap baseUrl + anthropicBaseUrl together
  // based on the key prefix.
  const MIMO_PAYG = { baseUrl: "https://api.xiaomimimo.com/v1", anthropicBaseUrl: "https://api.xiaomimimo.com/anthropic" };
  const MIMO_TOKEN_PLAN = { baseUrl: "https://token-plan-cn.xiaomimimo.com/v1", anthropicBaseUrl: "https://token-plan-cn.xiaomimimo.com/anthropic" };
  const mimoUrlsFromKey = (key: string) =>
    key.startsWith("tp-") ? MIMO_TOKEN_PLAN : MIMO_PAYG;
  const isKnownMimoUrl = (url: string) =>
    url === MIMO_PAYG.baseUrl || url === MIMO_TOKEN_PLAN.baseUrl
      || url === MIMO_PAYG.anthropicBaseUrl || url === MIMO_TOKEN_PLAN.anthropicBaseUrl;

  const applyPreset = (type: string) => {
    const preset = PROVIDER_PRESETS[type];
    if (!preset || isEdit) return;
    setBaseUrl(preset.baseUrl);
    setProtocols(preset.protocols);
    setDefaultModel(preset.defaultModel);
    setReasoningModel(preset.reasoningModel ?? "");
    setAnthropicBaseUrl(preset.anthropicBaseUrl ?? "");
    setResponsesBaseUrl(preset.responsesBaseUrl ?? "");
    setExtraHeaders(preset.extraHeaders ?? "");
    // Auto-fill name if empty
    const typeLabel = PROVIDER_TYPES.find(t => t.value === type)?.label;
    if (!name && typeLabel) setName(typeLabel);
  };

  // MiMo-specific: when user enters a tp-* key after picking MiMo from the
  // dropdown, swap to the token-plan host. Only swap if the current URL is
  // one of the two known MiMo URLs (don't clobber custom edits).
  useEffect(() => {
    if (isEdit || providerType !== "mimo") return;
    const key = (apiKeys[0] || "").trim();
    if (!key || (!key.startsWith("tp-") && !key.startsWith("sk-"))) return;
    const target = mimoUrlsFromKey(key);
    if (isKnownMimoUrl(baseUrl) && baseUrl !== target.baseUrl) setBaseUrl(target.baseUrl);
    if (isKnownMimoUrl(anthropicBaseUrl) && anthropicBaseUrl !== target.anthropicBaseUrl) {
      setAnthropicBaseUrl(target.anthropicBaseUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [providerType, apiKeys, isEdit]);

  // API Key auto-detection
  const detectProviderFromKey = (key: string): string | null => {
    const k = key.trim();
    if (!k) return null;
    if (k.startsWith("sk-ant-")) return "anthropic";
    if (k.startsWith("tp-")) return "mimo";  // MiMo token-plan keys
    if (k.startsWith("deepseek-")) return "deepseek";
    if (k.startsWith("sk-or-")) return "openrouter";
    if (k.startsWith("gsk_")) return "groq";
    if (k.startsWith("xai-")) return "xai";
    if (k.startsWith("pplx-")) return "perplexity";
    // Generic sk- could be OpenAI, SiliconFlow, Kimi, etc.
    // Default to OpenAI for sk- prefix
    if (k.startsWith("sk-")) return "openai";
    return null;
  };

  const handleQuickKeyChange = (key: string) => {
    setQuickKey(key);
    setDetectedType(detectProviderFromKey(key));
  };

  const handleQuickCreate = () => {
    const type = detectedType;
    if (!type || !quickKey.trim()) return;
    // Apply preset and fill API key
    setProviderType(type);
    applyPreset(type);
    setApiKeys([quickKey.trim()]);
    // Submit directly
    const preset = PROVIDER_PRESETS[type];
    if (!preset) return;
    const typeLabel = PROVIDER_TYPES.find(t => t.value === type)?.label ?? type;
    // MiMo: tp-* keys must hit the token-plan host (cross-host → 401).
    const mimoOverride = type === "mimo" ? mimoUrlsFromKey(quickKey.trim()) : null;
    onSubmit({
      name: typeLabel,
      provider_type: type,
      base_url: mimoOverride?.baseUrl ?? preset.baseUrl,
      api_key: quickKey.trim(),
      default_model: preset.defaultModel,
      reasoning_model: preset.reasoningModel ?? null,
      protocol: JSON.stringify(preset.protocols),
      timeout_seconds: 120,
      enabled: true,
      anthropic_base_url: mimoOverride?.anthropicBaseUrl ?? preset.anthropicBaseUrl ?? null,
      responses_base_url: preset.responsesBaseUrl ?? null,
      extra_headers: preset.extraHeaders ?? null,
      auto_cache_control: true,
    } as CreateProviderInput);
  };

  useEffect(() => {
    if (provider) {
      setName(provider.name);
      setProviderType(provider.provider_type);
      setBaseUrl(provider.base_url);
      setApiKeys([""]);
      setDefaultModel(provider.default_model);
      setReasoningModel(provider.reasoning_model ?? "");
      setSupportedModels(provider.supported_models ?? "");
      try { setModelCapabilities(provider.model_capabilities ? JSON.parse(provider.model_capabilities) : {}); } catch { setModelCapabilities({}); }
      try { setModelMapping(provider.model_mapping ? JSON.parse(provider.model_mapping) : {}); } catch { setModelMapping({}); }
      setExtraHeaders(provider.extra_headers ?? "");
      setAnthropicBaseUrl(provider.anthropic_base_url ?? "");
      setResponsesBaseUrl(provider.responses_base_url ?? "");
      setAutoCacheControl(provider.auto_cache_control ?? true);
      try { setProtocols(JSON.parse(provider.protocol)); } catch { setProtocols([provider.protocol]); }
      setTimeoutSeconds(String(provider.timeout_seconds));
      setEnabled(provider.enabled);
    } else {
      setName("");
      setProviderType("deepseek");
      setBaseUrl("");
      setApiKeys([""]);
      setDefaultModel("");
      setReasoningModel("");
      setSupportedModels("");
      setModelCapabilities({});
      setModelMapping({});
      setExtraHeaders("");
      setAnthropicBaseUrl("");
      setResponsesBaseUrl("");
      setAutoCacheControl(true);
      setProtocols(["openai_chat_completions"]);
      setTimeoutSeconds("120");
      setEnabled(true);
    }
    setErrors({});
    setQuickMode(!provider);
    setQuickKey("");
    setDetectedType(null);
  }, [provider, open]);

  const validate = (): boolean => {
    const errs: Record<string, string> = {};
    if (!name.trim()) errs.name = t("providers.name") + " required";
    if (!baseUrl.trim()) errs.baseUrl = t("providers.base_url") + " required";
    if (!defaultModel.trim()) errs.defaultModel = t("providers.default_model") + " required";
    const tv = parseInt(timeoutSeconds, 10);
    if (isNaN(tv) || tv <= 0) errs.timeoutSeconds = "> 0";
    setErrors(errs);
    return Object.keys(errs).length === 0;
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;

    const mmStr = Object.keys(modelMapping).length > 0 ? JSON.stringify(modelMapping) : undefined;
    const smStr = supportedModels || undefined;
    const ehStr = extraHeaders || undefined;
    const abuStr = anthropicBaseUrl || undefined;
    const rbuStr = responsesBaseUrl || undefined;
    const mcStr = Object.keys(modelCapabilities).length > 0 ? JSON.stringify(modelCapabilities) : undefined;

    if (isEdit) {
      const input: UpdateProviderInput = {
        name, provider_type: providerType, base_url: baseUrl,
        default_model: defaultModel, reasoning_model: reasoningModel || undefined,
        supported_models: smStr, model_mapping: mmStr, extra_headers: ehStr, anthropic_base_url: abuStr, responses_base_url: rbuStr,
        auto_cache_control: autoCacheControl,
        model_capabilities: mcStr,
        protocol: JSON.stringify(protocols), timeout_seconds: parseInt(timeoutSeconds, 10), enabled,
      };
      const validKeys = apiKeys.filter(k => k.trim());
      if (validKeys.length === 1) input.api_key = validKeys[0];
      else if (validKeys.length > 1) input.api_key = JSON.stringify(validKeys);
      onSubmit(input);
    } else {
      const input: CreateProviderInput = {
        name, provider_type: providerType, base_url: baseUrl,
        default_model: defaultModel, reasoning_model: reasoningModel || undefined,
        supported_models: smStr, model_mapping: mmStr, extra_headers: ehStr, anthropic_base_url: abuStr, responses_base_url: rbuStr,
        auto_cache_control: autoCacheControl,
        model_capabilities: mcStr,
        protocol: JSON.stringify(protocols), timeout_seconds: parseInt(timeoutSeconds, 10), enabled,
      };
      const validKeys = apiKeys.filter(k => k.trim());
      if (validKeys.length === 1) input.api_key = validKeys[0];
      else if (validKeys.length > 1) input.api_key = JSON.stringify(validKeys);
      onSubmit(input);
    }
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative z-10 w-full max-w-lg max-h-[90vh] overflow-y-auto rounded-lg border border-border bg-card shadow-xl">
        <div className="flex items-center justify-between border-b border-border px-6 py-4">
          <h2 className="text-sm font-semibold text-text-primary">
            {isEdit ? t("providers.edit") : t("providers.add")}
          </h2>
          <button onClick={onClose} className="rounded-md p-1.5 text-text-muted transition-colors hover:bg-card-secondary hover:text-text-primary">
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Quick mode — paste API key to auto-create */}
        {quickMode && !isEdit && (
          <div className="p-6 space-y-4">
            <div>
              <p className="text-xs text-text-muted mb-3">{t("providers.quick_add_hint")}</p>
              <input
                value={quickKey}
                onChange={(e) => handleQuickKeyChange(e.target.value)}
                placeholder="sk-xxx / deepseek-xxx / sk-ant-xxx ..."
                className="form-input text-sm"
                autoFocus
              />
            </div>
            {detectedType && (
              <div className="flex items-center justify-between rounded-xl border border-accent/30 bg-accent-soft p-4">
                <div>
                  <p className="text-sm font-medium text-text-primary">
                    {PROVIDER_TYPES.find(t => t.value === detectedType)?.label}
                  </p>
                  <p className="text-xs text-text-muted">
                    {PROVIDER_PRESETS[detectedType]?.defaultModel}
                  </p>
                </div>
                <button type="button" onClick={handleQuickCreate} className="btn-primary">
                  {t("providers.create")}
                </button>
              </div>
            )}
            {quickKey && !detectedType && (
              <p className="text-xs text-text-muted">{t("providers.quick_add_unknown")}</p>
            )}
            <button
              type="button"
              onClick={() => setQuickMode(false)}
              className="text-xs text-accent hover:underline"
            >
              {t("providers.manual_setup")}
            </button>
          </div>
        )}

        <form onSubmit={handleSubmit} className={`space-y-4 p-6 ${quickMode && !isEdit ? "hidden" : ""}`}>
          {/* Provider type — auto-fills everything below */}
          <Field label={t("providers.type")}>
            <select value={providerType} onChange={(e) => { setProviderType(e.target.value); applyPreset(e.target.value); }} className="form-input">
              {PROVIDER_TYPES.map((tp) => (
                <option key={tp.value} value={tp.value}>{tp.label}</option>
              ))}
            </select>
          </Field>

          <div className="grid grid-cols-2 gap-4">
            <Field label={t("providers.name")} error={errors.name}>
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="My Provider" className="form-input" />
            </Field>
            <Field label={t("providers.base_url")} error={errors.baseUrl}>
              <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.example.com" className="form-input" />
            </Field>
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-text-secondary">
              {t("providers.api_key")} {apiKeys.filter(k => k).length > 1 && <span className="text-text-muted">({apiKeys.filter(k => k).length} keys)</span>}
            </label>
            {isEdit && provider?.masked_api_key && <p className="mb-1 text-[11px] text-text-muted">Current: {provider.masked_api_key}</p>}
            <div className="space-y-1.5">
              {apiKeys.map((key, i) => (
                <div key={i} className="flex items-center gap-1.5">
                  <input
                    type="password"
                    value={key}
                    onChange={(e) => { const next = [...apiKeys]; next[i] = e.target.value; setApiKeys(next); }}
                    placeholder={i === 0 ? "sk-..." : `Key ${i + 1}`}
                    className="form-input flex-1"
                  />
                  {apiKeys.length > 1 && (
                    <button type="button" onClick={() => setApiKeys(apiKeys.filter((_, j) => j !== i))} className="text-text-muted hover:text-error text-xs">&times;</button>
                  )}
                </div>
              ))}
            </div>
            <button type="button" onClick={() => setApiKeys([...apiKeys, ""])} className="mt-1.5 text-[11px] text-accent hover:text-accent/80">+ {t("providers.add_key")}</button>
          </div>

          {/* Supported Models — fetch from provider */}
          <div>
            <div className="mb-1 flex items-center justify-between">
              <label className="text-xs font-medium text-text-secondary">{t("providers.supported_models")}</label>
              {isEdit && provider && (
                <button type="button" disabled={fetchingModels} onClick={async () => {
                  setFetchingModels(true);
                  try {
                    // Auto-save API key / base_url before fetching so backend has latest values
                    const saveInput: UpdateProviderInput = {};
                    const vk = apiKeys.filter(k => k.trim());
                    if (vk.length === 1) saveInput.api_key = vk[0];
                    else if (vk.length > 1) saveInput.api_key = JSON.stringify(vk);
                    if (baseUrl && baseUrl !== provider.base_url) saveInput.base_url = baseUrl;
                    if (Object.keys(saveInput).length > 0) {
                      await api.updateProvider(provider.id, saveInput);
                    }
                    const models = await api.fetchProviderModels(provider.id);
                    setSupportedModels(JSON.stringify(models));
                    toast("success", `${models.length} models`);
                  } catch (err) { toast("error", (err as api.AppError).message); }
                  finally { setFetchingModels(false); }
                }} className="flex items-center gap-1 text-[11px] text-accent hover:text-accent/80">
                  {fetchingModels ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCcw className="h-3 w-3" />}
                  {t("providers.fetch_models")}
                </button>
              )}
            </div>
            {(() => {
              let models: string[] = [];
              try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
              return models.length > 0 ? (
                <div className="flex flex-wrap gap-1.5 rounded-md border border-border bg-card-secondary p-2">
                  {models.map((m) => (
                    <span key={m} className="flex items-center gap-1 rounded bg-bg px-2 py-0.5 font-mono text-[11px] text-text-secondary">
                      {m}
                      <button type="button" onClick={() => {
                        const next = models.filter(x => x !== m);
                        setSupportedModels(next.length > 0 ? JSON.stringify(next) : "");
                      }} className="text-text-muted hover:text-error">&times;</button>
                    </span>
                  ))}
                </div>
              ) : (
                <p className="text-[11px] text-text-muted">{isEdit ? t("providers.fetch_models_hint") : t("providers.supported_models_hint")}</p>
              );
            })()}
          </div>

          {/* Model Capability Matrix — per-model capability flags for routing AND for card icons */}
          {(() => {
            let models: string[] = [];
            try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
            // Merge in default_model / reasoning_model so single-model providers
            // still get a row to configure (e.g. Kimi Code with only kimi-for-coding).
            // Without the matrix, capability icons fall back to the legacy probe
            // which is unreliable for non-OpenAI-style providers.
            const merged = new Set(models);
            if (defaultModel) merged.add(defaultModel);
            if (reasoningModel) merged.add(reasoningModel);
            models = Array.from(merged);
            if (models.length < 1) return null;
            return (
              <div>
                <div className="mb-1 flex items-center justify-between">
                  <label className="text-xs font-medium text-text-secondary">能力矩阵</label>
                  <button
                    type="button"
                    disabled={seedingCaps}
                    onClick={async () => {
                      setSeedingCaps(true);
                      try {
                        const seeded = await api.seedModelCapabilities(providerType, models);
                        setModelCapabilities(seeded);
                        toast("success", "已根据模型名自动识别能力");
                      } catch (err) {
                        toast("error", (err as api.AppError).message);
                      } finally {
                        setSeedingCaps(false);
                      }
                    }}
                    className="flex items-center gap-1 text-[11px] text-accent hover:text-accent/80 disabled:opacity-50"
                  >
                    {seedingCaps ? <Loader2 className="h-3 w-3 animate-spin" /> : <Sparkles className="h-3 w-3" />}
                    自动识别
                  </button>
                </div>
                <p className="mb-2 text-[11px] text-text-muted">
                  勾选每个模型支持的能力。请求带图片时网关会自动路由到支持 vision 的模型。
                </p>
                <div className="overflow-x-auto rounded-md border border-border bg-card-secondary">
                  <table className="w-full text-[11px]">
                    <thead>
                      <tr className="border-b border-border bg-bg/50">
                        <th className="sticky left-0 z-10 bg-bg/50 px-2 py-1.5 text-left font-mono text-text-secondary">model</th>
                        {ALL_CAPABILITIES.map((cap) => (
                          <th key={cap} className="px-2 py-1.5 text-center font-medium text-text-secondary" title={cap}>
                            {CAPABILITY_LABELS[cap]}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {models.map((m) => {
                        const caps = modelCapabilities[m] ?? [];
                        return (
                          <tr key={m} className="border-b border-border last:border-0">
                            <td className="sticky left-0 z-10 bg-card-secondary px-2 py-1.5 font-mono text-text-primary">{m}</td>
                            {ALL_CAPABILITIES.map((cap) => (
                              <td key={cap} className="px-2 py-1.5 text-center">
                                <input
                                  type="checkbox"
                                  checked={caps.includes(cap)}
                                  onChange={(e) => {
                                    setModelCapabilities((prev) => {
                                      const next = { ...prev };
                                      const cur = new Set(next[m] ?? []);
                                      if (e.target.checked) cur.add(cap); else cur.delete(cap);
                                      next[m] = Array.from(cur).sort();
                                      return next;
                                    });
                                  }}
                                  className="h-3.5 w-3.5 cursor-pointer accent-accent"
                                />
                              </td>
                            ))}
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </div>
            );
          })()}

          {/* Default / Reasoning Model — dropdown if models available */}
          <div className="grid grid-cols-2 gap-4">
            <Field label={t("providers.default_model")} error={errors.defaultModel}>
              {(() => {
                let models: string[] = [];
                try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
                return models.length > 0 ? (
                  <select value={defaultModel} onChange={(e) => setDefaultModel(e.target.value)} className="form-input">
                    <option value="">--</option>
                    {models.map((m) => <option key={m} value={m}>{m}</option>)}
                  </select>
                ) : (
                  <input value={defaultModel} onChange={(e) => setDefaultModel(e.target.value)} placeholder="model-name" className="form-input" />
                );
              })()}
            </Field>
            <Field label={t("providers.reasoning_model")}>
              {(() => {
                let models: string[] = [];
                try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
                return models.length > 0 ? (
                  <select value={reasoningModel} onChange={(e) => setReasoningModel(e.target.value)} className="form-input">
                    <option value="">--</option>
                    {models.map((m) => <option key={m} value={m}>{m}</option>)}
                  </select>
                ) : (
                  <input value={reasoningModel} onChange={(e) => setReasoningModel(e.target.value)} placeholder="Optional" className="form-input" />
                );
              })()}
            </Field>
          </div>

          {/* Model Mapping */}
          <div>
            <label className="mb-1 block text-xs font-medium text-text-secondary">{t("providers.model_mapping")}</label>
            <p className="mb-2 text-[11px] text-text-muted">{t("providers.model_mapping_hint")}</p>
            <div className="space-y-1.5">
              {Object.entries(modelMapping).map(([clientModel, providerModel]) => (
                <div key={clientModel} className="flex items-center gap-2">
                  <input value={clientModel} onChange={(e) => {
                    const newKey = e.target.value;
                    if (newKey && newKey !== clientModel) {
                      const next: Record<string, string> = {};
                      for (const [k, v] of Object.entries(modelMapping)) {
                        next[k === clientModel ? newKey : k] = v;
                      }
                      setModelMapping(next);
                    }
                  }} className="form-input flex-1" />
                  <span className="text-text-muted">→</span>
                  <ModelCombo
                    value={providerModel}
                    onChange={(v) => setModelMapping({ ...modelMapping, [clientModel]: v })}
                    models={(() => { try { return supportedModels ? JSON.parse(supportedModels) : []; } catch { return []; } })()}
                  />
                  <button type="button" onClick={() => { const next = { ...modelMapping }; delete next[clientModel]; setModelMapping(next); }} className="text-text-muted hover:text-error text-xs">✕</button>
                </div>
              ))}
            </div>
            <div className="mt-2 flex gap-2">
              <ModelCombo
                value={newMappingClient}
                onChange={(v) => setNewMappingClient(v)}
                models={["gpt-5.5","gpt-5.4","gpt-5.4-mini","gpt-5.3-codex","gpt-5.2","claude-sonnet-4-6","claude-opus-4-6","claude-haiku-4-5-20251001","o3","o4-mini","gemini-2.5-flash","gemini-2.5-pro","gemini-3-pro-preview"].filter(m => !(m in modelMapping))}
                placeholder={t("providers.select_client_model")}
              />
              <button type="button" onClick={() => {
                if (newMappingClient.trim() && !(newMappingClient.trim() in modelMapping)) {
                  let models: string[] = [];
                  try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
                  setModelMapping({ ...modelMapping, [newMappingClient.trim()]: models[0] || defaultModel || "" });
                  setNewMappingClient("");
                }
              }} className="btn-secondary">{t("routes.add")}</button>
            </div>
          </div>

          {/* Advanced settings — collapsible */}
          <div>
            <button type="button" onClick={() => setShowAdvanced(!showAdvanced)} className="flex items-center gap-1 text-[11px] text-accent hover:text-accent/80">
              <span className={`transition-transform ${showAdvanced ? "rotate-90" : ""}`}>&#9654;</span>
              {t("providers.advanced_settings")}
            </button>
            {showAdvanced && (
              <div className="mt-3 space-y-4 rounded-md border border-border/50 bg-card-secondary p-4">
                <Field label={t("providers.protocol")} hint={t("providers.protocol_hint")}>
                  <div className="space-y-2">
                    {PROTOCOLS.map((p) => (
                      <label key={p.value} className="flex cursor-pointer items-center gap-2">
                        <input
                          type="checkbox"
                          checked={protocols.includes(p.value)}
                          onChange={(e) => {
                            if (e.target.checked) {
                              setProtocols([...protocols, p.value]);
                            } else {
                              const next = protocols.filter(x => x !== p.value);
                              if (next.length > 0) setProtocols(next);
                            }
                          }}
                          className="accent-accent"
                        />
                        <span className="text-xs text-text-secondary">{p.label}</span>
                      </label>
                    ))}
                  </div>
                </Field>

                <Field label={t("providers.anthropic_url")} hint={t("providers.anthropic_url_hint")}>
                  <input value={anthropicBaseUrl} onChange={(e) => setAnthropicBaseUrl(e.target.value)} placeholder="https://api.deepseek.com/anthropic" className="form-input" />
                </Field>

                <Field label={t("providers.responses_url")} hint={t("providers.responses_url_hint")}>
                  <input value={responsesBaseUrl} onChange={(e) => setResponsesBaseUrl(e.target.value)} placeholder="https://api.openai.com" className="form-input" />
                </Field>

                <Field label={t("providers.extra_headers")} hint={t("providers.extra_headers_hint")}>
                  <input value={extraHeaders} onChange={(e) => setExtraHeaders(e.target.value)} placeholder='{"User-Agent":"KimiCLI/1.40.0"}' className="form-input" />
                </Field>

                {/* Cache control toggle — show for Anthropic or providers with anthropic_base_url */}
                {(providerType === "anthropic" || anthropicBaseUrl) && (
                  <Field label={t("providers.auto_cache")} hint={providerType === "anthropic" ? t("providers.auto_cache_hint_native") : t("providers.auto_cache_hint_compat")}>
                    <label className="mt-1 flex cursor-pointer items-center gap-2">
                      <input type="checkbox" checked={autoCacheControl} onChange={(e) => setAutoCacheControl(e.target.checked)} className="accent-accent" />
                      <span className="text-xs text-text-secondary">
                        {autoCacheControl ? t("providers.enabled") : t("providers.disabled")}
                        {providerType === "anthropic" && <span className="ml-1 text-[10px] text-green-400">[{t("providers.recommended")}]</span>}
                        {providerType !== "anthropic" && anthropicBaseUrl && <span className="ml-1 text-[10px] text-yellow-400">[{t("providers.experimental")}]</span>}
                      </span>
                    </label>
                  </Field>
                )}

                <div className="grid grid-cols-2 gap-4">
                  <Field label={t("providers.timeout")} error={errors.timeoutSeconds}>
                    <input type="number" value={timeoutSeconds} onChange={(e) => setTimeoutSeconds(e.target.value)} min={1} className="form-input" />
                  </Field>
                  <Field label={t("providers.enabled")}>
                    <label className="mt-1.5 flex cursor-pointer items-center gap-2">
                      <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} className="accent-accent" />
                      <span className="text-xs text-text-secondary">
                        {enabled ? t("providers.enabled") : t("providers.disabled")}
                      </span>
                    </label>
                  </Field>
                </div>
              </div>
            )}
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <button type="button" onClick={onClose} className="rounded-md bg-card-secondary px-4 py-2 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary">
              {t("common.cancel")}
            </button>
            <button type="submit" className="rounded-md bg-accent px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-accent/90">
              {isEdit ? t("providers.save") : t("providers.create")}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function Field({ label, error, hint, children }: { label: string; error?: string; hint?: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="mb-1 block text-xs font-medium text-text-secondary">{label}</label>
      {children}
      {hint && <p className="mt-1 text-[11px] text-text-muted">{hint}</p>}
      {error && <p className="mt-1 text-[11px] text-error">{error}</p>}
    </div>
  );
}

function ModelCombo({ value, onChange, models, placeholder }: { value: string; onChange: (v: string) => void; models: string[]; placeholder?: string }) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const filtered = filter ? models.filter((m) => m.toLowerCase().includes(filter.toLowerCase())) : models;

  return (
    <div className="relative flex-1">
      <input
        value={value}
        onChange={(e) => { onChange(e.target.value); setFilter(e.target.value); setOpen(true); }}
        onFocus={() => { if (models.length > 0) setOpen(true); }}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        placeholder={placeholder || "model-name"}
        className="form-input w-full"
      />
      {open && filtered.length > 0 && (
        <ul className="absolute z-50 mt-1 max-h-40 w-full overflow-y-auto rounded-md border border-accent/30 bg-bg shadow-xl ring-1 ring-white/5">
          {filtered.map((m) => (
            <li
              key={m}
              onMouseDown={(e) => { e.preventDefault(); onChange(m); setFilter(""); setOpen(false); }}
              className={`cursor-pointer px-3 py-1.5 text-xs transition-colors hover:bg-accent/20 hover:text-accent ${m === value ? "bg-accent/15 text-accent font-medium" : "text-text-secondary"}`}
            >
              {m}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
