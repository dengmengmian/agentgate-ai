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
  const [protocol, setProtocol] = useState("openai_chat_completions");
  const [timeoutSeconds, setTimeoutSeconds] = useState("120");
  const [enabled, setEnabled] = useState(true);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [fetchingModels, setFetchingModels] = useState(false);

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
      setProtocol(provider.protocol);
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
      setProtocol("openai_chat_completions");
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

    if (isEdit) {
      const input: UpdateProviderInput = {
        name, provider_type: providerType, base_url: baseUrl,
        default_model: defaultModel, reasoning_model: reasoningModel || undefined,
        supported_models: smStr, model_mapping: mmStr, extra_headers: ehStr, anthropic_base_url: abuStr,
        protocol, timeout_seconds: parseInt(timeoutSeconds, 10), enabled,
      };
      if (apiKey) input.api_key = apiKey;
      onSubmit(input);
    } else {
      const input: CreateProviderInput = {
        name, provider_type: providerType, base_url: baseUrl,
        default_model: defaultModel, reasoning_model: reasoningModel || undefined,
        supported_models: smStr, model_mapping: mmStr, extra_headers: ehStr, anthropic_base_url: abuStr,
        protocol, timeout_seconds: parseInt(timeoutSeconds, 10), enabled,
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
          <Field label={t("providers.name")} error={errors.name}>
            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="My Provider" className="form-input" />
          </Field>

          <div className="grid grid-cols-2 gap-4">
            <Field label={t("providers.type")}>
              <select value={providerType} onChange={(e) => setProviderType(e.target.value)} className="form-input">
                {PROVIDER_TYPES.map((tp) => (
                  <option key={tp.value} value={tp.value}>{tp.label}</option>
                ))}
              </select>
            </Field>
            <Field label={t("providers.protocol")}>
              <select value={protocol} onChange={(e) => setProtocol(e.target.value)} className="form-input">
                {PROTOCOLS.map((p) => (
                  <option key={p.value} value={p.value}>{p.label}</option>
                ))}
              </select>
            </Field>
          </div>

          <Field label={t("providers.base_url")} error={errors.baseUrl}>
            <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.example.com" className="form-input" />
          </Field>

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
                  <input value={clientModel} readOnly className="form-input flex-1 bg-bg text-text-muted" />
                  <span className="text-text-muted">→</span>
                  {(() => {
                    let models: string[] = [];
                    try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
                    return models.length > 0 ? (
                      <select value={providerModel} onChange={(e) => setModelMapping({ ...modelMapping, [clientModel]: e.target.value })} className="form-input flex-1">
                        {models.map((m) => <option key={m} value={m}>{m}</option>)}
                      </select>
                    ) : (
                      <input value={providerModel} onChange={(e) => setModelMapping({ ...modelMapping, [clientModel]: e.target.value })} className="form-input flex-1" />
                    );
                  })()}
                  <button type="button" onClick={() => { const next = { ...modelMapping }; delete next[clientModel]; setModelMapping(next); }} className="text-text-muted hover:text-error text-xs">✕</button>
                </div>
              ))}
            </div>
            <div className="mt-2 flex gap-2">
              <select id="new-mapping-client" className="form-input flex-1" defaultValue="">
                <option value="" disabled>{t("providers.select_client_model")}</option>
                {["gpt-5.5","gpt-5.4","gpt-5.4-mini","gpt-5.3-codex","gpt-5.2","claude-sonnet-4-6","claude-opus-4-6","claude-haiku-4-5-20251001","o3","o4-mini"].filter(m => !(m in modelMapping)).map(m => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
              <button type="button" onClick={() => {
                const sel = document.getElementById("new-mapping-client") as HTMLSelectElement;
                if (sel.value) {
                  let models: string[] = [];
                  try { models = supportedModels ? JSON.parse(supportedModels) : []; } catch { /* */ }
                  setModelMapping({ ...modelMapping, [sel.value]: models[0] || defaultModel || "" });
                  sel.value = "";
                }
              }} className="btn-secondary">{t("routes.add")}</button>
            </div>
          </div>

          <Field label={t("providers.anthropic_url")} hint={t("providers.anthropic_url_hint")}>
            <input value={anthropicBaseUrl} onChange={(e) => setAnthropicBaseUrl(e.target.value)} placeholder="https://api.deepseek.com/anthropic" className="form-input" />
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
