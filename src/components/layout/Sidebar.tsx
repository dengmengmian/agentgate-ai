import { useState, useEffect } from "react";
import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Wrench,
  Cloud,
  Radio,
  GitBranch,
  ScrollText,
  Stethoscope,
  Settings,
} from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { cn } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";

const navItems = [
  { to: "/", labelKey: "nav.dashboard", icon: LayoutDashboard },
  { to: "/tools", labelKey: "nav.tools", icon: Wrench },
  { to: "/providers", labelKey: "nav.providers", icon: Cloud },
  { to: "/routes", labelKey: "nav.routes", icon: GitBranch },
  { to: "/gateway", labelKey: "nav.gateway", icon: Radio },
  { to: "/logs", labelKey: "nav.logs", icon: ScrollText },
  { to: "/diagnostics", labelKey: "nav.diagnostics", icon: Stethoscope },
  { to: "/settings", labelKey: "nav.settings", icon: Settings },
];

export function Sidebar() {
  const { t } = useI18n();
  const [version, setVersion] = useState("");
  useEffect(() => { getVersion().then(setVersion).catch(() => {}); }, []);

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-border bg-card">
      <div className="flex h-14 items-center gap-2.5 px-5">
        <svg className="h-7 w-7" viewBox="0 0 512 512" fill="none" xmlns="http://www.w3.org/2000/svg">
          <circle cx="256" cy="256" r="180" stroke="currentColor" strokeWidth="16" opacity="0.2" className="text-accent" />
          <ellipse cx="256" cy="256" rx="180" ry="100" stroke="currentColor" strokeWidth="16" opacity="0.35" className="text-accent" transform="rotate(-25 256 256)" />
          <ellipse cx="256" cy="256" rx="180" ry="100" stroke="currentColor" strokeWidth="16" opacity="0.35" className="text-accent" transform="rotate(25 256 256)" />
          <circle cx="256" cy="256" r="56" stroke="currentColor" strokeWidth="16" fill="none" className="text-accent" />
          <circle cx="256" cy="256" r="30" fill="currentColor" className="text-cyan-400" />
        </svg>
        <span className="text-sm font-semibold tracking-tight text-text-primary">
          AgentGate
        </span>
      </div>

      <nav className="flex flex-1 flex-col gap-0.5 px-3 pt-2">
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === "/"}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-3 rounded-md px-3 py-2 text-[13px] font-medium transition-colors",
                isActive
                  ? "bg-accent/10 text-accent"
                  : "text-text-secondary hover:bg-card-secondary hover:text-text-primary"
              )
            }
          >
            <item.icon className="h-4 w-4" />
            {t(item.labelKey)}
          </NavLink>
        ))}
      </nav>

      <div className="border-t border-border px-5 py-3">
        <p className="text-[11px] text-text-muted">{version ? `v${version}` : ""}</p>
      </div>
    </aside>
  );
}
