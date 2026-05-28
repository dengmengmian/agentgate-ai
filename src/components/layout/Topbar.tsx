import { useState, useEffect, useCallback } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Stethoscope, Search } from "lucide-react";
import * as api from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import type { GatewayStatus } from "@/types/gateway";

const IS_MAC = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);

export function Topbar({ onOpenCmdK }: { onOpenCmdK?: () => void }) {
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
        {/* Cmd+K 触发器——让用户发现这个快捷键的存在。点击也能打开。 */}
        {onOpenCmdK && (
          <button
            type="button"
            onClick={onOpenCmdK}
            title={t("cmdk.placeholder")}
            className="inline-flex h-7 items-center gap-1.5 rounded-md border border-border bg-card-secondary px-2 text-[11px] text-text-muted transition-colors hover:text-text-primary hover:bg-hover"
          >
            <Search className="h-3.5 w-3.5" />
            <kbd className="font-sans">{IS_MAC ? "⌘" : "Ctrl"}+K</kbd>
          </button>
        )}
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
        {/* 诊断快捷入口：只在 stopped 时显眼出现——running 健康状态下完全
            隐藏，避免视觉噪音。点击跳 /diagnostics。 */}
        {status && !status.running && (
          <button
            type="button"
            onClick={() => navigate("/diagnostics")}
            title={t("nav.diagnostics")}
            aria-label={t("nav.diagnostics")}
            className="inline-flex h-7 items-center justify-center rounded-full border border-warning/30 bg-warning-soft px-2 text-warning transition-colors hover:bg-warning/20"
          >
            <Stethoscope className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
    </header>
  );
}
