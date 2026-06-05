import { useState, useEffect, useCallback, useMemo } from "react";
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
  Monitor,
  Eye,
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
import type {
  CodexConfigStatus,
  ClaudeCodeEnvStatus,
  OpenCodeConfigStatus,
  GeminiCliConfigStatus,
  AtomCodeConfigStatus,
  ClaudeDesktopStatus,
} from "@/types/config";
import type { GatewayStatus } from "@/types/gateway";

/// Master-detail 布局：左侧 5 行客户端列表常驻显示状态，右侧渲染选中客户端
/// 的完整详情。比原先的手风琴更适合「同时管理 5 个客户端」的场景——总览不
/// 丢失、详情区不再被卡片 chrome 切碎。
type ClientId = "codex" | "claude_code" | "opencode" | "gemini_cli" | "atomcode" | "claude_desktop";

/// 把每个客户端在「列表行」上需要的状态压成统一三态：
/// - `active`：已接入 AgentGate
/// - `detected`：检测到配置但未接入 AgentGate
/// - `absent`：未检测到
type ClientPresence = "active" | "detected" | "absent";

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
  const [claudeDesktopStatus, setClaudeDesktopStatus] = useState<ClaudeDesktopStatus | null>(null);
  const [cdPreview, setCdPreview] = useState("");
  const [historyClients, setHistoryClients] = useState<string[]>([]);
  const [gatewayStatus, setGatewayStatus] = useState<GatewayStatus | null>(null);
  const [startingGateway, setStartingGateway] = useState(false);

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

  // 当前选中的客户端。默认选第一个「已应用 / 检测到」的客户端，没有则回退
  // 到 codex（catalog 的第一项）。用 sessionStorage 记住一下，刷新不丢。
  const [selectedClientId, setSelectedClientId] = useState<ClientId>(() => {
    const saved = sessionStorage.getItem("agentgate_tools_selected") as ClientId | null;
    return saved ?? "codex";
  });
  useEffect(() => {
    sessionStorage.setItem("agentgate_tools_selected", selectedClientId);
  }, [selectedClientId]);

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
      const [c, cc, oc, gc, ac, cd, gw, hist] = await Promise.all([
        api.detectCodexConfig(),
        api.detectClaudeCodeEnv(),
        api.detectOpenCodeConfig(),
        api.detectGeminiConfig(),
        api.detectAtomCodeConfig(),
        api.detectClaudeDesktop().catch(() => null),
        api.getGatewayStatus(),
        api.clientsWithApplyHistory().catch(() => [] as string[]),
      ]);
      setCodexStatus(c);
      setClaudeEnv(cc);
      setOpenCodeStatus(oc);
      setGeminiStatus(gc);
      setAtomCodeStatus(ac);
      setClaudeDesktopStatus(cd);
      setGatewayStatus(gw);
      setHistoryClients(hist);
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

  const handleApplyClaudeDesktop = async () => {
    try {
      const result = await api.applyClaudeDesktopConfig();
      load();
      if (result.success) {
        await showPostApply("claude_desktop", "Claude Desktop", result.profile_path);
      }
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handlePreviewClaudeDesktop = async () => {
    try {
      setCdPreview(await api.previewClaudeDesktopProfile());
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

  // 列表行的元数据。每个客户端的 presence 直接从对应 status 推断，避免
  // 列表/详情两边对「是否检测到」的判定标准不一致。
  const clientRows: {
    id: ClientId;
    name: string;
    desc: string;
    icon: React.ComponentType<{ className?: string }>;
    presence: ClientPresence;
    drifted: boolean;
  }[] = useMemo(() => {
    // 配置漂移:接入过(在 apply 历史里)但当前掉成 detected,说明配置被改回去了。
    // 注意 Gemini CLI 在 apply 历史里 client_id 存的是 "gemini",和前端 id 不一致。
    const drifted = (id: ClientId, presence: ClientPresence) => {
      const histId = id === "gemini_cli" ? "gemini" : id;
      return presence === "detected" && historyClients.includes(histId);
    };
    const codexPresence: ClientPresence = codexStatus?.has_agentgate
      ? "active"
      : codexStatus?.exists
        ? "detected"
        : "absent";
    const claudePresence: ClientPresence = claudeEnv?.has_agentgate
      ? "active"
      : claudeEnv?.has_api_key || claudeEnv?.has_auth_token
        ? "detected"
        : "absent";
    const opencodePresence: ClientPresence = openCodeStatus?.has_agentgate
      ? "active"
      : openCodeStatus?.exists
        ? "detected"
        : "absent";
    const geminiPresence: ClientPresence = geminiStatus?.has_agentgate
      ? "active"
      : geminiStatus?.exists
        ? "detected"
        : "absent";
    const atomPresence: ClientPresence = atomCodeStatus?.has_agentgate
      ? "active"
      : atomCodeStatus?.exists
        ? "detected"
        : "absent";
    const claudeDesktopPresence: ClientPresence = claudeDesktopStatus?.has_agentgate_profile
      ? "active"
      : claudeDesktopStatus?.installed
        ? "detected"
        : "absent";
    return [
      { id: "codex", name: t("tools.codex"), desc: t("tools.codex_desc"), icon: Code, presence: codexPresence, drifted: drifted("codex", codexPresence) },
      { id: "claude_code", name: t("tools.claude_code"), desc: t("tools.claude_code_desc"), icon: Terminal, presence: claudePresence, drifted: drifted("claude_code", claudePresence) },
      { id: "opencode", name: t("tools.opencode"), desc: t("tools.opencode_desc"), icon: Braces, presence: opencodePresence, drifted: drifted("opencode", opencodePresence) },
      { id: "gemini_cli", name: t("tools.gemini_cli"), desc: t("tools.gemini_cli_desc"), icon: Sparkles, presence: geminiPresence, drifted: drifted("gemini_cli", geminiPresence) },
      { id: "atomcode", name: t("tools.atomcode"), desc: t("tools.atomcode_desc"), icon: Atom, presence: atomPresence, drifted: drifted("atomcode", atomPresence) },
      { id: "claude_desktop", name: "Claude Desktop", desc: "接入 Claude Desktop（实验·macOS）", icon: Monitor, presence: claudeDesktopPresence, drifted: drifted("claude_desktop", claudeDesktopPresence) },
    ];
  }, [codexStatus, claudeEnv, openCodeStatus, geminiStatus, atomCodeStatus, claudeDesktopStatus, historyClients, t]);

  if (loading) return <p className="text-xs text-text-muted">{t("common.loading")}</p>;

  return (
    <div className="space-y-5">
      {/* Connection Status Bar */}
      <div className="rounded-xl border border-border bg-card p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-6">
            <ConnectionStep label={t("tools.step_config")} ok={testResult?.config_ok ?? null} testing={testing} />
            <div className="h-px w-6 bg-border" />
            <ConnectionStep label={t("tools.step_gateway")} ok={testResult?.gateway_ok ?? null} testing={testing} />
            <div className="h-px w-6 bg-border" />
            <ConnectionStep label={t("tools.step_provider")} ok={testResult?.provider_ok ?? null} testing={testing} />
          </div>
          <button onClick={handleTestConnection} disabled={testing} className="btn-secondary">
            {testing ? <Loader2 className="h-3 w-3 animate-spin" /> : <Activity className="h-3 w-3" />}
            {t("tools.test_connection")}
          </button>
        </div>
        {gatewayStatus && (
          <div className={`mt-3 flex items-center justify-between rounded-md border px-3 py-2 ${
            gatewayStatus.running ? "border-success/30 bg-success-soft" : "border-warning/30 bg-warning/5"
          }`}>
            <p className={`text-xs ${gatewayStatus.running ? "text-success" : "text-warning"}`}>
              {gatewayStatus.running
                ? `${t("tools.gateway_running")} http://${gatewayStatus.host}:${gatewayStatus.port}`
                : t("tools.gateway_not_running_hint")}
            </p>
            {!gatewayStatus.running && (
              <button onClick={handleStartGateway} disabled={startingGateway} className="btn-primary">
                {startingGateway ? <Loader2 className="h-3 w-3 animate-spin" /> : <Activity className="h-3 w-3" />}
                {t("gateway.start")}
              </button>
            )}
          </div>
        )}
        {testResult?.error && <p className="mt-2 text-xs text-error">{testResult.error}</p>}
      </div>

      {/* Master-detail */}
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-[260px_minmax(0,1fr)]">
        {/* Left list */}
        <aside className="rounded-xl border border-border bg-card p-2">
          {/* 4.1 状态总汇 + 4.2 漂移提示 */}
          <div className="flex items-center justify-between px-2.5 py-1.5 text-[10px] text-text-muted">
            <span>客户端</span>
            <span>已接入 {clientRows.filter((r) => r.presence === "active").length}/{clientRows.length}</span>
          </div>
          {clientRows.some((r) => r.drifted) && (
            <div className="mb-1 px-2.5 text-[10px] text-warning">
              {clientRows.filter((r) => r.drifted).length} 个配置已变，建议重新应用
            </div>
          )}
          <ul className="space-y-1">
            {clientRows.map((row) => {
              const Icon = row.icon;
              const selected = selectedClientId === row.id;
              return (
                <li key={row.id}>
                  <button
                    type="button"
                    onClick={() => setSelectedClientId(row.id)}
                    className={
                      "flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left transition-colors " +
                      (selected
                        ? "bg-accent-soft text-accent"
                        : "text-text-secondary hover:bg-hover hover:text-text-primary")
                    }
                  >
                    <PresenceDot presence={row.presence} />
                    <Icon className="h-4 w-4 shrink-0" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-xs font-medium">{row.name}</div>
                      <div className={
                        "truncate text-[10px] " +
                        (row.drifted ? "text-warning" : selected ? "text-accent/80" : "text-text-muted")
                      }>
                        {row.drifted ? "配置已变·重新应用" : presenceLabel(row.presence, t)}
                      </div>
                    </div>
                  </button>
                </li>
              );
            })}
          </ul>
        </aside>

        {/* Right detail */}
        <section className="min-w-0">
          {selectedClientId === "codex" && (
            <CodexDetail
              status={codexStatus}
              codexConfig={codexConfig}
              onApply={() => setConfirmApplyCodex(true)}
              onToggle={handleToggleCodex}
              load={load}
              t={t}
            />
          )}
          {selectedClientId === "claude_code" && (
            <ClaudeDetail
              env={claudeEnv}
              snippet={claudeSnippet}
              onApply={() => setConfirmApplyClaude(true)}
              onToggle={handleToggleClaude}
              onGenerateSnippet={handleGenerateClaudeSnippet}
              load={load}
              t={t}
            />
          )}
          {selectedClientId === "opencode" && (
            <OpenCodeDetail
              status={openCodeStatus}
              onApply={() => setConfirmApplyOpenCode(true)}
              load={load}
              t={t}
            />
          )}
          {selectedClientId === "gemini_cli" && (
            <GeminiDetail
              status={geminiStatus}
              onApply={() => setConfirmApplyGemini(true)}
              onToggle={handleToggleGemini}
              load={load}
              t={t}
            />
          )}
          {selectedClientId === "atomcode" && (
            <AtomCodeDetail
              status={atomCodeStatus}
              onApply={() => setConfirmApplyAtomCode(true)}
              onToggle={handleToggleAtomCode}
              load={load}
              t={t}
            />
          )}
          {selectedClientId === "claude_desktop" && (
            <div className="rounded-xl border border-border bg-card p-5">
              <DetailHeader
                Icon={Monitor}
                name="Claude Desktop"
                desc="把第三方推理网关指向 AgentGate（仅 macOS，需先启用过一次第三方网关）"
                badge={
                  <StatusBadge variant={claudeDesktopStatus?.has_agentgate_profile ? "success" : claudeDesktopStatus?.installed ? "warning" : "muted"}>
                    {claudeDesktopStatus?.has_agentgate_profile ? "已接入 AgentGate" : claudeDesktopStatus?.installed ? "未接入" : "未检测到"}
                  </StatusBadge>
                }
              />

              {!claudeDesktopStatus?.supported ? (
                <p className="text-xs text-error">当前平台不支持（仅 macOS）。</p>
              ) : !claudeDesktopStatus?.installed ? (
                <p className="text-xs text-text-muted">未检测到 Claude Desktop。</p>
              ) : (
                <>
                  <div className="mb-4 text-xs">
                    <span className="text-text-muted">配置文件（3p profile）</span>
                    <p className="break-all font-mono text-[11px] text-text-secondary">{claudeDesktopStatus.profile_path}</p>
                  </div>

                  <div className="flex flex-wrap gap-2">
                    <button onClick={handleApplyClaudeDesktop} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
                    <button onClick={handlePreviewClaudeDesktop} className="btn-secondary"><Eye className="h-3 w-3" />预览 profile</button>
                    <ClientHistoryButton clientId="claude_desktop" clientName="Claude Desktop" onRollbackDone={load} />
                  </div>

                  {cdPreview && (
                    <pre className="mt-3 max-h-60 overflow-auto rounded-md bg-card-secondary p-3 text-[11px] text-text-primary">{cdPreview}</pre>
                  )}
                  <p className="mt-3 text-[11px] text-text-muted">应用后请重启 Claude Desktop 生效。要还原，用上面的历史回滚。</p>
                </>
              )}
            </div>
          )}
        </section>
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

// ── Helpers ────────────────────────────────────────────────────

function PresenceDot({ presence }: { presence: ClientPresence }) {
  const cls = presence === "active"
    ? "bg-success"
    : presence === "detected"
      ? "bg-warning"
      : "bg-border";
  return <span className={`h-2 w-2 shrink-0 rounded-full ${cls}`} />;
}

function presenceLabel(p: ClientPresence, t: (k: string) => string): string {
  switch (p) {
    case "active": return t("tools.agentgate_configured");
    case "detected": return t("tools.not_configured");
    case "absent": return t("tools.no_config");
  }
}

type T = (k: string) => string;

/// 详情区共用的页眉：图标 + 标题 + 描述 + 状态徽章。
function DetailHeader({
  Icon, name, desc, badge,
}: {
  Icon: React.ComponentType<{ className?: string }>;
  name: string;
  desc: string;
  badge: React.ReactNode;
}) {
  return (
    <div className="mb-4 flex items-start justify-between gap-3">
      <div className="flex items-center gap-3">
        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
          <Icon className="h-5 w-5 text-accent" />
        </div>
        <div>
          <h3 className="text-sm font-semibold text-text-primary">{name}</h3>
          <p className="text-xs text-text-muted">{desc}</p>
        </div>
      </div>
      <div>{badge}</div>
    </div>
  );
}

// ── Per-client detail components ───────────────────────────────

function CodexDetail({
  status, codexConfig, onApply, onToggle, load, t,
}: {
  status: CodexConfigStatus | null;
  codexConfig: string;
  onApply: () => void;
  onToggle: () => void;
  load: () => void;
  t: T;
}) {
  const badge = (
    <StatusBadge variant={status?.has_agentgate ? "success" : status?.exists ? "warning" : "muted"}>
      {status?.has_agentgate ? t("tools.agentgate_configured") : status?.exists ? t("tools.not_configured") : t("tools.no_config")}
    </StatusBadge>
  );

  return (
    <div className="rounded-xl border border-border bg-card p-5">
      <DetailHeader Icon={Code} name={t("tools.codex")} desc={t("tools.codex_desc")} badge={badge} />

      {status && (
        <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
          <div><span className="text-text-muted">config.toml</span><p className="font-mono text-text-secondary text-[11px]">{status.config_path}</p></div>
          <div><span className="text-text-muted">{t("tools.current_provider")}</span><p className="text-text-primary">{status.current_provider ?? "—"}</p></div>
          <div><span className="text-text-muted">auth.json</span><p className="font-mono text-text-secondary text-[11px]">{status.auth_json_path}</p></div>
          <div><span className="text-text-muted">{t("tools.auth_status")}</span><p className="flex items-center gap-1 text-text-primary"><Shield className="h-3 w-3 text-accent" />{status.has_agentgate_auth ? t("tools.token_set") : t("tools.not_configured")}</p></div>
        </div>
      )}

      {status?.openai_key_polluted && (
        <div className="mb-3 rounded-md border border-warning/30 bg-warning/5 p-3">
          <div className="flex items-center gap-2 text-xs font-medium text-warning">
            <AlertTriangle className="h-3.5 w-3.5" />
            {t("tools.openai_key_polluted")}
          </div>
          <p className="mt-1 text-[11px] text-text-secondary">{t("tools.openai_key_polluted_desc")}</p>
        </div>
      )}

      {status?.is_agentgate_active && (
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

      {!status?.is_agentgate_active && status?.exists && (
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

      <div className="flex flex-wrap gap-2">
        <button onClick={onApply} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
        {status?.is_agentgate_active && status?.has_saved_official && (
          <button onClick={onToggle} className="btn-secondary">
            <ToggleRight className="h-3 w-3" />{t("tools.switch_to_official")}
          </button>
        )}
        {!status?.is_agentgate_active && status?.has_agentgate && (
          <button onClick={onToggle} className="btn-primary">
            <ToggleLeft className="h-3 w-3" />{t("tools.switch_to_agentgate")}
          </button>
        )}
        {status?.exists && (
          <button onClick={() => api.openCodexConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
        )}
        <ClientHistoryButton clientId="codex" clientName="Codex" onRollbackDone={load} />
        <CopyButton text={codexConfig} />
      </div>
    </div>
  );
}

function ClaudeDetail({
  env, snippet, onApply, onToggle, onGenerateSnippet, load, t,
}: {
  env: ClaudeCodeEnvStatus | null;
  snippet: string;
  onApply: () => void;
  onToggle: () => void;
  onGenerateSnippet: () => void;
  load: () => void;
  t: T;
}) {
  const badge = (
    <StatusBadge variant={env?.has_agentgate ? "success" : env?.has_api_key || env?.has_auth_token ? "warning" : "muted"}>
      {env?.has_agentgate ? t("tools.agentgate_configured") : env?.has_api_key || env?.has_auth_token ? t("tools.direct_credentials") : t("tools.no_credentials")}
    </StatusBadge>
  );

  return (
    <div className="rounded-xl border border-border bg-card p-5">
      <DetailHeader Icon={Terminal} name={t("tools.claude_code")} desc={t("tools.claude_code_desc")} badge={badge} />

      {env && (
        <>
          <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
            <div><span className="text-text-muted">Settings Path</span><p className="font-mono text-text-secondary text-[11px]">{env.settings_path}</p></div>
            <div><span className="text-text-muted">{t("settings.auth_mode")}</span><p className="flex items-center gap-1 text-text-primary"><Shield className="h-3 w-3 text-accent" />{env.auth_mode}</p></div>
            <div><span className="text-text-muted">{t("providers.base_url")}</span><p className="font-mono text-text-secondary">{env.active_base_url ?? "default"}</p></div>
            <div><span className="text-text-muted">{t("logs.model")}</span><p className="font-mono text-text-primary">{env.active_model ?? "default"}</p></div>
          </div>

          {env.conflicts.length > 0 && (
            <div className="mb-4 rounded-md border border-warning/30 bg-warning/5 p-3">
              <div className="flex items-center gap-2 text-xs font-medium text-warning"><AlertTriangle className="h-3.5 w-3.5" />{env.conflicts.length} {t("tools.conflicts_detected")}</div>
              {env.conflicts.map((c, i) => <p key={i} className="mt-1 text-[11px] text-text-secondary">{c}</p>)}
            </div>
          )}
        </>
      )}

      <p className="mb-3 text-[11px] text-text-muted">{t("tools.claude_auth_desc")}</p>

      <div className="mb-4 flex flex-wrap gap-2">
        <button onClick={onApply} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
        {env?.has_agentgate && env?.has_saved_official && (
          <button onClick={onToggle} className="btn-secondary">
            <ToggleRight className="h-3 w-3" />{t("tools.switch_to_official")}
          </button>
        )}
        {!env?.has_agentgate && env?.has_saved_official && (
          <button onClick={onToggle} className="btn-primary">
            <ToggleLeft className="h-3 w-3" />{t("tools.switch_to_agentgate")}
          </button>
        )}
        {env?.settings_exists && (
          <button onClick={() => api.openClaudeCodeConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
        )}
        <ClientHistoryButton clientId="claude_code" clientName="Claude Code" onRollbackDone={load} />
        <button onClick={onGenerateSnippet} className="btn-secondary"><Code className="h-3 w-3" />{t("tools.env_snippet")}</button>
      </div>

      {snippet && <JsonCodeBlock title="Claude Code Environment" content={snippet} language="bash" />}
    </div>
  );
}

function OpenCodeDetail({
  status, onApply, load, t,
}: {
  status: OpenCodeConfigStatus | null;
  onApply: () => void;
  load: () => void;
  t: T;
}) {
  const badge = (
    <StatusBadge variant={status?.has_agentgate ? "success" : status?.exists ? "warning" : "muted"}>
      {status?.has_agentgate ? t("tools.agentgate_configured") : status?.exists ? t("tools.not_configured") : t("tools.no_config")}
    </StatusBadge>
  );

  return (
    <div className="rounded-xl border border-border bg-card p-5">
      <DetailHeader Icon={Braces} name={t("tools.opencode")} desc={t("tools.opencode_desc")} badge={badge} />

      {status && (
        <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
          <div><span className="text-text-muted">opencode.json</span><p className="font-mono text-text-secondary text-[11px]">{status.config_path}</p></div>
          <div><span className="text-text-muted">{t("logs.model")}</span><p className="text-text-primary">{status.current_model ?? "—"}</p></div>
        </div>
      )}

      <p className="mb-3 text-[11px] text-text-muted">{t("tools.opencode_auth_desc")}</p>

      <div className="flex flex-wrap gap-2">
        <button onClick={onApply} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
        {status?.exists && (
          <button onClick={() => api.openOpenCodeConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
        )}
        <ClientHistoryButton clientId="opencode" clientName="OpenCode" onRollbackDone={load} />
      </div>
    </div>
  );
}

function GeminiDetail({
  status, onApply, onToggle, load, t,
}: {
  status: GeminiCliConfigStatus | null;
  onApply: () => void;
  onToggle: () => void;
  load: () => void;
  t: T;
}) {
  const badge = (
    <StatusBadge variant={status?.has_agentgate ? "success" : status?.exists ? "warning" : "muted"}>
      {status?.has_agentgate ? t("tools.agentgate_configured") : status?.exists ? t("tools.not_configured") : t("tools.no_config")}
    </StatusBadge>
  );

  return (
    <div className="rounded-xl border border-border bg-card p-5">
      <DetailHeader Icon={Sparkles} name={t("tools.gemini_cli")} desc={t("tools.gemini_cli_desc")} badge={badge} />

      {status && (
        <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
          <div><span className="text-text-muted">settings.json</span><p className="font-mono text-text-secondary text-[11px]">{status.config_path}</p></div>
          <div><span className="text-text-muted">{t("logs.model")}</span><p className="text-text-primary">{status.current_model ?? "—"}</p></div>
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <button onClick={onApply} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
        {status?.has_agentgate && status?.has_saved_official && (
          <button onClick={onToggle} className="btn-secondary"><ToggleRight className="h-3 w-3" />{t("tools.switch_to_official")}</button>
        )}
        {!status?.has_agentgate && status?.has_saved_official && (
          <button onClick={onToggle} className="btn-primary"><ToggleLeft className="h-3 w-3" />{t("tools.switch_to_agentgate")}</button>
        )}
        {status?.exists && (
          <button onClick={() => api.openGeminiConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
        )}
        <ClientHistoryButton clientId="gemini" clientName="Gemini CLI" onRollbackDone={load} />
      </div>
    </div>
  );
}

function AtomCodeDetail({
  status, onApply, onToggle, load, t,
}: {
  status: AtomCodeConfigStatus | null;
  onApply: () => void;
  onToggle: () => void;
  load: () => void;
  t: T;
}) {
  const badge = (
    <StatusBadge variant={status?.has_agentgate ? "success" : status?.exists ? "warning" : "muted"}>
      {status?.has_agentgate ? t("tools.agentgate_configured") : status?.exists ? t("tools.not_configured") : t("tools.no_config")}
    </StatusBadge>
  );

  return (
    <div className="rounded-xl border border-border bg-card p-5">
      <DetailHeader Icon={Atom} name={t("tools.atomcode")} desc={t("tools.atomcode_desc")} badge={badge} />

      {status && (
        <div className="mb-4 grid grid-cols-2 gap-y-2 text-xs">
          <div><span className="text-text-muted">config.toml</span><p className="font-mono text-text-secondary text-[11px]">{status.config_path}</p></div>
          <div><span className="text-text-muted">{t("logs.model")}</span><p className="text-text-primary">{status.current_model ?? "—"}</p></div>
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <button onClick={onApply} className="btn-primary"><Zap className="h-3 w-3" />{t("tools.apply_config")}</button>
        {status?.has_agentgate && status?.has_saved_official && (
          <button onClick={onToggle} className="btn-secondary"><ToggleRight className="h-3 w-3" />{t("tools.switch_to_official")}</button>
        )}
        {!status?.has_agentgate && status?.has_saved_official && (
          <button onClick={onToggle} className="btn-primary"><ToggleLeft className="h-3 w-3" />{t("tools.switch_to_agentgate")}</button>
        )}
        {status?.exists && (
          <button onClick={() => api.openAtomCodeConfig()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("tools.open")}</button>
        )}
        <ClientHistoryButton clientId="atomcode" clientName="AtomCode" onRollbackDone={load} />
      </div>
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
