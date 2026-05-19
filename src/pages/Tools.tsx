import { useState, useEffect, useCallback } from "react";
import {
  Terminal,
  Code,
  Braces,
  Sparkles,
  Atom,
  FolderOpen,
  AlertTriangle,
  Zap,
  Shield,
  ToggleLeft,
  ToggleRight,
} from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { JsonCodeBlock } from "@/components/common/JsonCodeBlock";
import { CopyButton } from "@/components/common/CopyButton";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { CodexConfigStatus, ClaudeCodeEnvStatus, OpenCodeConfigStatus, GeminiCliConfigStatus, AtomCodeConfigStatus } from "@/types/config";

export function Tools() {
  const { t } = useI18n();
  const [codexStatus, setCodexStatus] = useState<CodexConfigStatus | null>(null);
  const [claudeEnv, setClaudeEnv] = useState<ClaudeCodeEnvStatus | null>(null);
  const [codexConfig, setCodexConfig] = useState("");
  const [claudeSnippet, setClaudeSnippet] = useState("");
  const [loading, setLoading] = useState(true);
  const [openCodeStatus, setOpenCodeStatus] = useState<OpenCodeConfigStatus | null>(null);
  const [geminiStatus, setGeminiStatus] = useState<GeminiCliConfigStatus | null>(null);
  const [atomCodeStatus, setAtomCodeStatus] = useState<AtomCodeConfigStatus | null>(null);
  const [confirmApplyCodex, setConfirmApplyCodex] = useState(false);
  const [confirmApplyClaude, setConfirmApplyClaude] = useState(false);
  const [confirmApplyOpenCode, setConfirmApplyOpenCode] = useState(false);
  const [confirmApplyGemini, setConfirmApplyGemini] = useState(false);
  const [confirmApplyAtomCode, setConfirmApplyAtomCode] = useState(false);

  const load = useCallback(async () => {
    try {
      const [c, cc, oc, gc, ac] = await Promise.all([
        api.detectCodexConfig(),
        api.detectClaudeCodeEnv(),
        api.detectOpenCodeConfig(),
        api.detectGeminiConfig(),
        api.detectAtomCodeConfig(),
      ]);
      setCodexStatus(c);
      setClaudeEnv(cc);
      setOpenCodeStatus(oc);
      setGeminiStatus(gc);
      setAtomCodeStatus(ac);
      const snippet = await api.generateCodexConfig();
      setCodexConfig(snippet);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleApplyCodex = async () => {
    try {
      const result = await api.applyCodexConfig();
      if (result.success) {
        toast("success", `Codex ${t("tools.config_written")} ${result.config_path}`);
      }
      setConfirmApplyCodex(false);
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleToggleCodex = async () => {
    try {
      const result = await api.toggleCodexProvider();
      if (result.success) {
        const label = result.new_provider === "agentgate" ? "AgentGate" : result.new_provider;
        toast("success", `${t("tools.switched_to")} ${label}`);
      }
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleApplyClaude = async () => {
    try {
      const result = await api.applyClaudeCodeConfig();
      if (result.success) {
        toast("success", `Claude Code ${t("tools.config_written")} ${result.config_path}`);
      }
      setConfirmApplyClaude(false);
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleToggleClaude = async () => {
    try {
      const result = await api.toggleClaudeCodeProvider();
      if (result.success) {
        const label = result.new_provider === "agentgate" ? "AgentGate" : t("tools.official");
        toast("success", `${t("tools.switched_to")} ${label}`);
      }
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleApplyOpenCode = async () => {
    try {
      const result = await api.applyOpenCodeConfig();
      if (result.success) {
        toast("success", `OpenCode ${t("tools.config_written")} ${result.config_path}`);
      }
      setConfirmApplyOpenCode(false);
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleGenerateClaudeSnippet = async () => {
    try {
      const snippet = await api.generateClaudeCodeEnv();
      setClaudeSnippet(snippet);
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleApplyGemini = async () => {
    try {
      const result = await api.applyGeminiConfig();
      if (result.success) toast("success", `Gemini CLI ${t("tools.config_written")} ${result.config_path}`);
      setConfirmApplyGemini(false);
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleToggleGemini = async () => {
    try {
      const result = await api.toggleGeminiProvider();
      if (result.success) {
        const label = result.new_provider === "agentgate" ? "AgentGate" : t("tools.official");
        toast("success", `${t("tools.switched_to")} ${label}`);
      }
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleApplyAtomCode = async () => {
    try {
      const result = await api.applyAtomCodeConfig();
      if (result.success) toast("success", `AtomCode ${t("tools.config_written")} ${result.config_path}`);
      setConfirmApplyAtomCode(false);
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleToggleAtomCode = async () => {
    try {
      const result = await api.toggleAtomCodeProvider();
      if (result.success) {
        const label = result.new_provider === "agentgate" ? "AgentGate" : t("tools.official");
        toast("success", `${t("tools.switched_to")} ${label}`);
      }
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  if (loading) return <p className="text-xs text-text-muted">{t("common.loading")}</p>;

  return (
    <div className="space-y-6">
      {/* Codex Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <div className="mb-4 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Code className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.codex")}</h3>
              <p className="text-xs text-text-muted">{t("tools.codex_desc")}</p>
            </div>
          </div>
          <StatusBadge variant={codexStatus?.has_agentgate ? "success" : codexStatus?.exists ? "warning" : "muted"}>
            {codexStatus?.has_agentgate ? t("tools.agentgate_configured") : codexStatus?.exists ? t("tools.not_configured") : t("tools.no_config")}
          </StatusBadge>
        </div>

        {codexStatus && (
          <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
            <div><span className="text-text-muted">config.toml</span><p className="font-mono text-text-secondary text-[11px]">{codexStatus.config_path}</p></div>
            <div><span className="text-text-muted">{t("tools.current_provider")}</span><p className="text-text-primary">{codexStatus.current_provider ?? "—"}</p></div>
            <div><span className="text-text-muted">auth.json</span><p className="font-mono text-text-secondary text-[11px]">{codexStatus.auth_json_path}</p></div>
            <div><span className="text-text-muted">{t("tools.auth_status")}</span><p className="flex items-center gap-1 text-text-primary"><Shield className="h-3 w-3 text-accent" />{codexStatus.has_agentgate_auth ? t("tools.token_set") : t("tools.not_configured")}</p></div>
          </div>
        )}

        {codexStatus?.openai_key_polluted && (
          <div className="mb-3 rounded-md border border-warning/30 bg-warning/5 p-3">
            <div className="flex items-center gap-2 text-xs font-medium text-warning">
              <AlertTriangle className="h-3.5 w-3.5" />
              {t("tools.openai_key_polluted")}
            </div>
            <p className="mt-1 text-[11px] text-text-secondary">{t("tools.openai_key_polluted_desc")}</p>
          </div>
        )}

        <p className="mb-3 text-[11px] text-text-muted">{t("tools.codex_auth_desc")}</p>

        <div className="mb-4 flex flex-wrap gap-2">
          <button onClick={() => setConfirmApplyCodex(true)} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
          {codexStatus?.is_agentgate_active && codexStatus?.has_saved_official && (
            <button onClick={handleToggleCodex} className="btn-secondary">
              <ToggleRight className="h-3 w-3" />{t("tools.switch_to_official")}
            </button>
          )}
          {!codexStatus?.is_agentgate_active && codexStatus?.has_agentgate && (
            <button onClick={handleToggleCodex} className="btn-primary">
              <ToggleLeft className="h-3 w-3" />{t("tools.switch_to_agentgate")}
            </button>
          )}
          {codexStatus?.exists && (
            <button onClick={() => api.openCodexConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
          )}
          <CopyButton text={codexConfig} />
        </div>
      </div>

      {/* Claude Code Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <div className="mb-4 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Terminal className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.claude_code")}</h3>
              <p className="text-xs text-text-muted">{t("tools.claude_code_desc")}</p>
            </div>
          </div>
          <StatusBadge variant={claudeEnv?.has_agentgate ? "success" : claudeEnv?.has_api_key || claudeEnv?.has_auth_token ? "warning" : "muted"}>
            {claudeEnv?.has_agentgate ? t("tools.agentgate_configured") : claudeEnv?.has_api_key || claudeEnv?.has_auth_token ? t("tools.direct_credentials") : t("tools.no_credentials")}
          </StatusBadge>
        </div>

        {claudeEnv && (
          <>
            <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
              <div><span className="text-text-muted">Settings Path</span><p className="font-mono text-text-secondary text-[11px]">{claudeEnv.settings_path}</p></div>
              <div><span className="text-text-muted">{t("settings.auth_mode")}</span><p className="flex items-center gap-1 text-text-primary"><Shield className="h-3 w-3 text-accent" />{claudeEnv.auth_mode}</p></div>
              <div><span className="text-text-muted">{t("providers.base_url")}</span><p className="font-mono text-text-secondary">{claudeEnv.active_base_url ?? "default"}</p></div>
              <div><span className="text-text-muted">{t("logs.model")}</span><p className="font-mono text-text-primary">{claudeEnv.active_model ?? "default"}</p></div>
            </div>

            {claudeEnv.conflicts.length > 0 && (
              <div className="mb-4 rounded-md border border-warning/30 bg-warning/5 p-3">
                <div className="flex items-center gap-2 text-xs font-medium text-warning"><AlertTriangle className="h-3.5 w-3.5" />{claudeEnv.conflicts.length} {t("tools.conflicts_detected")}</div>
                {claudeEnv.conflicts.map((c, i) => <p key={i} className="mt-1 text-[11px] text-text-secondary">{c}</p>)}
              </div>
            )}
          </>
        )}

        <p className="mb-3 text-[11px] text-text-muted">{t("tools.claude_auth_desc")}</p>

        <div className="mb-4 flex flex-wrap gap-2">
          <button onClick={() => setConfirmApplyClaude(true)} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
          {claudeEnv?.has_agentgate && claudeEnv?.has_saved_official && (
            <button onClick={handleToggleClaude} className="btn-secondary">
              <ToggleRight className="h-3 w-3" />{t("tools.switch_to_official")}
            </button>
          )}
          {!claudeEnv?.has_agentgate && claudeEnv?.has_saved_official && (
            <button onClick={handleToggleClaude} className="btn-primary">
              <ToggleLeft className="h-3 w-3" />{t("tools.switch_to_agentgate")}
            </button>
          )}
          {claudeEnv?.settings_exists && (
            <button onClick={() => api.openClaudeCodeConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
          )}
          <button onClick={handleGenerateClaudeSnippet} className="btn-secondary"><Code className="h-3 w-3" />{t("tools.env_snippet")}</button>
        </div>

        {claudeSnippet && <JsonCodeBlock title="Claude Code Environment" content={claudeSnippet} language="bash" />}
      </div>

      {/* OpenCode Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <div className="mb-4 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Braces className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.opencode")}</h3>
              <p className="text-xs text-text-muted">{t("tools.opencode_desc")}</p>
            </div>
          </div>
          <StatusBadge variant={openCodeStatus?.has_agentgate ? "success" : openCodeStatus?.exists ? "warning" : "muted"}>
            {openCodeStatus?.has_agentgate ? t("tools.agentgate_configured") : openCodeStatus?.exists ? t("tools.not_configured") : t("tools.no_config")}
          </StatusBadge>
        </div>

        {openCodeStatus && (
          <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
            <div><span className="text-text-muted">opencode.json</span><p className="font-mono text-text-secondary text-[11px]">{openCodeStatus.config_path}</p></div>
            <div><span className="text-text-muted">{t("logs.model")}</span><p className="text-text-primary">{openCodeStatus.current_model ?? "—"}</p></div>
          </div>
        )}

        <p className="mb-3 text-[11px] text-text-muted">{t("tools.opencode_auth_desc")}</p>

        <div className="mb-4 flex flex-wrap gap-2">
          <button onClick={() => setConfirmApplyOpenCode(true)} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
          {openCodeStatus?.exists && (
            <button onClick={() => api.openOpenCodeConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
          )}
        </div>
      </div>

      {/* Gemini CLI Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <div className="mb-4 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Sparkles className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.gemini_cli")}</h3>
              <p className="text-xs text-text-muted">{t("tools.gemini_cli_desc")}</p>
            </div>
          </div>
          <StatusBadge variant={geminiStatus?.has_agentgate ? "success" : geminiStatus?.exists ? "warning" : "muted"}>
            {geminiStatus?.has_agentgate ? t("tools.agentgate_configured") : geminiStatus?.exists ? t("tools.not_configured") : t("tools.no_config")}
          </StatusBadge>
        </div>

        {geminiStatus && (
          <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
            <div><span className="text-text-muted">settings.json</span><p className="font-mono text-text-secondary text-[11px]">{geminiStatus.config_path}</p></div>
            <div><span className="text-text-muted">{t("logs.model")}</span><p className="text-text-primary">{geminiStatus.current_model ?? "—"}</p></div>
          </div>
        )}

        <div className="mb-4 flex flex-wrap gap-2">
          <button onClick={() => setConfirmApplyGemini(true)} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
          {geminiStatus?.has_agentgate && geminiStatus?.has_saved_official && (
            <button onClick={handleToggleGemini} className="btn-secondary"><ToggleRight className="h-3 w-3" />{t("tools.switch_to_official")}</button>
          )}
          {!geminiStatus?.has_agentgate && geminiStatus?.has_saved_official && (
            <button onClick={handleToggleGemini} className="btn-primary"><ToggleLeft className="h-3 w-3" />{t("tools.switch_to_agentgate")}</button>
          )}
          {geminiStatus?.exists && (
            <button onClick={() => api.openGeminiConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
          )}
        </div>
      </div>

      {/* AtomCode Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <div className="mb-4 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Atom className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.atomcode")}</h3>
              <p className="text-xs text-text-muted">{t("tools.atomcode_desc")}</p>
            </div>
          </div>
          <StatusBadge variant={atomCodeStatus?.has_agentgate ? "success" : atomCodeStatus?.exists ? "warning" : "muted"}>
            {atomCodeStatus?.has_agentgate ? t("tools.agentgate_configured") : atomCodeStatus?.exists ? t("tools.not_configured") : t("tools.no_config")}
          </StatusBadge>
        </div>

        {atomCodeStatus && (
          <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
            <div><span className="text-text-muted">config.toml</span><p className="font-mono text-text-secondary text-[11px]">{atomCodeStatus.config_path}</p></div>
            <div><span className="text-text-muted">{t("logs.model")}</span><p className="text-text-primary">{atomCodeStatus.current_model ?? "—"}</p></div>
          </div>
        )}

        <div className="mb-4 flex flex-wrap gap-2">
          <button onClick={() => setConfirmApplyAtomCode(true)} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
          {atomCodeStatus?.has_agentgate && atomCodeStatus?.has_saved_official && (
            <button onClick={handleToggleAtomCode} className="btn-secondary"><ToggleRight className="h-3 w-3" />{t("tools.switch_to_official")}</button>
          )}
          {!atomCodeStatus?.has_agentgate && atomCodeStatus?.has_saved_official && (
            <button onClick={handleToggleAtomCode} className="btn-primary"><ToggleLeft className="h-3 w-3" />{t("tools.switch_to_agentgate")}</button>
          )}
          {atomCodeStatus?.exists && (
            <button onClick={() => api.openAtomCodeConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
          )}
        </div>
      </div>

      <ConfirmDialog open={confirmApplyCodex} title={t("tools.apply_codex_title")} message={t("tools.apply_codex_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyCodex} onCancel={() => setConfirmApplyCodex(false)} />
      <ConfirmDialog open={confirmApplyClaude} title={t("tools.apply_claude_title")} message={t("tools.apply_claude_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyClaude} onCancel={() => setConfirmApplyClaude(false)} />
      <ConfirmDialog open={confirmApplyOpenCode} title={t("tools.apply_opencode_title")} message={t("tools.apply_opencode_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyOpenCode} onCancel={() => setConfirmApplyOpenCode(false)} />
      <ConfirmDialog open={confirmApplyGemini} title={t("tools.apply_gemini_title")} message={t("tools.apply_gemini_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyGemini} onCancel={() => setConfirmApplyGemini(false)} />
      <ConfirmDialog open={confirmApplyAtomCode} title={t("tools.apply_atomcode_title")} message={t("tools.apply_atomcode_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyAtomCode} onCancel={() => setConfirmApplyAtomCode(false)} />
    </div>
  );
}
