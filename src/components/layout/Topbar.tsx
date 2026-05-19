import { useState, useEffect, useCallback } from "react";
import { useLocation } from "react-router-dom";
import * as api from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import type { GatewayStatus } from "@/types/gateway";

export function Topbar() {
  const { t } = useI18n();
  const location = useLocation();

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

      <div className="flex items-center gap-4">
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
      </div>
    </header>
  );
}
