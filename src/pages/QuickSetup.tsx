import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Key, Monitor, Rocket, CheckCircle, XCircle, Loader2, ArrowRight } from "lucide-react";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import { detectProvider } from "@/lib/keyDetection";
import { fetchDetectAndPersistProviderModels } from "@/lib/providerAutoSetup";
import { PROVIDER_PRESETS, resolveProviderPresetForKey } from "@/data/providerPresets";
import { PROVIDER_TYPES } from "@/types/provider";

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

export function QuickSetup() {
  const { t } = useI18n();
  const navigate = useNavigate();
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

  const handleKeyChange = (key: string) => {
    setApiKey(key);
    const result = detectProvider(key);
    setDetectedProvider(result ? { type: result.type, name: result.label } : null);
  };

  const handleGoToTools = async () => {
    setStep("tools");
    const results: ToolDetection[] = [];
    try { const c = await api.detectCodexConfig(); results.push({ id: "codex", name: "Codex", detected: c.exists, checked: true }); } catch { results.push({ id: "codex", name: "Codex", detected: false, checked: false }); }
    try { const c = await api.detectClaudeCodeEnv(); results.push({ id: "claude_code", name: "Claude Code", detected: c.settings_exists, checked: true }); } catch { results.push({ id: "claude_code", name: "Claude Code", detected: false, checked: false }); }
    try { const c = await api.detectOpenCodeConfig(); results.push({ id: "opencode", name: "OpenCode", detected: c.exists, checked: c.exists }); } catch { results.push({ id: "opencode", name: "OpenCode", detected: false, checked: false }); }
    try { const c = await api.detectGeminiConfig(); results.push({ id: "gemini", name: "Gemini CLI", detected: c.exists, checked: c.exists }); } catch { results.push({ id: "gemini", name: "Gemini CLI", detected: false, checked: false }); }
    try { const c = await api.detectAtomCodeConfig(); results.push({ id: "atomcode", name: "AtomCode", detected: c.exists, checked: c.exists }); } catch { results.push({ id: "atomcode", name: "AtomCode", detected: false, checked: false }); }
    setTools(results);
  };

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
      setStep("done"); return;
    }

    addLog(t("onboarding.starting_gateway"), "running");
    try { await api.startGateway(); } catch { /* maybe already running */ }
    addLog(t("onboarding.starting_gateway"), "ok");
    await new Promise(r => setTimeout(r, 500));

    for (const tool of tools.filter(t => t.checked)) {
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
      } catch { addLog(`${t("onboarding.configuring")} ${tool.name}`, "error"); }
    }

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
    <div className="mx-auto max-w-lg">
      {/* Step indicators */}
      <div className="mb-8 flex items-center justify-center gap-3">
        {[
          { key: "key", icon: Key, label: "API Key" },
          { key: "tools", icon: Monitor, label: t("onboarding.select_tools") },
          { key: "setup", icon: Rocket, label: t("onboarding.start_setup") },
        ].map((s, i) => {
          const isActive = s.key === step || (step === "done" && s.key === "setup");
          const isPast = ["key", "tools", "setup", "done"].indexOf(step) > ["key", "tools", "setup"].indexOf(s.key);
          return (
            <div key={s.key} className="flex items-center gap-3">
              {i > 0 && <div className={`h-px w-8 ${isPast ? "bg-accent" : "bg-border"}`} />}
              <div className={`flex items-center gap-2 rounded-full px-3 py-1.5 text-xs font-medium ${
                isActive ? "bg-accent-soft text-accent" : isPast ? "text-success" : "text-text-muted"
              }`}>
                {isPast ? <CheckCircle className="h-3.5 w-3.5" /> : <s.icon className="h-3.5 w-3.5" />}
                {s.label}
              </div>
            </div>
          );
        })}
      </div>

      {/* Step 1: API Key */}
      {step === "key" && (
        <div className="rounded-xl border border-border bg-card p-6 space-y-5" style={{ boxShadow: "var(--shadow-sm)" }}>
          <div>
            <h2 className="text-base font-semibold text-text-primary mb-1">{t("onboarding.welcome")}</h2>
            <p className="text-xs text-text-muted">{t("onboarding.welcome_desc")}</p>
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

          <div className="flex justify-end">
            <button onClick={handleGoToTools} disabled={!detectedProvider} className="btn-primary disabled:opacity-40">
              {t("onboarding.next")} <ArrowRight className="h-3 w-3" />
            </button>
          </div>
        </div>
      )}

      {/* Step 2: Tools */}
      {step === "tools" && (
        <div className="rounded-xl border border-border bg-card p-6 space-y-5" style={{ boxShadow: "var(--shadow-sm)" }}>
          <div>
            <h2 className="text-base font-semibold text-text-primary mb-1">{t("onboarding.select_tools")}</h2>
            <p className="text-xs text-text-muted">{t("onboarding.select_tools_desc")}</p>
          </div>

          <div className="space-y-2">
            {tools.map((tool) => (
              <label key={tool.id} className={`flex items-center gap-3 rounded-lg border px-4 py-3 cursor-pointer transition-colors ${
                tool.checked ? "border-accent bg-accent-soft" : "border-border hover:border-text-muted"
              }`}>
                <input type="checkbox" checked={tool.checked} onChange={(e) => setTools(tools.map(t => t.id === tool.id ? { ...t, checked: e.target.checked } : t))} className="sr-only" />
                <div className={`h-4 w-4 rounded border flex items-center justify-center ${tool.checked ? "bg-accent border-accent" : "border-border"}`}>
                  {tool.checked && <CheckCircle className="h-3 w-3 text-white" />}
                </div>
                <span className="text-sm text-text-primary">{tool.name}</span>
                {tool.detected && <span className="ml-auto text-[10px] text-success">{t("tools.config_found")}</span>}
              </label>
            ))}
          </div>

          <div className="flex justify-between">
            <button onClick={() => setStep("key")} className="text-xs text-text-muted hover:text-text-primary">← {t("onboarding.back")}</button>
            <button onClick={handleSetup} className="btn-primary"><Rocket className="h-3 w-3" /> {t("onboarding.start_setup")}</button>
          </div>
        </div>
      )}

      {/* Step 3+4: Progress & Done */}
      {(step === "setup" || step === "done") && (
        <div className="rounded-xl border border-border bg-card p-6 space-y-5" style={{ boxShadow: "var(--shadow-sm)" }}>
          <h2 className="text-base font-semibold text-text-primary">
            {step === "done" ? (allOk ? t("onboarding.complete") : t("onboarding.partial")) : t("onboarding.setting_up")}
          </h2>

          <div className="space-y-3">
            {setupLog.map((entry, i) => (
              <div key={i} className="flex items-start gap-3 text-sm">
                {entry.status === "running" ? <Loader2 className="h-4 w-4 animate-spin text-accent" />
                  : entry.status === "ok" ? <CheckCircle className="h-4 w-4 text-success" />
                  : entry.status === "error" ? <XCircle className="h-4 w-4 text-error" />
                  : <div className="h-4 w-4 rounded-full border-2 border-border" />}
                <div className="min-w-0">
                  <div className="text-text-primary">{entry.label}</div>
                  {entry.detail && (
                    <div className="mt-1 max-w-full break-words text-xs text-error">{entry.detail}</div>
                  )}
                </div>
              </div>
            ))}
          </div>

          {step === "done" && (
            <div className="space-y-3">
              {/* 完成后告诉用户"下一步"——配好了不等于能用，要去终端真跑命令。
                  跳 /tools 让用户看到客户端配置卡片 + 复制命令。 */}
              {allOk && (
                <div className="rounded-lg border border-accent/20 bg-accent-soft/40 p-4">
                  <p className="text-sm font-medium text-text-primary">
                    {t("onboarding.next_step_title")}
                  </p>
                  <p className="mt-1 text-xs text-text-secondary">
                    {t("onboarding.next_step_desc")}
                  </p>
                </div>
              )}
              <div className="flex justify-end gap-2">
                {allOk && (
                  <button onClick={() => navigate("/")} className="btn-secondary">
                    {t("onboarding.back_to_overview")}
                  </button>
                )}
                <button
                  onClick={() => navigate(allOk ? "/tools" : "/")}
                  className="btn-primary"
                >
                  {allOk ? t("onboarding.go_to_clients") : t("common.close")}
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
