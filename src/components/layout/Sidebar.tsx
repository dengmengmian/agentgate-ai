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
  PanelLeftClose,
  PanelLeftOpen,
  FileText,
  Plug,
} from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { cn } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";

// 顺序按新用户配置任务流：
// 看运行情况（概览） → 配上游（供应商） → 接客户端（客户端） →
// 启停服务（服务） → 高级失败转移（路由策略） → 用了之后看日志和诊断 → 设置
const navItems = [
  { to: "/", labelKey: "nav.overview", icon: LayoutDashboard },
  { to: "/providers", labelKey: "nav.providers", icon: Cloud },
  { to: "/tools", labelKey: "nav.clients", icon: Monitor },
  { to: "/gateway", labelKey: "nav.gateway", icon: Radio },
  { to: "/routes", labelKey: "nav.routes", icon: GitBranch },
  { to: "/logs", labelKey: "nav.logs", icon: ScrollText },
  { to: "/diagnostics", labelKey: "nav.diagnostics", icon: Stethoscope },
  { to: "/instructions", labelKey: "nav.instructions", icon: FileText },
  { to: "/mcp", labelKey: "nav.mcp", icon: Plug },
  { to: "/settings", labelKey: "nav.settings", icon: Settings },
];

export function Sidebar() {
  const { t } = useI18n();
  const [version, setVersion] = useState("");
  const [showQuickSetup, setShowQuickSetup] = useState(false);
  // 折叠状态：用 localStorage 持久化——用户折叠后下次打开仍是折叠的。
  const [collapsed, setCollapsed] = useState<boolean>(() => localStorage.getItem("agentgate_sidebar_collapsed") === "1");
  const toggleCollapse = () => {
    const next = !collapsed;
    setCollapsed(next);
    localStorage.setItem("agentgate_sidebar_collapsed", next ? "1" : "0");
  };
  useEffect(() => { getVersion().then(setVersion).catch(() => {}); }, []);

  // Show quick setup only if: no providers AND not manually hidden
  useEffect(() => {
    if (localStorage.getItem("agentgate_hide_quick_setup") === "1") return;
    if (localStorage.getItem("agentgate_show_quick_setup") === "1") { setShowQuickSetup(true); return; }
    api.listProviders().then(providers => {
      setShowQuickSetup(providers.length === 0);
    }).catch(() => {});
  }, []);

  return (
    <aside className={cn(
      "flex shrink-0 flex-col border-r border-border bg-sidebar transition-[width] duration-150",
      collapsed ? "w-14" : "w-52"
    )}>
      {/* Logo + collapse toggle */}
      <div className={cn(
        "flex h-14 items-center border-b border-border",
        collapsed ? "justify-center px-2" : "justify-between px-5"
      )}>
        <div className="flex items-center gap-2.5 min-w-0">
          <svg className="h-10 w-10 shrink-0" viewBox="0 0 512 512" fill="none" xmlns="http://www.w3.org/2000/svg">
            <circle cx="256" cy="256" r="180" stroke="currentColor" strokeWidth="16" opacity="0.2" className="text-accent" />
            <ellipse cx="256" cy="256" rx="180" ry="100" stroke="currentColor" strokeWidth="16" opacity="0.35" className="text-accent" transform="rotate(-25 256 256)" />
            <ellipse cx="256" cy="256" rx="180" ry="100" stroke="currentColor" strokeWidth="16" opacity="0.35" className="text-accent" transform="rotate(25 256 256)" />
            <circle cx="256" cy="256" r="56" stroke="currentColor" strokeWidth="16" fill="none" className="text-accent" />
            <circle cx="256" cy="256" r="30" fill="currentColor" className="text-accent" />
          </svg>
          {!collapsed && (
            <span className="text-sm font-semibold tracking-tight text-text-primary truncate">
              AgentGate
            </span>
          )}
        </div>
        {!collapsed && (
          <button
            type="button"
            onClick={toggleCollapse}
            title={t("nav.collapse")}
            className="shrink-0 rounded p-1 text-text-muted hover:bg-hover hover:text-text-primary"
          >
            <PanelLeftClose className="h-4 w-4" />
          </button>
        )}
      </div>

      {/* 折叠态：单独一个展开按钮放在 logo 下面，避免点 logo 误折叠 */}
      {collapsed && (
        <button
          type="button"
          onClick={toggleCollapse}
          title={t("nav.expand")}
          className="mt-2 mx-auto rounded p-1.5 text-text-muted hover:bg-hover hover:text-text-primary"
        >
          <PanelLeftOpen className="h-4 w-4" />
        </button>
      )}

      {/* Navigation */}
      <nav className={cn("flex flex-1 flex-col gap-0.5 pt-3", collapsed ? "px-2" : "px-3")}>
        {showQuickSetup && (
          <NavLink
            to="/quick-setup"
            title={collapsed ? t("nav.quick_setup") : undefined}
            className={({ isActive }) =>
              cn(
                "group relative mb-1 flex items-center rounded-lg text-[13px] font-medium transition-all duration-150",
                collapsed ? "justify-center px-2 py-2" : "gap-3 px-3 py-2",
                isActive
                  ? "bg-accent-soft text-accent"
                  : "text-accent hover:bg-accent-soft"
              )
            }
          >
            {({ isActive }) => (
              <>
                {isActive && !collapsed && (
                  <span className="absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-r-full bg-accent" />
                )}
                <Rocket className="h-4 w-4 shrink-0" />
                {!collapsed && t("nav.quick_setup")}
              </>
            )}
          </NavLink>
        )}
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === "/"}
            title={collapsed ? t(item.labelKey) : undefined}
            className={({ isActive }) =>
              cn(
                "group relative flex items-center rounded-lg text-[13px] font-medium transition-all duration-150",
                collapsed ? "justify-center px-2 py-2" : "gap-3 px-3 py-2",
                isActive
                  ? "bg-accent-soft text-accent"
                  : "text-text-secondary hover:bg-hover hover:text-text-primary"
              )
            }
          >
            {({ isActive }) => (
              <>
                {isActive && !collapsed && (
                  <span className="absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-r-full bg-accent" />
                )}
                <item.icon className="h-4 w-4 shrink-0" />
                {!collapsed && t(item.labelKey)}
              </>
            )}
          </NavLink>
        ))}
      </nav>

      {/* Footer */}
      {!collapsed && (
        <div className="border-t border-border px-5 py-3">
          <p className="text-[11px] text-text-muted">{version ? `v${version}` : ""}</p>
        </div>
      )}
    </aside>
  );
}
