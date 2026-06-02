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
  Activity,
  CheckCircle,
  XCircle,
  Loader2,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { JsonCodeBlock } from "@/components/common/JsonCodeBlock";
import { CopyButton } from "@/components/common/CopyButton";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { PostApplyDialog } from "@/components/tools/PostApplyDialog";
import { ClientHistoryButton } from "@/components/tools/ClientHistoryButton";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import { usePolling } from "@/lib/usePolling";
import * as api from "@/lib/api";
import type { CodexConfigStatus, ClaudeCodeEnvStatus, OpenCodeConfigStatus, GeminiCliConfigStatus, AtomCodeConfigStatus } from "@/types/config";
import type { GatewayStatus } from "@/types/gateway";

export function Tools() {
  const { t } = useI18n();
  const [codexStatus, setCodexStatus] = useState<CodexConfigStatus | null>(null);
  const [claudeEnv, setClaudeEnv] = useState<ClaudeCodeEnvStatus | null>(null);
  const [codexConfig, setCodexConfig] = useState("");
  const [claudeSnippet, setClaudeSnippet] = useState("");
  const [loading, setLoading] = useState(true);
  const [testResult, setTestResult] = useState<api.ConnectionTestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [openCodeStatus, setOpenCodeStatus] = useState<OpenCodeConfigStatus | null>(null);
  const [geminiStatus, setGeminiStatus] = useState<GeminiCliConfigStatus | null>(null);
  const [atomCodeStatus, setAtomCodeStatus] = useState<AtomCodeConfigStatus | null>(null);
  const [gatewayStatus, setGatewayStatus] = useState<GatewayStatus | null>(null);
  const [startingGateway, setStartingGateway] = useState(false);

  // 未检测到的 client 默认折叠——5 个全展开屏幕太长，用户只用 1-2 个。
  // 检测到的 client 默认展开。用户可以手动点 chevron 切换任一 client 状态。
  const [expandedClients, setExpandedClients] = useState<Record<string, boolean>>({});
  const toggleExpanded = (id: string) => setExpandedClients(s => ({ ...s, [id]: !(s[id] ?? false) }));
  const isExpanded = (id: string, detected: boolean) =>
    expandedClients[id] !== undefined ? expandedClients[id] : detected;
  const [confirmApplyCodex, setConfirmApplyCodex] = useState(false);
  const [confirmApplyClaude, setConfirmApplyClaude] = useState(false);
  const [confirmApplyOpenCode, setConfirmApplyOpenCode] = useState(false);
  const [confirmApplyGemini, setConfirmApplyGemini] = useState(false);
  const [confirmApplyAtomCode, setConfirmApplyAtomCode] = useState(false);

  /// Post-apply summary: shown once per apply with config path + running
  /// process warning. Null means "no dialog open right now". Detect failure
  /// degrades to processes=[] so the dialog still shows the success state.
  const [postApply, setPostApply] = useState<{
    clientId: string;
    clientName: string;
    configPath: string;
    processes: api.RunningProcess[];
  } | null>(null);

  const showPostApply = async (
    clientId: string,
    clientName: string,
    configPath: string,
  ) => {
    let processes: api.RunningProcess[] = [];
    try {
      processes = await api.detectClientRunning(clientId);
    } catch {
      // Detection is best-effort. Permission denied / Windows / pgrep
      // missing all degrade to "we don't know" — dialog renders without
      // the warning band.
    }
    setPostApply({ clientId, clientName, configPath, processes });
  };

  const load = useCallback(async () => {
    try {
      const [c, cc, oc, gc, ac, gw] = await Promise.all([
        api.detectCodexConfig(),
        api.detectClaudeCodeEnv(),
        api.detectOpenCodeConfig(),
        api.detectGeminiConfig(),
        api.detectAtomCodeConfig(),
        api.getGatewayStatus(),
      ]);
      setCodexStatus(c);
      setClaudeEnv(cc);
      setOpenCodeStatus(oc);
      setGeminiStatus(gc);
      setAtomCodeStatus(ac);
      setGatewayStatus(gw);
      const snippet = await api.generateCodexConfig();
      setCodexConfig(snippet);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);
  // window focus 时刷新——从终端切回时立刻看到 Codex 应用配置后的状态变化
  usePolling(load, 15_000);

  const handleApplyCodex = async () => {
    try {
      const result = await api.applyCodexConfig();
      setConfirmApplyCodex(false);
      load();
      if (result.success) {
        await showPostApply("codex", "Codex", result.config_path);
      }
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

  // `disable_codex_agentgate` is exposed via api.disableCodexAgentgate() and
  // does the same restore as `handleToggleCodex` going compat → native (the
  // existing "切换到官方" button covers it). Kept as a backend primitive for
  // future direct callers; UI keeps the single toggle.

  const handleApplyClaude = async () => {
    try {
      const result = await api.applyClaudeCodeConfig();
      setConfirmApplyClaude(false);
      load();
      if (result.success) {
        await showPostApply("claude_code", "Claude Code", result.config_path);
      }
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
      setConfirmApplyOpenCode(false);
      load();
      if (result.success) {
        await showPostApply("opencode", "OpenCode", result.config_path);
      }
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
      setConfirmApplyGemini(false);
      load();
      if (result.success) {
        await showPostApply("gemini", "Gemini CLI", result.config_path);
      }
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
      setConfirmApplyAtomCode(false);
      load();
      if (result.success) {
        await showPostApply("atomcode", "AtomCode", result.config_path);
      }
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

  const handleTestConnection = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const result = await api.testToolConnection();
      setTestResult(result);
    } catch {
      setTestResult({ config_ok: false, gateway_ok: false, provider_ok: false, error: "Test failed" });
    } finally {
      setTesting(false);
    }
  };

  const handleStartGateway = async () => {
    setStartingGateway(true);
    try {
      const status = await api.startGateway();
      setGatewayStatus(status);
      toast("success", t("gateway.started"));
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setStartingGateway(false);
      load();
    }
  };

  if (loading) return <p className="text-xs text-text-muted">{t("common.loading")}</p>;

  return (
    <div className="space-y-6">
      {/* Connection Status Bar */}
      <div className="rounded-xl border border-border bg-card p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-6">
            <ConnectionStep
              label={t("tools.step_config")}
              ok={testResult?.config_ok ?? null}
              testing={testing}
            />
            <div className="h-px w-6 bg-border" />
            <ConnectionStep
              label={t("tools.step_gateway")}
              ok={testResult?.gateway_ok ?? null}
              testing={testing}
            />
            <div className="h-px w-6 bg-border" />
            <ConnectionStep
              label={t("tools.step_provider")}
              ok={testResult?.provider_ok ?? null}
              testing={testing}
            />
          </div>
          <button
            onClick={handleTestConnection}
            disabled={testing}
            className="btn-secondary"
          >
            {testing ? <Loader2 className="h-3 w-3 animate-spin" /> : <Activity className="h-3 w-3" />}
            {t("tools.test_connection")}
          </button>
        </div>
        {gatewayStatus && (
          <div className={`mt-3 flex items-center justify-between rounded-md border px-3 py-2 ${
            gatewayStatus.running
              ? "border-success/30 bg-success-soft"
              : "border-warning/30 bg-warning/5"
          }`}>
            <p className={`text-xs ${gatewayStatus.running ? "text-success" : "text-warning"}`}>
              {gatewayStatus.running
                ? `${t("tools.gateway_running")} http://${gatewayStatus.host}:${gatewayStatus.port}`
                : t("tools.gateway_not_running_hint")}
            </p>
            {!gatewayStatus.running && (
              <button
                onClick={handleStartGateway}
                disabled={startingGateway}
                className="btn-primary"
              >
                {startingGateway ? <Loader2 className="h-3 w-3 animate-spin" /> : <Activity className="h-3 w-3" />}
                {t("gateway.start")}
              </button>
            )}
          </div>
        )}
        {testResult?.error && (
          <p className="mt-2 text-xs text-error">{testResult.error}</p>
        )}
      </div>

      {/* Codex Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <button
          type="button"
          onClick={() => toggleExpanded("codex")}
          className="mb-4 flex w-full items-start justify-between text-left"
        >
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Code className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.codex")}</h3>
              <p className="text-xs text-text-muted">{t("tools.codex_desc")}</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge variant={codexStatus?.has_agentgate ? "success" : codexStatus?.exists ? "warning" : "muted"}>
              {codexStatus?.has_agentgate ? t("tools.agentgate_configured") : codexStatus?.exists ? t("tools.not_configured") : t("tools.no_config")}
            </StatusBadge>
            {isExpanded("codex", !!codexStatus?.exists) ? <ChevronDown className="h-4 w-4 text-text-muted" /> : <ChevronRight className="h-4 w-4 text-text-muted" />}
          </div>
        </button>

        {isExpanded("codex", !!codexStatus?.exists) && <>
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

        {codexStatus?.is_agentgate_active && (
          <div className="mb-3 rounded-md border border-success/30 bg-success-soft p-3">
            <div className="flex items-center gap-2 text-xs font-medium text-success">
              <Shield className="h-3.5 w-3.5" />
              代理模式已启用：对话走 AgentGate · IDE 插件继续可用
            </div>
            <p className="mt-1 text-[11px] text-text-secondary">
              当前配置使用「劫持 OpenAI provider + <code className="font-mono">requires_openai_auth</code>」方案：
              对话请求路由到 AgentGate（→ 第三方模型），同时保留 ChatGPT 官方登录态 —
              Browser / Computer-Use / Mobile / 配额查询 全部可用。<br />
              要切回 Codex 直连 ChatGPT 官方，点击 "切换到官方"。
            </p>
          </div>
        )}

        {!codexStatus?.is_agentgate_active && codexStatus?.exists && (
          <div className="mb-3 rounded-md border border-border bg-card-secondary p-3">
            <div className="text-xs font-medium text-text-primary">
              原生模式：Codex 直连 ChatGPT 官方
            </div>
            <p className="mt-1 text-[11px] text-text-secondary">
              当前不经过 AgentGate。如需路由到 MiMo / DeepSeek / Kimi 等第三方模型，
              点击 "应用配置" 切换到代理模式 —— 切换后 IDE 插件 / Codex Mobile 仍可正常使用。
            </p>
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
          <ClientHistoryButton clientId="codex" clientName="Codex" onRollbackDone={load} />
          <CopyButton text={codexConfig} />
        </div>
        </>}
      </div>

      {/* Claude Code Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <button
          type="button"
          onClick={() => toggleExpanded("claude_code")}
          className="mb-4 flex w-full items-start justify-between text-left"
        >
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Terminal className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.claude_code")}</h3>
              <p className="text-xs text-text-muted">{t("tools.claude_code_desc")}</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge variant={claudeEnv?.has_agentgate ? "success" : claudeEnv?.has_api_key || claudeEnv?.has_auth_token ? "warning" : "muted"}>
              {claudeEnv?.has_agentgate ? t("tools.agentgate_configured") : claudeEnv?.has_api_key || claudeEnv?.has_auth_token ? t("tools.direct_credentials") : t("tools.no_credentials")}
            </StatusBadge>
            {isExpanded("claude_code", !!claudeEnv?.settings_exists) ? <ChevronDown className="h-4 w-4 text-text-muted" /> : <ChevronRight className="h-4 w-4 text-text-muted" />}
          </div>
        </button>

        {isExpanded("claude_code", !!claudeEnv?.settings_exists) && <>
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
          <ClientHistoryButton clientId="claude_code" clientName="Claude Code" onRollbackDone={load} />
          <button onClick={handleGenerateClaudeSnippet} className="btn-secondary"><Code className="h-3 w-3" />{t("tools.env_snippet")}</button>
        </div>

        {claudeSnippet && <JsonCodeBlock title="Claude Code Environment" content={claudeSnippet} language="bash" />}
        </>}
      </div>

      {/* OpenCode Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <button
          type="button"
          onClick={() => toggleExpanded("opencode")}
          className="mb-4 flex w-full items-start justify-between text-left"
        >
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Braces className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.opencode")}</h3>
              <p className="text-xs text-text-muted">{t("tools.opencode_desc")}</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
          <StatusBadge variant={openCodeStatus?.has_agentgate ? "success" : openCodeStatus?.exists ? "warning" : "muted"}>
            {openCodeStatus?.has_agentgate ? t("tools.agentgate_configured") : openCodeStatus?.exists ? t("tools.not_configured") : t("tools.no_config")}
          </StatusBadge>
          {isExpanded("opencode", !!openCodeStatus?.exists) ? <ChevronDown className="h-4 w-4 text-text-muted" /> : <ChevronRight className="h-4 w-4 text-text-muted" />}
          </div>
        </button>

        {isExpanded("opencode", !!openCodeStatus?.exists) && <>
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
          <ClientHistoryButton clientId="opencode" clientName="OpenCode" onRollbackDone={load} />
        </div>
        </>}
      </div>

      {/* Gemini CLI Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <button
          type="button"
          onClick={() => toggleExpanded("gemini_cli")}
          className="mb-4 flex w-full items-start justify-between text-left"
        >
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Sparkles className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.gemini_cli")}</h3>
              <p className="text-xs text-text-muted">{t("tools.gemini_cli_desc")}</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge variant={geminiStatus?.has_agentgate ? "success" : geminiStatus?.exists ? "warning" : "muted"}>
              {geminiStatus?.has_agentgate ? t("tools.agentgate_configured") : geminiStatus?.exists ? t("tools.not_configured") : t("tools.no_config")}
            </StatusBadge>
            {isExpanded("gemini_cli", !!geminiStatus?.exists) ? <ChevronDown className="h-4 w-4 text-text-muted" /> : <ChevronRight className="h-4 w-4 text-text-muted" />}
          </div>
        </button>

        {isExpanded("gemini_cli", !!geminiStatus?.exists) && <>
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
          <ClientHistoryButton clientId="gemini" clientName="Gemini CLI" onRollbackDone={load} />
        </div>
        </>}
      </div>

      {/* AtomCode Card */}
      <div className="rounded-xl border border-border bg-card p-5">
        <button
          type="button"
          onClick={() => toggleExpanded("atomcode")}
          className="mb-4 flex w-full items-start justify-between text-left"
        >
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
              <Atom className="h-5 w-5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("tools.atomcode")}</h3>
              <p className="text-xs text-text-muted">{t("tools.atomcode_desc")}</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge variant={atomCodeStatus?.has_agentgate ? "success" : atomCodeStatus?.exists ? "warning" : "muted"}>
              {atomCodeStatus?.has_agentgate ? t("tools.agentgate_configured") : atomCodeStatus?.exists ? t("tools.not_configured") : t("tools.no_config")}
            </StatusBadge>
            {isExpanded("atomcode", !!atomCodeStatus?.exists) ? <ChevronDown className="h-4 w-4 text-text-muted" /> : <ChevronRight className="h-4 w-4 text-text-muted" />}
          </div>
        </button>

        {isExpanded("atomcode", !!atomCodeStatus?.exists) && <>
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
          <ClientHistoryButton clientId="atomcode" clientName="AtomCode" onRollbackDone={load} />
        </div>
        </>}
      </div>

      <ConfirmDialog open={confirmApplyCodex} title={t("tools.apply_codex_title")} message={t("tools.apply_codex_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyCodex} onCancel={() => setConfirmApplyCodex(false)} />
      <ConfirmDialog open={confirmApplyClaude} title={t("tools.apply_claude_title")} message={t("tools.apply_claude_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyClaude} onCancel={() => setConfirmApplyClaude(false)} />
      <ConfirmDialog open={confirmApplyOpenCode} title={t("tools.apply_opencode_title")} message={t("tools.apply_opencode_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyOpenCode} onCancel={() => setConfirmApplyOpenCode(false)} />
      <ConfirmDialog open={confirmApplyGemini} title={t("tools.apply_gemini_title")} message={t("tools.apply_gemini_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyGemini} onCancel={() => setConfirmApplyGemini(false)} />
      <ConfirmDialog open={confirmApplyAtomCode} title={t("tools.apply_atomcode_title")} message={t("tools.apply_atomcode_msg")} confirmLabel={t("common.apply")} variant="default" onConfirm={handleApplyAtomCode} onCancel={() => setConfirmApplyAtomCode(false)} />

      <PostApplyDialog
        open={postApply !== null}
        clientId={postApply?.clientId}
        clientName={postApply?.clientName ?? ""}
        configPath={postApply?.configPath ?? ""}
        processes={postApply?.processes ?? []}
        onClose={() => setPostApply(null)}
      />
    </div>
  );
}

function ConnectionStep({ label, ok, testing }: { label: string; ok: boolean | null; testing: boolean }) {
  return (
    <div className="flex items-center gap-2">
      {testing ? (
        <Loader2 className="h-4 w-4 animate-spin text-text-muted" />
      ) : ok === null ? (
        <div className="h-4 w-4 rounded-full border-2 border-border" />
      ) : ok ? (
        <CheckCircle className="h-4 w-4 text-success" />
      ) : (
        <XCircle className="h-4 w-4 text-error" />
      )}
      <span className={`text-xs ${ok === true ? "text-success" : ok === false ? "text-error" : "text-text-muted"}`}>
        {label}
      </span>
    </div>
  );
}
