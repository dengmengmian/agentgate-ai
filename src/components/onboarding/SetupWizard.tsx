import { useState } from "react";
import { Zap, CheckCircle, XCircle, Loader2, ArrowRight, Key, Monitor, Rocket } from "lucide-react";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import { detectProvider } from "@/lib/keyDetection";
import { fetchDetectAndPersistProviderModels } from "@/lib/providerAutoSetup";
import { PROVIDER_PRESETS, resolveProviderPresetForKey } from "@/data/providerPresets";
import { PROVIDER_TYPES } from "@/types/provider";

interface Props {
  onComplete: () => void;
}

type Step = "key" | "tools" | "setup" | "done";

interface ToolDetection {
  id: string;
  name: string;
  detected: boolean;
  checked: boolean;
}

interface SetupLogEntry {
  label: string;
  status: "pending" | "running" | "ok" | "error";
  detail?: string;
}

export function SetupWizard({ onComplete }: Props) {
  const { t } = useI18n();
  const [step, setStep] = useState<Step>("key");
  const [apiKey, setApiKey] = useState("");
  const [detectedProvider, setDetectedProvider] = useState<{ type: string; name: string } | null>(null);
  const [tools, setTools] = useState<ToolDetection[]>([]);
  const [setupLog, setSetupLog] = useState<SetupLogEntry[]>([]);

  const quickProviderTypes = PROVIDER_TYPES.filter((tp) => PROVIDER_PRESETS[tp.value]);

  const selectProvider = (type: string) => {
    const label = PROVIDER_TYPES.find((tp) => tp.value === type)?.label ?? type;
    setDetectedProvider(type ? { type, name: label } : null);
  };

  // Step 1: detect provider from key
  const handleKeyChange = (key: string) => {
    setApiKey(key);
    const result = detectProvider(key);
    setDetectedProvider(result ? { type: result.type, name: result.label } : null);
  };

  // Step 2: detect installed tools
  const handleGoToTools = async () => {
    setStep("tools");
    const results: ToolDetection[] = [];

    try {
      const codex = await api.detectCodexConfig();
      results.push({ id: "codex", name: "Codex", detected: codex.exists, checked: true });
    } catch { results.push({ id: "codex", name: "Codex", detected: false, checked: false }); }

    try {
      const claude = await api.detectClaudeCodeEnv();
      results.push({ id: "claude_code", name: "Claude Code", detected: claude.settings_exists, checked: true });
    } catch { results.push({ id: "claude_code", name: "Claude Code", detected: false, checked: false }); }

    try {
      const oc = await api.detectOpenCodeConfig();
      results.push({ id: "opencode", name: "OpenCode", detected: oc.exists, checked: oc.exists });
    } catch { results.push({ id: "opencode", name: "OpenCode", detected: false, checked: false }); }

    try {
      const gem = await api.detectGeminiConfig();
      results.push({ id: "gemini", name: "Gemini CLI", detected: gem.exists, checked: gem.exists });
    } catch { results.push({ id: "gemini", name: "Gemini CLI", detected: false, checked: false }); }

    try {
      const atom = await api.detectAtomCodeConfig();
      results.push({ id: "atomcode", name: "AtomCode", detected: atom.exists, checked: atom.exists });
    } catch { results.push({ id: "atomcode", name: "AtomCode", detected: false, checked: false }); }

    setTools(results);
  };

  // Step 3: execute setup
  const handleSetup = async () => {
    setStep("setup");
    const log: typeof setupLog = [];
    const addLog = (label: string, status: "pending" | "running" | "ok" | "error", detail?: string) => {
      const idx = log.findIndex(l => l.label === label);
      if (idx >= 0) {
        log[idx] = { ...log[idx], status, detail };
      } else {
        log.push({ label, status, detail });
      }
      setSetupLog([...log]);
    };

    // 1. Create provider
    const preset = resolveProviderPresetForKey(detectedProvider!.type, apiKey.trim());
    if (!preset) return;
    addLog(t("onboarding.creating_provider"), "running");
    try {
      const provider = await api.createProvider({
        name: detectedProvider!.name,
        provider_type: detectedProvider!.type,
        base_url: preset.baseUrl,
        api_key: apiKey.trim(),
        default_model: preset.defaultModel,
        reasoning_model: preset.reasoningModel ?? undefined,
        protocol: JSON.stringify(preset.protocols),
        timeout_seconds: 120,
        enabled: true,
        anthropic_base_url: preset.anthropicBaseUrl ?? undefined,
        responses_base_url: preset.responsesBaseUrl ?? undefined,
        extra_headers: preset.extraHeaders ?? undefined,
        auto_cache_control: true,
      });
      await api.setActiveProvider(provider.id);
      addLog(t("onboarding.creating_provider"), "ok");

      addLog(t("onboarding.detecting_capabilities"), "running");
      try {
        const { models } = await fetchDetectAndPersistProviderModels(provider.id, detectedProvider!.type);
        const detail = models.length ? `${models.length} ${t("providers.toast_models_and_caps")}` : t("providers.test.autofill_none");
        addLog(t("onboarding.detecting_capabilities"), "ok", detail);
      } catch (err) {
        addLog(t("onboarding.detecting_capabilities"), "error", err instanceof Error ? err.message : String(err));
      }
    } catch {
      addLog(t("onboarding.creating_provider"), "error");
      setStep("done");
      return;
    }

    // 2. Start gateway
    addLog(t("onboarding.starting_gateway"), "running");
    try {
      await api.startGateway();
      addLog(t("onboarding.starting_gateway"), "ok");
    } catch {
      // Maybe already running
      addLog(t("onboarding.starting_gateway"), "ok");
    }

    // Small delay for gateway to be ready
    await new Promise(r => setTimeout(r, 500));

    // 3. Apply tool configs
    const checkedTools = tools.filter(t => t.checked);
    for (const tool of checkedTools) {
      addLog(`${t("onboarding.configuring")} ${tool.name}`, "running");
      try {
        switch (tool.id) {
          case "codex": await api.applyCodexConfig(); break;
          case "claude_code": await api.applyClaudeCodeConfig(); break;
          case "opencode": await api.applyOpenCodeConfig(); break;
          case "gemini": await api.applyGeminiConfig(); break;
          case "atomcode": await api.applyAtomCodeConfig(); break;
        }
        addLog(`${t("onboarding.configuring")} ${tool.name}`, "ok");
      } catch {
        addLog(`${t("onboarding.configuring")} ${tool.name}`, "error");
      }
    }

    // 4. Test connection
    addLog(t("onboarding.testing"), "running");
    try {
      const test = await api.testToolConnection();
      const detail = test.provider_ok ? undefined : (test.error ?? "Gateway or provider test failed");
      addLog(t("onboarding.testing"), test.provider_ok ? "ok" : "error", detail);
    } catch (err) {
      addLog(t("onboarding.testing"), "error", err instanceof Error ? err.message : String(err));
    }

    setStep("done");
  };

  const allOk = setupLog.length > 0 && setupLog.every(l => l.status === "ok");

  return (
    <div className="fixed inset-0 z-[95] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50 backdrop-blur-sm" />
      <div className="animate-scale-in relative z-10 w-full max-w-md rounded-xl border border-border bg-card p-8" style={{ boxShadow: "var(--shadow-lg)" }}>

        {/* Step 1: API Key */}
        {step === "key" && (
          <div className="space-y-5">
            <div className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-accent-soft">
                <Key className="h-5 w-5 text-accent" />
              </div>
              <div>
                <h2 className="text-base font-semibold text-text-primary">{t("onboarding.welcome")}</h2>
                <p className="text-xs text-text-muted">{t("onboarding.welcome_desc")}</p>
              </div>
            </div>

            <input
              value={apiKey}
              onChange={(e) => handleKeyChange(e.target.value)}
              placeholder="sk-xxx / tp-xxx / deepseek-xxx / sk-ant-xxx ..."
              className="form-input text-sm"
              autoFocus
            />

            {detectedProvider && (
              <div className="flex items-center gap-2 rounded-lg bg-success-soft px-3 py-2 text-xs text-success">
                <CheckCircle className="h-3.5 w-3.5" />
                {t("onboarding.detected")} {detectedProvider.name}
              </div>
            )}

            {apiKey.trim() && (
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-text-secondary">
                  {t("onboarding.provider_label")}
                </label>
                <select
                  value={detectedProvider?.type ?? ""}
                  onChange={(e) => selectProvider(e.target.value)}
                  className="form-input text-sm"
                >
                  <option value="">{t("onboarding.provider_select_placeholder")}</option>
                  {quickProviderTypes.map((tp) => (
                    <option key={tp.value} value={tp.value}>{tp.label}</option>
                  ))}
                </select>
                <p className="text-[11px] text-text-muted">{t("onboarding.provider_hint")}</p>
              </div>
            )}

            <div className="flex justify-between items-center">
              <button onClick={onComplete} className="text-xs text-text-muted hover:text-text-primary">
                {t("onboarding.skip")}
              </button>
              <button
                onClick={handleGoToTools}
                disabled={!detectedProvider}
                className="btn-primary disabled:opacity-40"
              >
                {t("onboarding.next")} <ArrowRight className="h-3 w-3" />
              </button>
            </div>
          </div>
        )}

        {/* Step 2: Tool Selection */}
        {step === "tools" && (
          <div className="space-y-5">
            <div className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-accent-soft">
                <Monitor className="h-5 w-5 text-accent" />
              </div>
              <div>
                <h2 className="text-base font-semibold text-text-primary">{t("onboarding.select_tools")}</h2>
                <p className="text-xs text-text-muted">{t("onboarding.select_tools_desc")}</p>
              </div>
            </div>

            <div className="space-y-2">
              {tools.map((tool) => (
                <label
                  key={tool.id}
                  className={`flex items-center gap-3 rounded-lg border px-4 py-3 cursor-pointer transition-colors ${
                    tool.checked ? "border-accent bg-accent-soft" : "border-border hover:border-text-muted"
                  }`}
                >
                  <input
                    type="checkbox"
                    checked={tool.checked}
                    onChange={(e) => setTools(tools.map(t => t.id === tool.id ? { ...t, checked: e.target.checked } : t))}
                    className="sr-only"
                  />
                  <div className={`h-4 w-4 rounded border flex items-center justify-center ${tool.checked ? "bg-accent border-accent" : "border-border"}`}>
                    {tool.checked && <CheckCircle className="h-3 w-3 text-white" />}
                  </div>
                  <span className="text-sm text-text-primary">{tool.name}</span>
                  {tool.detected && (
                    <span className="ml-auto text-[10px] text-success">{t("tools.config_found")}</span>
                  )}
                </label>
              ))}
            </div>

            <div className="flex justify-between items-center">
              <button onClick={() => setStep("key")} className="text-xs text-text-muted hover:text-text-primary">
                ← {t("onboarding.back")}
              </button>
              <button onClick={handleSetup} className="btn-primary">
                <Rocket className="h-3 w-3" /> {t("onboarding.start_setup")}
              </button>
            </div>
          </div>
        )}

        {/* Step 3: Setup Progress */}
        {(step === "setup" || step === "done") && (
          <div className="space-y-5">
            <div className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-accent-soft">
                <Zap className="h-5 w-5 text-accent" />
              </div>
              <div>
                <h2 className="text-base font-semibold text-text-primary">
                  {step === "done" ? (allOk ? t("onboarding.complete") : t("onboarding.partial")) : t("onboarding.setting_up")}
                </h2>
              </div>
            </div>

            <div className="space-y-2">
              {setupLog.map((entry, i) => (
                <div key={i} className="flex items-start gap-3 text-xs">
                  {entry.status === "running" ? (
                    <Loader2 className="h-4 w-4 animate-spin text-accent" />
                  ) : entry.status === "ok" ? (
                    <CheckCircle className="h-4 w-4 text-success" />
                  ) : entry.status === "error" ? (
                    <XCircle className="h-4 w-4 text-error" />
                  ) : (
                    <div className="h-4 w-4 rounded-full border-2 border-border" />
                  )}
                  <div className="min-w-0">
                    <div className="text-text-primary">{entry.label}</div>
                    {entry.detail && (
                      <div className="mt-1 max-w-full break-words text-[11px] text-error">{entry.detail}</div>
                    )}
                  </div>
                </div>
              ))}
            </div>

            {step === "done" && (
              <div className="flex justify-end">
                <button onClick={onComplete} className="btn-primary">
                  {allOk ? t("onboarding.done") : t("common.close")}
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
