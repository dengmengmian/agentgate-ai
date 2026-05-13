import { useState, useEffect, useCallback } from "react";
import { useLocation } from "react-router-dom";
import { Circle } from "lucide-react";
import * as api from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import type { GatewayStatus } from "@/types/gateway";

export function Topbar() {
  const { t } = useI18n();
  const location = useLocation();

  const pageTitleKeys: Record<string, string> = {
    "/": "nav.dashboard",
    "/tools": "nav.tools",
    "/providers": "nav.providers",
    "/routes": "nav.routes",
    "/gateway": "nav.gateway",
    "/logs": "nav.logs",
    "/diagnostics": "nav.diagnostics",
    "/settings": "nav.settings",
  };

  const titleKey = pageTitleKeys[location.pathname] ?? "nav.dashboard";
  const [status, setStatus] = useState<GatewayStatus | null>(null);

  const refresh = useCallback(() => {
    api.getGatewayStatus().then(setStatus).catch(() => {});
  }, []);

  // Refresh on route change
  useEffect(() => { refresh(); }, [location.pathname, refresh]);

  // Poll every 3 seconds to catch gateway state changes
  useEffect(() => {
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, [refresh]);

  return (
    <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-card px-6">
      <h1 className="text-sm font-semibold text-text-primary">{t(titleKey)}</h1>

      <div className="flex items-center gap-4">
        {status && (
          <div className="flex items-center gap-2 text-xs">
            <Circle
              className={`h-2 w-2 fill-current ${
                status.running ? "text-success" : "text-text-muted"
              }`}
            />
            <span className="text-text-secondary">
              {t("topbar.gateway")}{" "}
              <span className="capitalize text-text-primary">
                {status.running ? t("topbar.running") : t("topbar.stopped")}
              </span>
            </span>
            {status.running && (
              <span className="font-mono text-text-muted">
                {status.host}:{status.port}
              </span>
            )}
          </div>
        )}
      </div>
    </header>
  );
}
