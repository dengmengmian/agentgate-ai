import { Code, FolderOpen, AlertTriangle, Zap, Shield, ToggleLeft, ToggleRight } from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { CopyButton } from "@/components/common/CopyButton";
import { ClientHistoryButton } from "@/components/tools/ClientHistoryButton";
import * as api from "@/lib/api";
import type { CodexConfigStatus } from "@/types/config";
import { DetailHeader, type T } from "@/pages/Tools";

export function CodexDetail({
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
