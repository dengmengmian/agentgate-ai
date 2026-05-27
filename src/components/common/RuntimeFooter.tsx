// Sticky bottom KPI footer surfaced on Dashboard + Routes.
//
// Polls `get_runtime_kpis` every 5 s — counters all live in gateway-side
// memory + one cheap SQLite COUNT for today's totals. Cheap enough to keep
// the footer alive without a "Refresh" button.

import { useEffect, useState } from "react";
import { Activity, TrendingUp, Clock, Cpu } from "lucide-react";
import * as api from "@/lib/api";
import type { RuntimeKpis } from "@/types/stats";

function formatUptime(secs: number): string {
  if (secs <= 0) return "—";
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

interface MetricProps {
  icon: React.ReactNode;
  label: string;
  value: string;
  tone?: "default" | "accent" | "muted";
}

function Metric({ icon, label, value, tone = "default" }: MetricProps) {
  const tones: Record<string, string> = {
    default: "text-text-primary",
    accent: "text-accent",
    muted: "text-text-muted",
  };
  return (
    <div className="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-2.5" style={{ boxShadow: "var(--shadow-sm)" }}>
      <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-accent-soft text-accent">
        {icon}
      </div>
      <div className="flex min-w-0 flex-col">
        <span className="text-[10px] uppercase tracking-wide text-text-muted">{label}</span>
        <span className={`font-mono text-base font-semibold tabular-nums ${tones[tone]}`}>{value}</span>
      </div>
    </div>
  );
}

export function RuntimeFooter() {
  const [kpis, setKpis] = useState<RuntimeKpis | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const k = await api.getRuntimeKpis();
        if (!cancelled) setKpis(k);
      } catch {
        // Silent — footer is a passive observer, don't toast errors.
      }
    };
    load();
    const timer = setInterval(load, 5000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  if (!kpis) return null;

  // success_rate_today is rendered as a percent string; show "—" before any
  // requests have come in (denominator 0).
  const rateStr = kpis.total_today > 0 ? `${kpis.success_rate_today.toFixed(1)}%` : "—";

  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
      <Metric icon={<Activity className="h-3.5 w-3.5" />} label="活跃连接" value={String(kpis.active_requests)} />
      <Metric icon={<TrendingUp className="h-3.5 w-3.5" />} label="今日请求" value={kpis.total_today.toLocaleString()} />
      <Metric
        icon={<Cpu className="h-3.5 w-3.5" />}
        label="今日成功率"
        value={rateStr}
        tone={kpis.total_today > 0 && kpis.success_rate_today < 90 ? "default" : "accent"}
      />
      <Metric
        icon={<Clock className="h-3.5 w-3.5" />}
        label="运行时间"
        value={kpis.gateway_running ? formatUptime(kpis.uptime_seconds) : "已停止"}
        tone={kpis.gateway_running ? "default" : "muted"}
      />
    </div>
  );
}
