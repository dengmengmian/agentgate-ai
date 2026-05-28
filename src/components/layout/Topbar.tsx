import { useState, useEffect, useCallback } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Stethoscope } from "lucide-react";
import * as api from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import type { GatewayStatus } from "@/types/gateway";

export function Topbar() {
  const { t } = useI18n();
  const location = useLocation();
  const navigate = useNavigate();

  const pageTitleKeys: Record<string, string> = {
    "/": "nav.overview",
    "/quick-setup": "nav.quick_setup",
    "/tools": "nav.clients",
    "/providers": "nav.providers",
    "/routes": "nav.routes",
    "/gateway": "nav.gateway",
    "/logs": "nav.logs",
    "/diagnostics": "nav.diagnostics",
    "/settings": "nav.settings",
  };

  const titleKey = pageTitleKeys[location.pathname] ?? "nav.overview";
  const [status, setStatus] = useState<GatewayStatus | null>(null);

  const refresh = useCallback(() => {
    api.getGatewayStatus().then(setStatus).catch(() => {});
  }, []);

  useEffect(() => { refresh(); }, [location.pathname, refresh]);

  useEffect(() => {
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, [refresh]);

  return (
    <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-sidebar px-6" style={{ boxShadow: "var(--shadow-sm)" }}>
      <h1 className="text-sm font-semibold text-text-primary">{t(titleKey)}</h1>

      <div className="flex items-center gap-2">
        {status && (
          <div className={`flex items-center gap-2 rounded-full border px-3 py-1 text-xs ${
            status.running
              ? "border-success/20 bg-success-soft text-success"
              : "border-border bg-card-secondary text-text-muted"
          }`}>
            <span className={`h-1.5 w-1.5 rounded-full ${
              status.running ? "bg-success animate-pulse-dot" : "bg-text-muted"
            }`} />
            <span className="font-medium">
              {status.running ? t("topbar.running") : t("topbar.stopped")}
            </span>
            {status.running && (
              <span className="font-mono text-text-secondary">
                {status.host}:{status.port}
              </span>
            )}
          </div>
        )}
        {/* 诊断快捷入口：始终可见、stopped 时高亮——故障排查路径从"先想到
            侧边栏第 7 项"缩短到"一眼看到"。点击跳 /diagnostics。 */}
        <button
          type="button"
          onClick={() => navigate("/diagnostics")}
          title={t("nav.diagnostics")}
          aria-label={t("nav.diagnostics")}
          className={`inline-flex h-7 items-center justify-center rounded-full border px-2 transition-colors ${
            status && !status.running
              ? "border-warning/30 bg-warning-soft text-warning hover:bg-warning/20"
              : "border-border bg-card-secondary text-text-muted hover:text-text-primary hover:bg-hover"
          }`}
        >
          <Stethoscope className="h-3.5 w-3.5" />
        </button>
      </div>
    </header>
  );
}
