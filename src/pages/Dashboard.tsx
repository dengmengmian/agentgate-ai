import { useState, useEffect } from "react";
import {
  Radio,
  Play,
  Square,
  RotateCcw,
  BarChart3,
  Rocket,
  ArrowRight,
  Coins,
} from "lucide-react";
import { Link } from "react-router-dom";
import { RecentRequests } from "@/components/dashboard/RecentRequests";
import { RuntimeFooter } from "@/components/common/RuntimeFooter";
import { StatusBadge } from "@/components/common/StatusBadge";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import { formatLatency } from "@/lib/utils";
import * as api from "@/lib/api";
import type { GatewayStatus } from "@/types/gateway";
import type { ToolConfigView } from "@/types/tool";
import type { RequestLogListItem, CostBreakdown } from "@/types/request-log";
import type { RequestStats } from "@/types/stats";

/// 极简 deep equal：JSON 字符串化对比。dashboard 数据 payload 不大
/// （几个 KB），常数时间。避免 5 秒轮询每次都触发 React 重渲让数字
/// 闪烁、按钮 hover 状态丢失。
function shallowEqual<T>(a: T, b: T): boolean {
  if (a === b) return true;
  if (a === null || b === null) return false;
  try {
    return JSON.stringify(a) === JSON.stringify(b);
  } catch {
    return false;
  }
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatCost(n: number): string {
  if (n === 0) return "$0";
  if (n < 0.01) return `$${n.toFixed(4)}`;
  if (n < 1) return `$${n.toFixed(3)}`;
  return `$${n.toFixed(2)}`;
}

// 成本分解小列表：每行 名称 + 占比条 + 请求数 + 成本，按成本倒序（后端已排）。
function CostList({ title, rows }: { title: string; rows: CostBreakdown[] }) {
  const max = rows.reduce((m, r) => Math.max(m, r.cost), 0) || 1;
  return (
    <div>
      <div className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-text-secondary">{title}</div>
      {rows.length === 0 ? (
        <p className="text-[11px] text-text-muted">—</p>
      ) : (
        <div className="space-y-1.5">
          {rows.map((r) => (
            <div key={r.key} className="flex items-center gap-2 text-[11px]">
              <span className="w-28 shrink-0 truncate font-mono text-text-primary" title={r.key}>{r.key}</span>
              <div className="relative h-1.5 flex-1 overflow-hidden rounded-full bg-card-secondary">
                <div className="absolute inset-y-0 left-0 rounded-full bg-accent/60" style={{ width: `${(r.cost / max) * 100}%` }} />
              </div>
              <span className="shrink-0 tabular-nums text-text-muted">{r.request_count}</span>
              {r.has_price ? (
                <span className="w-16 shrink-0 text-right font-mono tabular-nums text-text-primary">{formatCost(r.cost)}</span>
              ) : (
                <span className="w-16 shrink-0 text-right text-[10px] text-text-muted/60" title="价格表里没有这个模型的价，成本算不出（不是免费）">无价格</span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// Single metric in a horizontal strip: label above, value below, no card chrome.
function StripMetric({ label, value, tone }: { label: string; value: string; tone?: "default" | "error" | "accent" }) {
  const valueColor = tone === "error" ? "text-error" : tone === "accent" ? "text-accent" : "text-text-primary";
  return (
    <div className="flex flex-col">
      <span className="text-[10px] uppercase tracking-wide text-text-muted">{label}</span>
      <span className={`text-base font-semibold ${valueColor} tabular-nums`}>{value}</span>
    </div>
  );
}

/// 命中率 = cache_read / (cache_read + cache_write + 非缓存输入)。
/// 三项分母对 Anthropic 三段式（read / write / non-cached input 各自独立）天然正确；
/// OpenAI 把 cached_tokens 计入 input_tokens 会让分母略偏大，但永远落在 [0, 100]，
/// 不会出现 >100% 这种用户看了懵的情况。颜色阈值参照"系统提示稳定时的健康面"：
/// ≥70% 绿，30-70% 黄（可能在切换 prompt / 上游 TTL 到期），<30% 红。
function CacheHitBadge({
  cacheRead,
  cacheWrite,
  inputTokens,
}: {
  cacheRead: number;
  cacheWrite: number;
  inputTokens: number;
}) {
  const denom = cacheRead + cacheWrite + inputTokens;
  if (denom <= 0) {
    return <StatusBadge variant="muted">—</StatusBadge>;
  }
  const rate = (cacheRead / denom) * 100;
  const variant = rate >= 70 ? "success" : rate >= 30 ? "warning" : "error";
  return <StatusBadge variant={variant}>{rate.toFixed(1)}%</StatusBadge>;
}

type RangeDays = 1 | 7 | 14 | 30;
const RANGE_OPTIONS: { days: RangeDays; labelZh: string; labelEn: string }[] = [
  { days: 1, labelZh: "今天", labelEn: "Today" },
  { days: 7, labelZh: "7天", labelEn: "7d" },
  { days: 14, labelZh: "14天", labelEn: "14d" },
  { days: 30, labelZh: "30天", labelEn: "30d" },
];

export function Dashboard() {
  const { t } = useI18n();
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [tools, setTools] = useState<ToolConfigView[]>([]);
  const [recentLogs, setRecentLogs] = useState<RequestLogListItem[]>([]);
  const [stats, setStats] = useState<RequestStats | null>(null);
  const [providerCount, setProviderCount] = useState<number | null>(null);
  const [costByModel, setCostByModel] = useState<CostBreakdown[]>([]);
  const [costByClient, setCostByClient] = useState<CostBreakdown[]>([]);
  const [costByStrategy, setCostByStrategy] = useState<CostBreakdown[]>([]);
  const [rangeDays, setRangeDays] = useState<RangeDays>(7);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const [s, tl, l, st, ps, cm, cc, rs, rp] = await Promise.all([
          api.getGatewayStatus(),
          api.listTools(),
          api.listRequestLogs({ limit: 5 }),
          api.getRequestStatsRange(rangeDays),
          api.listProviders(),
          api.aggregateCostByModel(rangeDays, 8),
          api.aggregateCostByClient(rangeDays, 8),
          api.aggregateRouteProfileStats(rangeDays).catch(() => []),
          api.listRouteProfiles().catch(() => []),
        ]);
        // 按策略成本：route_profile stats(含 cost/请求数) + profile 名字，转成
        // 和按模型/客户端一致的 CostBreakdown 形态复用 CostList。
        const nameMap = Object.fromEntries(rp.map((p) => [p.id, p.name]));
        const byStrategy: CostBreakdown[] = rs
          .map((x) => ({
            key: nameMap[x.route_profile_id] ?? x.route_profile_id,
            provider: null,
            request_count: x.request_count,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost: x.cost,
            has_price: true,
          }))
          .filter((x) => x.request_count > 0)
          .sort((a, b) => b.cost - a.cost);
        if (!cancelled) {
          // 首次请求 celebration：lifetime total 从 0 翻到 ≥1 时 toast 一次。
          // 用 localStorage 标记"已庆祝过"——避免清日志后再次触发。
          const lifetimeTotal = st.total;
          if (
            lifetimeTotal >= 1
            && localStorage.getItem("agentgate_first_req_seen") !== "1"
          ) {
            localStorage.setItem("agentgate_first_req_seen", "1");
            toast("success", t("dashboard.first_request_seen"));
          }

          // Incremental update：只在数据实际变化时 setState，避免每 5 秒整页
          // re-render 让数字闪烁、按钮跳动。浅比对 JSON 字符串虽然不最高效，
          // 但对这点 payload 来说是常数时间，且写法最直接。
          setStatus(prev => shallowEqual(prev, s) ? prev : s);
          setTools(prev => shallowEqual(prev, tl) ? prev : tl);
          setProviderCount(prev => prev === ps.length ? prev : ps.length);
          setRecentLogs(prev => shallowEqual(prev, l) ? prev : l);
          setStats(prev => shallowEqual(prev, st) ? prev : st);
          setCostByModel(prev => shallowEqual(prev, cm) ? prev : cm);
          setCostByClient(prev => shallowEqual(prev, cc) ? prev : cc);
          setCostByStrategy(prev => shallowEqual(prev, byStrategy) ? prev : byStrategy);
        }
      } catch (err) {
        if (!cancelled) toast("error", (err as api.AppError).message);
      }
    };
    load();
    const timer = setInterval(load, 5000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [rangeDays, t]);

  const handleStart = async () => { try { setStatus(await api.startGateway()); toast("success", t("gateway.started")); } catch (err) { toast("error", (err as api.AppError).message); } };
  const handleStop = async () => { try { setStatus(await api.stopGateway()); toast("success", t("gateway.stopped")); } catch (err) { toast("error", (err as api.AppError).message); } };
  const handleRestart = async () => { try { setStatus(await api.restartGateway()); toast("success", t("gateway.restarted")); } catch (err) { toast("error", (err as api.AppError).message); } };

  if (!status) return (
    <div className="space-y-4">
      <div className="skeleton h-12 w-full" />
      <div className="skeleton h-16 w-full" />
      <div className="skeleton h-40 w-full" />
    </div>
  );

  const todayTokens = stats ? stats.today_input_tokens + stats.today_output_tokens : 0;

  return (
    <div className="space-y-4">
      {/* ── 1. Gateway strip — active provider, protocol chain, controls.
              host:port + running badge live in the global Topbar; we don't
              repeat them here. ── */}
      <div className="rounded-xl border border-border bg-card px-5 py-3" style={{ boxShadow: "var(--shadow-sm)" }}>
        <div className="flex items-center justify-between gap-4">
          <div className="flex min-w-0 items-center gap-3">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft">
              <Radio className="h-4 w-4 text-accent" />
            </div>
            <div className="flex min-w-0 items-baseline gap-3">
              <span className="text-sm font-semibold text-text-primary">{t("dashboard.gateway")}</span>
              <span className="truncate text-xs text-text-secondary">
                {status.active_provider ?? t("common.none")}
              </span>
              <span className="hidden text-text-muted/40 md:inline">·</span>
              <span className="hidden truncate font-mono text-[11px] text-text-muted md:inline">
                {status.input_protocol} → {status.output_protocol}
              </span>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {status.running ? (
              <>
                <button onClick={handleStop} className="flex items-center gap-1 rounded-md bg-error-soft px-2.5 py-1 text-[11px] font-medium text-error transition-colors hover:bg-error/20">
                  <Square className="h-3 w-3" />{t("dashboard.stop")}
                </button>
                <button onClick={handleRestart} className="flex items-center gap-1 rounded-md bg-warning-soft px-2.5 py-1 text-[11px] font-medium text-warning transition-colors hover:bg-warning/20">
                  <RotateCcw className="h-3 w-3" />{t("dashboard.restart")}
                </button>
              </>
            ) : (
              <button onClick={handleStart} className="flex items-center gap-1 rounded-md bg-accent px-2.5 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent/90">
                <Play className="h-3 w-3" />{t("dashboard.start")}
              </button>
            )}
          </div>
        </div>
      </div>

      {/* ── 0/1 providers 时显示新手引导卡，替代一片空白的 stats ── */}
      {providerCount === 0 && (
        <Link
          to="/quick-setup"
          className="group block rounded-xl border-2 border-dashed border-accent/30 bg-accent-soft/30 p-6 transition-colors hover:border-accent/60 hover:bg-accent-soft/50"
        >
          <div className="flex items-start gap-4">
            <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-accent text-white">
              <Rocket className="h-6 w-6" />
            </div>
            <div className="min-w-0 flex-1">
              <h3 className="text-base font-semibold text-text-primary">
                {t("dashboard.empty_title")}
              </h3>
              <p className="mt-1 text-sm text-text-secondary">
                {t("dashboard.empty_desc")}
              </p>
              <div className="mt-3 inline-flex items-center gap-1.5 text-sm font-medium text-accent group-hover:gap-2 transition-all">
                {t("dashboard.empty_cta")}
                <ArrowRight className="h-4 w-4" />
              </div>
            </div>
          </div>
        </Link>
      )}

      {/* ── 2. Today card — 5 primary metrics + cache inline footer when present ── */}
      {providerCount !== 0 && stats && (
        <div className="rounded-xl border border-border bg-card px-6 py-4" style={{ boxShadow: "var(--shadow-sm)" }}>
          <div className="mb-3 flex items-center justify-between">
            <span className="text-xs font-semibold uppercase tracking-wide text-text-secondary">{t("stats.today")}</span>
            <span className="text-[11px] text-text-muted">{t("stats.realtime") || "实时刷新"}</span>
          </div>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-5">
            <StripMetric label={t("stats.requests")} value={String(stats.today_total)} />
            <StripMetric
              label={t("stats.errors_label")}
              value={String(stats.today_errors)}
              tone={stats.today_errors > 0 ? "error" : "default"}
            />
            <StripMetric label={t("stats.tokens_today") || "Tokens"} value={formatTokens(todayTokens)} />
            <StripMetric label={t("stats.cost_today") || "今日费用"} value={formatCost(stats.today_cost)} />
            <StripMetric label={t("stats.avg_latency")} value={formatLatency(stats.avg_latency_ms)} />
          </div>
          {(stats.today_cache_read_tokens > 0 || stats.today_cache_write_tokens > 0) && (
            <div className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-1 border-t border-border pt-3 text-[11px] text-text-muted">
              <span className="font-medium text-text-secondary">缓存</span>
              <span>
                写入 <span className="font-mono text-text-primary">{formatTokens(stats.today_cache_write_tokens)}</span>
              </span>
              <span className="text-text-muted/40">·</span>
              <span>
                命中 <span className="font-mono text-text-primary">{formatTokens(stats.today_cache_read_tokens)}</span>
              </span>
              <span className="text-text-muted/40">·</span>
              <span>
                输入合计 <span className="font-mono text-text-primary">{formatTokens(stats.today_input_tokens)}</span>
              </span>
              <span className="ml-auto flex items-center gap-1.5">
                命中率
                <CacheHitBadge
                  cacheRead={stats.today_cache_read_tokens}
                  cacheWrite={stats.today_cache_write_tokens}
                  inputTokens={stats.today_input_tokens}
                />
              </span>
            </div>
          )}
        </div>
      )}

      {/* ── 3. Trend chart + Top providers in one card. Range tabs live in the
              chart header (they only affect the chart, not today's strip). ── */}
      {stats && (
        <>
          <div className="rounded-xl border border-border bg-card p-5">
            <div className="mb-4 flex items-center justify-between gap-2">
              <h3 className="flex items-center gap-2 text-sm font-semibold text-text-primary">
                <BarChart3 className="h-4 w-4 text-text-muted" />
                {t("stats.daily_chart")}
                <span className="text-text-muted">· {rangeDays === 1 ? "今天" : `${rangeDays} 天`}</span>
              </h3>
              <div className="flex items-center gap-3">
                <div className="hidden items-center gap-3 text-[10px] text-text-muted sm:flex">
                  <div className="flex items-center gap-1"><div className="h-2 w-2 rounded-sm bg-accent/70" /><span>{t("stats.success_rate_label") || "成功"}</span></div>
                  <div className="flex items-center gap-1"><div className="h-2 w-2 rounded-sm bg-error/60" /><span>{t("stats.errors_label")}</span></div>
                </div>
                <div className="flex items-center gap-0.5 rounded-md bg-card-secondary p-0.5">
                  {RANGE_OPTIONS.map((opt) => (
                    <button
                      key={opt.days}
                      onClick={() => setRangeDays(opt.days)}
                      className={`rounded px-2.5 py-0.5 text-[11px] font-medium transition-colors ${
                        rangeDays === opt.days
                          ? "bg-accent text-white"
                          : "text-text-secondary hover:text-accent"
                      }`}
                    >
                      {opt.labelZh}
                    </button>
                  ))}
                </div>
              </div>
            </div>
            {(() => {
              const BAR_H = 110; // px
              const maxReq = Math.max(...stats.daily.map((x) => x.total), 1);
              // Pick a clean tick value at the top (round up to nearest "nice" number).
              const niceMax = (() => {
                const orders = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000];
                return orders.find((o) => o >= maxReq) ?? maxReq;
              })();
              const Y_AXIS_W = 36; // px reserved for y-axis tick labels
              return (
                <div className="relative" style={{ paddingLeft: Y_AXIS_W }}>
                  {/* Y-axis tick labels, aligned with grid lines */}
                  <div
                    className="pointer-events-none absolute left-0 top-0 flex flex-col justify-between text-right text-[10px] font-mono text-text-muted"
                    style={{ height: BAR_H, width: Y_AXIS_W - 6 }}
                  >
                    <span className="-translate-y-1/2">{niceMax.toLocaleString()}</span>
                    <span className="-translate-y-1/2">{Math.round(niceMax / 2).toLocaleString()}</span>
                    <span className="-translate-y-1/2">0</span>
                  </div>
                  {/* Horizontal grid lines */}
                  <div className="pointer-events-none absolute inset-y-0 right-0" style={{ left: Y_AXIS_W, height: BAR_H }}>
                    <div className="h-px w-full bg-border/40" />
                    <div className="absolute top-1/2 h-px w-full bg-border/30" />
                    <div className="absolute bottom-0 h-px w-full bg-border/40" />
                  </div>
                  {/* Density tuning: gap shrinks + bar caps shrink as bar
                      count grows so 30-day view doesn't overflow. */}
                  <div className={`relative flex items-end justify-between px-1 ${stats.daily.length > 20 ? "gap-1" : stats.daily.length > 10 ? "gap-2" : "gap-3"}`} style={{ height: BAR_H }}>
                    {stats.daily.map((d) => {
                      const successCount = Math.max(d.total - d.errors, 0);
                      const totalH = d.total > 0 ? Math.max((d.total / niceMax) * BAR_H, 2) : 0;
                      const errH = d.errors > 0 && totalH > 0 ? Math.max((d.errors / d.total) * totalH, 2) : 0;
                      const tooltip = `${d.date}\n请求: ${d.total} (成功 ${successCount} / 错误 ${d.errors})\nTokens: in ${formatTokens(d.input_tokens)} · out ${formatTokens(d.output_tokens)}`;
                      return (
                        <div
                          key={d.date}
                          className="group relative flex flex-1 items-end justify-center"
                          style={{ height: BAR_H }}
                          title={tooltip}
                        >
                          {/* Bar */}
                          <div
                            className="flex w-full flex-col items-center justify-end overflow-hidden rounded-md transition-opacity group-hover:opacity-80"
                            style={{ maxWidth: stats.daily.length > 20 ? 18 : stats.daily.length > 10 ? 24 : 32 }}
                          >
                            {totalH > 0 ? (
                              <>
                                {errH > 0 && <div className="w-full bg-error/65" style={{ height: errH }} />}
                                <div className="w-full bg-accent/70" style={{ height: totalH - errH }} />
                              </>
                            ) : (
                              // Empty day — show a faint baseline so the column is visible.
                              <div className="w-full bg-border/40" style={{ height: 2 }} />
                            )}
                          </div>
                          {/* Hover total badge */}
                          {d.total > 0 && (
                            <div
                              className="pointer-events-none absolute opacity-0 transition-opacity group-hover:opacity-100"
                              style={{ bottom: totalH + 4 }}
                            >
                              <span className="rounded bg-text-primary px-1.5 py-0.5 font-mono text-[10px] text-card whitespace-nowrap">{d.total}</span>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                  {/* X-axis labels: date + counts row, aligned with bars (already
                      indented by parent padding-left). */}
                  <div className={`mt-2 flex items-start justify-between px-1 ${stats.daily.length > 20 ? "gap-1" : stats.daily.length > 10 ? "gap-2" : "gap-3"}`}>
                    {stats.daily.map((d, i) => {
                      const tokTotal = d.input_tokens + d.output_tokens;
                      const n = stats.daily.length;
                      const stride = n > 20 ? 3 : n > 10 ? 2 : 1;
                      const showDate = i % stride === 0 || i === n - 1;
                      const showTokens = n <= 14;
                      return (
                        <div key={d.date} className="flex flex-1 flex-col items-center gap-0.5">
                          <span className="text-[10px] text-text-muted">{showDate ? d.date.slice(5) : ""}</span>
                          <span className="font-mono text-[11px] font-medium text-text-primary tabular-nums">
                            {d.total > 0 ? d.total.toLocaleString() : "—"}
                          </span>
                          {showTokens && (
                            <span className="font-mono text-[9px] text-text-muted tabular-nums">
                              {tokTotal > 0 ? formatTokens(tokTotal) : " "}
                            </span>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              );
            })()}
            {/* Top providers — inline strip under the chart in the same card. */}
            {(() => {
              const visible = stats.providers.filter((p) => p.name !== "unknown");
              if (visible.length === 0) return null;
              return (
                <div className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-border pt-3 text-[11px]">
                  <span className="font-medium text-text-secondary">{t("stats.top_providers")}</span>
                  {visible.slice(0, 6).map((p, i) => (
                    <div key={p.name} className="flex items-center gap-1">
                      {i > 0 && <span className="text-text-muted/40">·</span>}
                      <span className="text-text-primary">{p.name}</span>
                      <span className="font-mono text-text-muted">{p.count.toLocaleString()}</span>
                    </div>
                  ))}
                </div>
              );
            })()}
          </div>
        </>
      )}

      {/* ── 3.5 成本分解：钱花在哪个模型 / 哪个客户端。仅有数据时显示。 ── */}
      {(costByModel.length > 0 || costByClient.length > 0) && (
        <div className="rounded-xl border border-border bg-card p-5" style={{ boxShadow: "var(--shadow-sm)" }}>
          <h3 className="mb-4 flex items-center gap-2 text-sm font-semibold text-text-primary">
            <Coins className="h-4 w-4 text-text-muted" />
            {t("stats.cost_breakdown")}
          </h3>
          <div className={`grid gap-6 sm:grid-cols-2${costByStrategy.length > 0 ? " lg:grid-cols-3" : ""}`}>
            <CostList title={t("stats.cost_by_model")} rows={costByModel} />
            <CostList title={t("stats.cost_by_client")} rows={costByClient} />
            {costByStrategy.length > 0 && (
              <CostList title={t("stats.cost_by_strategy")} rows={costByStrategy} />
            )}
          </div>
        </div>
      )}

      {/* ── 4. Recent requests with inline tool status header chip. ── */}
      <RecentRequests requests={recentLogs} tools={tools} />

      {/* ── 7. Runtime KPI footer ── */}
      <RuntimeFooter />
    </div>
  );
}
