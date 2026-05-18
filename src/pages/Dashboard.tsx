import { useState, useEffect } from "react";
import {
  Activity,
  Zap,
  Clock,
  Radio,
  Play,
  Square,
  RotateCcw,
  CheckCircle,
  BarChart3,
  ArrowDownToLine,
  ArrowUpFromLine,
  DollarSign,
} from "lucide-react";
import { MetricCard } from "@/components/dashboard/MetricCard";
import { RecentRequests } from "@/components/dashboard/RecentRequests";
import { StatusBadge } from "@/components/common/StatusBadge";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import { formatLatency } from "@/lib/utils";
import * as api from "@/lib/api";
import type { GatewayStatus } from "@/types/gateway";
import type { ToolConfigView } from "@/types/tool";
import type { RequestLogListItem } from "@/types/request-log";
import type { RequestStats } from "@/types/stats";

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatCost(n: number): string {
  if (n < 0.01) return `$${n.toFixed(4)}`;
  if (n < 1) return `$${n.toFixed(3)}`;
  return `$${n.toFixed(2)}`;
}

export function Dashboard() {
  const { t } = useI18n();
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [tools, setTools] = useState<ToolConfigView[]>([]);
  const [recentLogs, setRecentLogs] = useState<RequestLogListItem[]>([]);
  const [stats, setStats] = useState<RequestStats | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const [s, tl, l, st] = await Promise.all([
          api.getGatewayStatus(),
          api.listTools(),
          api.listRequestLogs({ limit: 5 }),
          api.getRequestStats(),
        ]);
        if (!cancelled) {
          setStatus(s);
          setTools(tl);
          setRecentLogs(l);
          setStats(st);
        }
      } catch (err) {
        if (!cancelled) toast("error", (err as api.AppError).message);
      }
    };
    load();
    const timer = setInterval(load, 5000);
    return () => { cancelled = true; clearInterval(timer); };
  }, []);

  const handleStart = async () => { try { setStatus(await api.startGateway()); toast("success", t("gateway.started")); } catch (err) { toast("error", (err as api.AppError).message); } };
  const handleStop = async () => { try { setStatus(await api.stopGateway()); toast("success", t("gateway.stopped")); } catch (err) { toast("error", (err as api.AppError).message); } };
  const handleRestart = async () => { try { setStatus(await api.restartGateway()); toast("success", t("gateway.restarted")); } catch (err) { toast("error", (err as api.AppError).message); } };

  if (!status) return <p className="text-xs text-text-muted">{t("common.loading")}</p>;

  return (
    <div className="space-y-6">
      {/* Gateway Status */}
      <div className="rounded-lg border border-border bg-card p-5">
        <div className="mb-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-accent/10">
              <Radio className="h-4.5 w-4.5 text-accent" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-text-primary">{t("dashboard.gateway")}</h3>
              <p className="text-xs text-text-muted">{status.host}:{status.port}</p>
            </div>
          </div>
          <StatusBadge variant={status.running ? "success" : "muted"}>
            {status.running ? t("topbar.running") : t("topbar.stopped")}
          </StatusBadge>
        </div>
        <div className="mb-4 grid grid-cols-3 gap-3">
          <div><p className="text-[11px] text-text-muted">{t("dashboard.provider")}</p><p className="text-sm font-medium text-text-primary">{status.active_provider ?? t("common.none")}</p></div>
          <div><p className="text-[11px] text-text-muted">{t("dashboard.input")}</p><p className="text-sm font-medium text-text-primary">{status.input_protocol}</p></div>
          <div><p className="text-[11px] text-text-muted">{t("dashboard.output")}</p><p className="text-sm font-medium text-text-primary">{status.output_protocol}</p></div>
        </div>
        <div className="flex gap-2">
          {status.running ? (
            <>
              <button onClick={handleStop} className="flex items-center gap-1.5 rounded-md bg-error/10 px-3 py-1.5 text-xs font-medium text-error transition-colors hover:bg-error/20"><Square className="h-3 w-3" />{t("dashboard.stop")}</button>
              <button onClick={handleRestart} className="flex items-center gap-1.5 rounded-md bg-warning/10 px-3 py-1.5 text-xs font-medium text-warning transition-colors hover:bg-warning/20"><RotateCcw className="h-3 w-3" />{t("dashboard.restart")}</button>
            </>
          ) : (
            <button onClick={handleStart} className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-accent/90"><Play className="h-3 w-3" />{t("dashboard.start")}</button>
          )}
        </div>
      </div>

      {/* Overview Metrics */}
      {stats && (
        <div className="grid grid-cols-3 gap-4">
          <MetricCard label={t("stats.total_requests")} value={stats.total} icon={Activity} trend={`${t("stats.today")}: ${stats.today_total}`} />
          <MetricCard label={t("stats.success_rate")} value={`${stats.success_rate}%`} icon={CheckCircle} trend={`${stats.today_errors} ${t("stats.errors_label")}`} />
          <MetricCard label={t("stats.avg_latency")} value={formatLatency(stats.avg_latency_ms)} icon={Clock} />
        </div>
      )}

      {/* Token Stats */}
      {stats && (
        <div className="grid grid-cols-3 gap-4">
          <MetricCard label={t("stats.input_tokens")} value={formatTokens(stats.total_input_tokens)} icon={ArrowDownToLine} trend={`${t("stats.today")}: ${formatTokens(stats.today_input_tokens)}`} />
          <MetricCard label={t("stats.output_tokens")} value={formatTokens(stats.total_output_tokens)} icon={ArrowUpFromLine} trend={`${t("stats.today")}: ${formatTokens(stats.today_output_tokens)}`} />
          <MetricCard label={t("stats.total_tokens")} value={formatTokens(stats.total_input_tokens + stats.total_output_tokens)} icon={Zap} />
        </div>
      )}

      {/* Cost Stats */}
      {stats && (stats.total_cost > 0 || stats.today_cost > 0) && (
        <div className="grid grid-cols-3 gap-4">
          <MetricCard label={t("stats.total_cost")} value={formatCost(stats.total_cost)} icon={DollarSign} trend={`${t("stats.today")}: ${formatCost(stats.today_cost)}`} />
          <MetricCard label={t("stats.today_cost")} value={formatCost(stats.today_cost)} icon={DollarSign} />
          <MetricCard label={t("stats.avg_cost")} value={stats.total > 0 ? formatCost(stats.total_cost / stats.total) : "$0"} icon={DollarSign} trend={t("stats.per_request")} />
        </div>
      )}

      {/* Daily Chart + Top Providers */}
      {stats && (
        <div className="grid grid-cols-3 gap-4">
          {/* Daily Bar Chart */}
          <div className="col-span-2 rounded-lg border border-border bg-card p-5">
            <h3 className="mb-3 flex items-center gap-2 text-sm font-semibold text-text-primary">
              <BarChart3 className="h-4 w-4 text-text-muted" />{t("stats.daily_chart")}
            </h3>
            {/* Legend */}
            <div className="mb-3 flex items-center gap-4 text-[10px] text-text-muted">
              <div className="flex items-center gap-1">
                <div className="h-2.5 w-2.5 rounded-sm bg-accent/70" />
                <span>{t("stats.requests")}</span>
              </div>
              <div className="flex items-center gap-1">
                <div className="h-2.5 w-2.5 rounded-sm bg-cyan-500/60" />
                <span>{t("stats.input_tokens")}</span>
              </div>
              <div className="flex items-center gap-1">
                <div className="h-2.5 w-2.5 rounded-sm bg-success/60" />
                <span>{t("stats.output_tokens")}</span>
              </div>
            </div>
            <div className="flex items-end gap-2" style={{ height: 140 }}>
              {stats.daily.map((d) => {
                const maxReq = Math.max(...stats.daily.map(x => x.total), 1);
                const maxTok = Math.max(...stats.daily.map(x => x.input_tokens + x.output_tokens), 1);
                const tokTotal = d.input_tokens + d.output_tokens;

                // Request bar (0-80px)
                const reqH = d.total > 0 ? Math.max((d.total / maxReq) * 80, 3) : 0;
                const reqErrH = d.errors > 0 && reqH > 0
                  ? Math.max((d.errors / d.total) * reqH, 2)
                  : 0;

                // Token bar (0-80px)
                const tokH = tokTotal > 0 ? Math.max((tokTotal / maxTok) * 80, 3) : 0;
                const tokOutH = d.output_tokens > 0 && tokH > 0
                  ? Math.max((d.output_tokens / tokTotal) * tokH, 2)
                  : 0;

                return (
                  <div key={d.date} className="flex flex-1 flex-col items-center gap-1">
                    <div className="flex w-full items-end justify-center gap-1" style={{ height: 90 }}>
                      {/* Request bar */}
                      <div className="flex w-2.5 flex-col items-center justify-end rounded-md overflow-hidden">
                        {reqH > 0 ? (
                          <>
                            {reqErrH > 0 && <div className="w-full bg-error/60" style={{ height: reqErrH }} />}
                            <div className="w-full bg-accent/70" style={{ height: reqH - reqErrH }} />
                          </>
                        ) : <div className="w-full bg-border/30" style={{ height: 1 }} />}
                      </div>
                      {/* Token bar */}
                      <div className="flex w-2.5 flex-col items-center justify-end rounded-md overflow-hidden">
                        {tokH > 0 ? (
                          <>
                            {tokOutH > 0 && <div className="w-full bg-success/60" style={{ height: tokOutH }} />}
                            <div className="w-full bg-cyan-500/60" style={{ height: tokH - tokOutH }} />
                          </>
                        ) : <div className="w-full bg-border/30" style={{ height: 1 }} />}
                      </div>
                    </div>
                    <span className="text-[9px] text-text-muted">{d.date.slice(5)}</span>
                    <div className="flex flex-col items-center gap-0.5">
                      <span className="text-[10px] font-mono font-medium text-text-secondary">{d.total}</span>
                      {tokTotal > 0 && (
                        <span className="text-[9px] font-mono text-success">{formatTokens(tokTotal)}</span>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Top Providers */}
          <div className="rounded-lg border border-border bg-card p-5">
            <h3 className="mb-3 text-sm font-semibold text-text-primary">{t("stats.top_providers")}</h3>
            <div className="space-y-2">
              {stats.providers.map((p) => (
                <div key={p.name} className="flex items-center justify-between text-xs">
                  <span className="text-text-primary">{p.name}</span>
                  <span className="font-mono text-text-muted">{p.count}</span>
                </div>
              ))}
              {stats.providers.length === 0 && <p className="text-xs text-text-muted">—</p>}
            </div>
          </div>
        </div>
      )}

      {/* Tools Overview */}
      <div className="rounded-lg border border-border bg-card p-5">
        <h3 className="mb-3 text-sm font-semibold text-text-primary">{t("dashboard.tool_status")}</h3>
        <div className="grid grid-cols-3 gap-4">
          {tools.map((tool) => (
            <div key={tool.id} className="flex items-center justify-between rounded-md border border-border/50 bg-card-secondary px-4 py-3">
              <div>
                <p className="text-sm font-medium text-text-primary">{tool.name}</p>
                <p className="font-mono text-[11px] text-text-muted">{tool.config_path.split("/").pop()}</p>
              </div>
              <StatusBadge variant={tool.config_exists ? "success" : "muted"}>
                {tool.config_exists ? t("dashboard.found") : t("dashboard.not_found")}
              </StatusBadge>
            </div>
          ))}
        </div>
      </div>

      {/* Recent Requests */}
      <RecentRequests requests={recentLogs} />
    </div>
  );
}
