import { useState, useEffect } from "react";
import { X, RefreshCcw, Loader2 } from "lucide-react";
import { PROVIDER_TYPES, PROTOCOLS } from "@/types/provider";
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
  const [apiKey, setApiKey] = useState("");
  const [defaultModel, setDefaultModel] = useState("");
  const [reasoningModel, setReasoningModel] = useState("");
  const [supportedModels, setSupportedModels] = useState("");
  const [modelMapping, setModelMapping] = useState<Record<string, string>>({});
  const [extraHeaders, setExtraHeaders] = useState("");
  const [anthropicBaseUrl, setAnthropicBaseUrl] = useState("");
  const [responsesBaseUrl, setResponsesBaseUrl] = useState("");
  const [protocols, setProtocols] = useState<string[]>(["openai_chat_completions"]);
  const [timeoutSeconds, setTimeoutSeconds] = useState("120");
  const [enabled, setEnabled] = useState(true);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [fetchingModels, setFetchingModels] = useState(false);
  const [newMappingClient, setNewMappingClient] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Presets per provider type
  const PROVIDER_PRESETS: Record<string, { baseUrl: string; protocols: string[]; defaultModel: string; reasoningModel?: string; anthropicBaseUrl?: string; responsesBaseUrl?: string; extraHeaders?: string }> = {
    deepseek: { baseUrl: "https://api.deepseek.com", protocols: ["openai_chat_completions"], defaultModel: "deepseek-v4-flash", reasoningModel: "deepseek-v4-pro", anthropicBaseUrl: "https://api.deepseek.com/anthropic" },
    openai: { baseUrl: "https://api.openai.com", protocols: ["openai_chat_completions", "openai_responses"], defaultModel: "gpt-4o", responsesBaseUrl: "https://api.openai.com" },
    anthropic: { baseUrl: "https://api.anthropic.com", protocols: ["anthropic_messages"], defaultModel: "claude-sonnet-4-6" },
    openrouter: { baseUrl: "https://openrouter.ai/api", protocols: ["openai_chat_completions"], defaultModel: "deepseek/deepseek-v4-flash" },
    kimi: { baseUrl: "https://api.moonshot.cn", protocols: ["openai_chat_completions"], defaultModel: "kimi-k2", extraHeaders: '{"User-Agent":"KimiCLI/1.40.0"}' },
    minimax: { baseUrl: "https://api.minimax.chat", protocols: ["openai_chat_completions"], defaultModel: "MiniMax-M1" },
    custom_openai_compatible: { baseUrl: "", protocols: ["openai_chat_completions"], defaultModel: "" },
  };

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

  useEffect(() => {
    if (provider) {
      setName(provider.name);
      setProviderType(provider.provider_type);
      setBaseUrl(provider.base_url);
      setApiKey("");
      setDefaultModel(provider.default_model);
      setReasoningModel(provider.reasoning_model ?? "");
      setSupportedModels(provider.supported_models ?? "");
      try { setModelMapping(provider.model_mapping ? JSON.parse(provider.model_mapping) : {}); } catch { setModelMapping({}); }
      setExtraHeaders(provider.extra_headers ?? "");
      setAnthropicBaseUrl(provider.anthropic_base_url ?? "");
      setResponsesBaseUrl(provider.responses_base_url ?? "");
      try { setProtocols(JSON.parse(provider.protocol)); } catch { setProtocols([provider.protocol]); }
      setTimeoutSeconds(String(provider.timeout_seconds));
      setEnabled(provider.enabled);
    } else {
      setName("");
      setProviderType("deepseek");
      setBaseUrl("");
      setApiKey("");
      setDefaultModel("");
      setReasoningModel("");
      setSupportedModels("");
      setModelMapping({});
      setExtraHeaders("");
      setAnthropicBaseUrl("");
      setResponsesBaseUrl("");
      setProtocols(["openai_chat_completions"]);
      setTimeoutSeconds("120");
      setEnabled(true);
    }
    setErrors({});
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

    if (isEdit) {
      const input: UpdateProviderInput = {
        name, provider_type: providerType, base_url: baseUrl,
        default_model: defaultModel, reasoning_model: reasoningModel || undefined,
        supported_models: smStr, model_mapping: mmStr, extra_headers: ehStr, anthropic_base_url: abuStr, responses_base_url: rbuStr,
        protocol: JSON.stringify(protocols), timeout_seconds: parseInt(timeoutSeconds, 10), enabled,
      };
      if (apiKey) input.api_key = apiKey;
      onSubmit(input);
    } else {
      const input: CreateProviderInput = {
        name, provider_type: providerType, base_url: baseUrl,
        default_model: defaultModel, reasoning_model: reasoningModel || undefined,
        supported_models: smStr, model_mapping: mmStr, extra_headers: ehStr, anthropic_base_url: abuStr, responses_base_url: rbuStr,
        protocol: JSON.stringify(protocols), timeout_seconds: parseInt(timeoutSeconds, 10), enabled,
      };
      if (apiKey) input.api_key = apiKey;
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

        <form onSubmit={handleSubmit} className="space-y-4 p-6">
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

          <Field label={t("providers.api_key")} hint={isEdit && provider?.masked_api_key ? `Current: ${provider.masked_api_key}` : undefined}>
            <input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder={isEdit ? t("providers.api_key") : "sk-..."} className="form-input" />
          </Field>

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
                    if (apiKey) saveInput.api_key = apiKey;
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
                  {(() => {
                    let models: string[] = [];
                    try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
                    const listId = `provider-model-${clientModel}`;
                    return (
                      <>
                        <input
                          value={providerModel}
                          onChange={(e) => setModelMapping({ ...modelMapping, [clientModel]: e.target.value })}
                          list={models.length > 0 ? listId : undefined}
                          placeholder="model-name"
                          className="form-input flex-1"
                        />
                        {models.length > 0 && (
                          <datalist id={listId}>
                            {models.map((m) => <option key={m} value={m} />)}
                          </datalist>
                        )}
                      </>
                    );
                  })()}
                  <button type="button" onClick={() => { const next = { ...modelMapping }; delete next[clientModel]; setModelMapping(next); }} className="text-text-muted hover:text-error text-xs">✕</button>
                </div>
              ))}
            </div>
            <div className="mt-2 flex gap-2">
              <div className="relative flex-1">
                <input
                  value={newMappingClient}
                  onChange={(e) => setNewMappingClient(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      if (newMappingClient.trim() && !(newMappingClient.trim() in modelMapping)) {
                        let models: string[] = [];
                        try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
                        setModelMapping({ ...modelMapping, [newMappingClient.trim()]: models[0] || defaultModel || "" });
                        setNewMappingClient("");
                      }
                    }
                  }}
                  list="client-model-suggestions"
                  placeholder={t("providers.select_client_model")}
                  className="form-input w-full"
                />
                <datalist id="client-model-suggestions">
                  {["gpt-5.5","gpt-5.4","gpt-5.4-mini","gpt-5.3-codex","gpt-5.2","claude-sonnet-4-6","claude-opus-4-6","claude-haiku-4-5-20251001","o3","o4-mini"].filter(m => !(m in modelMapping)).map(m => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
              </div>
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
