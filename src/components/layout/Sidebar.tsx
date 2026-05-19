import { useState, useEffect } from "react";
import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Cloud,
  GitBranch,
  Radio,
  Monitor,
  ScrollText,
  Stethoscope,
  Settings,
  Rocket,
} from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { cn } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";

const navItems = [
  { to: "/", labelKey: "nav.overview", icon: LayoutDashboard },
  { to: "/providers", labelKey: "nav.providers", icon: Cloud },
  { to: "/routes", labelKey: "nav.routes", icon: GitBranch },
  { to: "/gateway", labelKey: "nav.gateway", icon: Radio },
  { to: "/tools", labelKey: "nav.clients", icon: Monitor },
  { to: "/logs", labelKey: "nav.logs", icon: ScrollText },
  { to: "/diagnostics", labelKey: "nav.diagnostics", icon: Stethoscope },
  { to: "/settings", labelKey: "nav.settings", icon: Settings },
];

export function Sidebar() {
  const { t } = useI18n();
  const [version, setVersion] = useState("");
  const [showQuickSetup, setShowQuickSetup] = useState(false);
  useEffect(() => { getVersion().then(setVersion).catch(() => {}); }, []);

  // Show quick setup only if: no providers AND not manually hidden
  useEffect(() => {
    if (localStorage.getItem("agentgate_hide_quick_setup") === "1") return;
    if (localStorage.getItem("agentgate_show_quick_setup") === "1") { setShowQuickSetup(true); return; }
    import("@/lib/api").then(api => {
      api.listProviders().then(providers => {
        setShowQuickSetup(providers.length === 0);
      }).catch(() => {});
    });
  }, []);

  return (
    <aside className="flex w-52 shrink-0 flex-col border-r border-border bg-sidebar">
      {/* Logo */}
      <div className="flex h-14 items-center gap-2.5 border-b border-border px-5">
        <svg className="h-10 w-10" viewBox="0 0 512 512" fill="none" xmlns="http://www.w3.org/2000/svg">
          <circle cx="256" cy="256" r="180" stroke="currentColor" strokeWidth="16" opacity="0.2" className="text-accent" />
          <ellipse cx="256" cy="256" rx="180" ry="100" stroke="currentColor" strokeWidth="16" opacity="0.35" className="text-accent" transform="rotate(-25 256 256)" />
          <ellipse cx="256" cy="256" rx="180" ry="100" stroke="currentColor" strokeWidth="16" opacity="0.35" className="text-accent" transform="rotate(25 256 256)" />
          <circle cx="256" cy="256" r="56" stroke="currentColor" strokeWidth="16" fill="none" className="text-accent" />
          <circle cx="256" cy="256" r="30" fill="currentColor" className="text-accent" />
        </svg>
        <span className="text-sm font-semibold tracking-tight text-text-primary">
          AgentGate
        </span>
      </div>

      {/* Navigation */}
      <nav className="flex flex-1 flex-col gap-0.5 px-3 pt-3">
        {showQuickSetup && (
          <div className="relative mb-1">
            <NavLink
              to="/quick-setup"
              className={({ isActive }) =>
                cn(
                  "group relative flex items-center gap-3 rounded-lg px-3 py-2 text-[13px] font-medium transition-all duration-150",
                  isActive
                    ? "bg-accent-soft text-accent"
                    : "text-accent hover:bg-accent-soft"
                )
              }
            >
              {({ isActive }) => (
                <>
                  {isActive && (
                    <span className="absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-r-full bg-accent" />
                  )}
                  <Rocket className="h-4 w-4 shrink-0" />
                  {t("nav.quick_setup")}
                </>
              )}
            </NavLink>
            <button
              onClick={(e) => {
                e.stopPropagation();
                localStorage.setItem("agentgate_hide_quick_setup", "1");
                localStorage.removeItem("agentgate_show_quick_setup");
                setShowQuickSetup(false);
              }}
              className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-0.5 text-text-muted hover:text-text-primary hover:bg-hover"
              title={t("common.close")}
            >
              <span className="text-xs">✕</span>
            </button>
          </div>
        )}
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === "/"}
            className={({ isActive }) =>
              cn(
                "group relative flex items-center gap-3 rounded-lg px-3 py-2 text-[13px] font-medium transition-all duration-150",
                isActive
                  ? "bg-accent-soft text-accent"
                  : "text-text-secondary hover:bg-hover hover:text-text-primary"
              )
            }
          >
            {({ isActive }) => (
              <>
                {isActive && (
                  <span className="absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-r-full bg-accent" />
                )}
                <item.icon className="h-4 w-4 shrink-0" />
                {t(item.labelKey)}
              </>
            )}
          </NavLink>
        ))}
      </nav>

      {/* Footer */}
      <div className="border-t border-border px-5 py-3">
        <p className="text-[11px] text-text-muted">{version ? `v${version}` : ""}</p>
      </div>
    </aside>
  );
}
