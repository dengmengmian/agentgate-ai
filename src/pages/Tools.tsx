import { useState, useEffect, useCallback } from "react";
import {
  Terminal,
  Code,
  Braces,
  Eye,
  Download,
  FolderOpen,
  AlertTriangle,
  Zap,
  Shield,
} from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { JsonCodeBlock } from "@/components/common/JsonCodeBlock";
import { CopyButton } from "@/components/common/CopyButton";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { ToolConfigView } from "@/types/tool";
import type { CodexConfigStatus, ClaudeCodeEnvStatus } from "@/types/config";

const iconMap: Record<string, React.ElementType> = {
  terminal: Terminal,
  code: Code,
  braces: Braces,
};

export function Tools() {
  const { t } = useI18n();
  const [tools, setTools] = useState<ToolConfigView[]>([]);
  const [codexStatus, setCodexStatus] = useState<CodexConfigStatus | null>(null);
  const [claudeEnv, setClaudeEnv] = useState<ClaudeCodeEnvStatus | null>(null);
  const [codexConfig, setCodexConfig] = useState("");
  const [claudeSnippet, setClaudeSnippet] = useState("");
  const [loading, setLoading] = useState(true);
  const [showCodexPreview, setShowCodexPreview] = useState(false);
  const [confirmApplyCodex, setConfirmApplyCodex] = useState(false);
  const [confirmApplyClaude, setConfirmApplyClaude] = useState(false);

  const load = useCallback(async () => {
    try {
      const [t, c, cc] = await Promise.all([
        api.listTools(),
        api.detectCodexConfig(),
        api.detectClaudeCodeEnv(),
      ]);
      setTools(t);
      setCodexStatus(c);
      setClaudeEnv(cc);
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
        if (result.backup_path) toast("success", `${t("tools.backup_saved")}: ${result.backup_path.split("/").pop()}`);
      }
      setConfirmApplyCodex(false);
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleApplyClaude = async () => {
    try {
      const result = await api.applyClaudeCodeConfig();
      if (result.success) {
        toast("success", `Claude Code ${t("tools.config_written")} ${result.config_path}`);
        if (result.backup_path) toast("success", `${t("tools.backup_saved")}: ${result.backup_path.split("/").pop()}`);
      }
      setConfirmApplyClaude(false);
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleBackupCodex = async () => {
    try { await api.backupCodexConfig(); toast("success", `Codex ${t("tools.backed_up")}`); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleBackupClaude = async () => {
    try { await api.backupClaudeCodeConfig(); toast("success", `Claude Code ${t("tools.backed_up")}`); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleGenerateClaudeSnippet = async () => {
    try {
      const snippet = await api.generateClaudeCodeEnv();
      setClaudeSnippet(snippet);
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  if (loading) return <p className="text-xs text-text-muted">{t("common.loading")}</p>;

  return (
    <div className="space-y-6">
      {/* Codex Card */}
      <div className="rounded-lg border border-border bg-card p-5">
        <div className="mb-4 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent/10">
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

        <p className="mb-3 text-[11px] text-text-muted">{t("tools.codex_auth_desc")}</p>

        <div className="mb-4 flex flex-wrap gap-2">
          <button onClick={() => setShowCodexPreview(!showCodexPreview)} className="btn-secondary"><Eye className="h-3 w-3" />{t("tools.preview")}</button>
          <button onClick={() => setConfirmApplyCodex(true)} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
          {codexStatus?.exists && (
            <>
              <button onClick={handleBackupCodex} className="btn-secondary"><Download className="h-3 w-3" />{t("tools.backup")}</button>
              <button onClick={() => api.openCodexConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
            </>
          )}
          <CopyButton text={codexConfig} />
        </div>

        {showCodexPreview && codexConfig && (
          <div className="space-y-3">
            <JsonCodeBlock title="Proposed ~/.codex/config.toml" content={codexConfig} language="toml" />
            <JsonCodeBlock title="Proposed ~/.codex/auth.json" content={'{\n  "OPENAI_API_KEY": "<ag_local_token>"\n}'} language="json" />
          </div>
        )}
      </div>

      {/* Claude Code Card */}
      <div className="rounded-lg border border-border bg-card p-5">
        <div className="mb-4 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent/10">
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
          {claudeEnv?.settings_exists && (
            <>
              <button onClick={handleBackupClaude} className="btn-secondary"><Download className="h-3 w-3" />{t("tools.backup")}</button>
              <button onClick={() => api.openClaudeCodeConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
            </>
          )}
          <button onClick={handleGenerateClaudeSnippet} className="btn-secondary"><Code className="h-3 w-3" />{t("tools.env_snippet")}</button>
        </div>

        {claudeSnippet && <JsonCodeBlock title="Claude Code Environment" content={claudeSnippet} language="bash" />}
      </div>

      {/* OpenCode Card */}
      {tools.filter(t => t.slug === "opencode").map(tool => {
        const Icon = iconMap[tool.icon] ?? Braces;
        return (
          <div key={tool.id} className="rounded-lg border border-border bg-card p-5">
            <div className="flex items-start justify-between">
              <div className="flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent/10"><Icon className="h-5 w-5 text-accent" /></div>
                <div><h3 className="text-sm font-semibold text-text-primary">{tool.name}</h3><p className="text-xs text-text-muted">{tool.description}</p></div>
              </div>
              <StatusBadge variant={tool.config_exists ? "success" : "muted"}>{tool.config_exists ? t("tools.config_found") : t("tools.no_config")}</StatusBadge>
            </div>
            <p className="mt-3 text-xs text-text-muted">Config: <span className="font-mono">{tool.config_path}</span></p>
          </div>
        );
      })}

      <ConfirmDialog open={confirmApplyCodex} title={t("tools.apply_codex_title")} message={t("tools.apply_codex_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyCodex} onCancel={() => setConfirmApplyCodex(false)} />
      <ConfirmDialog open={confirmApplyClaude} title={t("tools.apply_claude_title")} message={t("tools.apply_claude_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyClaude} onCancel={() => setConfirmApplyClaude(false)} />
    </div>
  );
}
