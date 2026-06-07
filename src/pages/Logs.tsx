import { useState, useEffect, useCallback } from "react";
import { useSearchParams } from "react-router-dom";
import { ScrollText, RefreshCcw, ChevronLeft, ChevronRight, LayoutList, Layers, Download } from "lucide-react";
import { RequestLogTable, sourceLabel } from "@/components/logs/RequestLogTable";
import { RequestDetailDrawer } from "@/components/logs/RequestDetailDrawer";
import { SessionGroupView } from "@/components/logs/SessionGroupView";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import { formatOptionalLatency } from "@/lib/utils";
import * as api from "@/lib/api";
import type { RequestLogListItem } from "@/types/request-log";
import type { RequestLogDetail } from "@/types/request-log";
import type { ProviderView } from "@/types/provider";
import type { RouteProfileView } from "@/types/route-profile";

// 客户端候选——detect_client_from_ua 用的固定列表。
const KNOWN_CLIENTS = ["Codex", "Claude Code", "OpenCode", "Gemini CLI", "AtomCode", "Generic"];

const PAGE_SIZE = 100;
const VALID_SOURCE_FILTERS = new Set(["gateway", "session_log", "claude_session", "codex_session", "gemini_session"]);

export function Logs() {
  const { t } = useI18n();
  const [searchParams] = useSearchParams();
  const [logs, setLogs] = useState<RequestLogListItem[]>([]);
  const [selected, setSelected] = useState<RequestLogDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [confirmClear, setConfirmClear] = useState(false);

  // Filters
  const [keyword, setKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState("");
  const [providerFilter, setProviderFilter] = useState("");
  const [modelFilter, setModelFilter] = useState("");
  const [routeProfileFilter, setRouteProfileFilter] = useState("");
  const [errorTypeFilter, setErrorTypeFilter] = useState("");
  const [clientFilter, setClientFilter] = useState("");
  // 'all' / 'gateway' / 'session_log'（聚合所有客户端日志）/ 单一来源（'claude_session' 等）
  const [sourceFilter, setSourceFilter] = useState<string>("");
  const [sessionIdFilter, setSessionIdFilter] = useState("");
  // 'list'（按时间逐条）/ 'session'（按会话聚合）
  const [viewMode, setViewMode] = useState<"list" | "session">("list");
  const [providerOptions, setProviderOptions] = useState<ProviderView[]>([]);
  const [routeProfileOptions, setRouteProfileOptions] = useState<RouteProfileView[]>([]);
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [showAdvancedFilters, setShowAdvancedFilters] = useState(false);
  const [showSyncActions, setShowSyncActions] = useState(false);

  // Pagination
  const [page, setPage] = useState(1); // 1-indexed
  const [total, setTotal] = useState(0);

  // 初次加载 provider 候选——用 name 而不是 id，因为 request_logs.provider
  // 字段存的是 name 字符串（见 routes.rs log_request_success 调用）。
  useEffect(() => {
    api.listProviders().then(setProviderOptions).catch(() => {});
    api.listRouteProfiles().then(setRouteProfileOptions).catch(() => {});
    api.listLogModels().then(setModelOptions).catch(() => {});
  }, []);

  useEffect(() => {
    const source = searchParams.get("source");
    if (source && VALID_SOURCE_FILTERS.has(source)) setSourceFilter(source);
  }, [searchParams]);

  // Reset to page 1 whenever filters change.
  useEffect(() => {
    setPage(1);
  }, [keyword, statusFilter, providerFilter, modelFilter, routeProfileFilter, errorTypeFilter, clientFilter, sourceFilter, sessionIdFilter]);

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      const filter = {
        keyword: keyword || undefined,
        status: statusFilter || undefined,
        provider: providerFilter || undefined,
        model: modelFilter || undefined,
        route_profile_id: routeProfileFilter || undefined,
        error_type: errorTypeFilter || undefined,
        client: clientFilter || undefined,
        source: sourceFilter || undefined,
        session_id: sessionIdFilter || undefined,
        limit: PAGE_SIZE,
        offset: (page - 1) * PAGE_SIZE,
      };
      const [data, count] = await Promise.all([
        api.listRequestLogs(filter),
        api.countRequestLogs(filter),
      ]);
      setLogs(data);
      setTotal(count);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setLoading(false);
    }
  }, [keyword, statusFilter, providerFilter, modelFilter, routeProfileFilter, errorTypeFilter, clientFilter, sourceFilter, sessionIdFilter, page]);

  useEffect(() => {
    loadLogs();
  }, [loadLogs]);

  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const knownStatusLogs = logs.filter((log) => log.status_code !== null);
  const pageErrorCount = knownStatusLogs.filter((log) => {
    const status = log.status_code ?? 0;
    return status < 200 || status >= 400;
  }).length;
  const pageSuccessCount = knownStatusLogs.filter((log) => {
    const status = log.status_code ?? 0;
    return status >= 200 && status < 400;
  }).length;
  const pageSuccessRate = knownStatusLogs.length > 0
    ? `${Math.round((pageSuccessCount / knownStatusLogs.length) * 100)}%`
    : "—";
  const pageRecordedLatencies = logs
    .map((log) => log.latency_ms)
    .filter((latency): latency is number => latency !== null && latency > 0);
  const pageAvgLatency = pageRecordedLatencies.length > 0
    ? Math.round(pageRecordedLatencies.reduce((sum, latency) => sum + latency, 0) / pageRecordedLatencies.length)
    : null;
  const advancedFilterCount = [errorTypeFilter, clientFilter, sourceFilter, sessionIdFilter].filter(Boolean).length;

  const handleSelect = async (item: RequestLogListItem) => {
    try {
      const detail = await api.getRequestLogDetail(item.id);
      setSelected(detail);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleClear = async () => {
    try {
      await api.clearRequestLogs();
      toast("success", t("logs.cleared"));
      setConfirmClear(false);
      loadLogs();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const [syncing, setSyncing] = useState<null | "claude" | "codex" | "gemini">(null);
  const runSync = async (
    kind: "claude" | "codex" | "gemini",
    label: string,
    fn: () => Promise<api.SyncResult>,
    missingHint: string,
  ) => {
    setSyncing(kind);
    try {
      const r = await fn();
      if (r.files_scanned === 0) {
        toast("success", missingHint);
      } else {
        toast("success", `${label} 已扫描 ${r.files_scanned} 个文件：新增 ${r.imported}，跳过 ${r.skipped}` +
          (r.errors.length > 0 ? `，${r.errors.length} 个错误` : ""));
      }
      loadLogs();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setSyncing(null);
    }
  };
  const handleSyncClaude = () =>
    runSync("claude", "Claude", api.syncClaudeSessions, "未找到 Claude Code 会话目录（~/.claude/projects/）");
  const handleSyncCodex = () =>
    runSync("codex", "Codex", api.syncCodexSessions, "未找到 Codex 会话目录（~/.codex/sessions/）");
  const handleSyncGemini = () =>
    runSync("gemini", "Gemini", api.syncGeminiSessions, "未找到 Gemini CLI 会话目录（~/.gemini/tmp/）");

  return (
    <div className="space-y-4">
      <div className="space-y-3">
        <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
          <LogSummaryItem label="筛选命中" value={total.toLocaleString()} hint={t("logs.requests")} />
          <LogSummaryItem label="本页错误" value={pageErrorCount.toLocaleString()} hint={`${knownStatusLogs.length.toLocaleString()} 条有状态`} />
          <LogSummaryItem label="本页成功率" value={pageSuccessRate} hint={`${pageSuccessCount.toLocaleString()} 条成功`} />
          <LogSummaryItem label="本页平均延迟" value={formatOptionalLatency(pageAvgLatency)} hint={`${pageRecordedLatencies.length.toLocaleString()} 条有记录`} />
        </div>

        <div className="rounded-xl border border-border bg-card p-3">
          <div className="flex flex-wrap items-center gap-2">
            <input
              type="text"
              value={keyword}
              onChange={(e) => setKeyword(e.target.value)}
              placeholder={t("logs.search")}
              className="form-input min-w-[13rem] flex-1"
            />
            <select
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
              className="form-input !w-28 shrink-0"
            >
              <option value="">{t("logs.all")}</option>
              <option value="success">{t("logs.success")}</option>
              <option value="error">{t("logs.error")}</option>
            </select>
            {providerOptions.length > 1 && (
              <select
                value={providerFilter}
                onChange={(e) => setProviderFilter(e.target.value)}
                className="form-input !w-36 shrink-0"
                title={t("logs.filter_provider")}
              >
                <option value="">{t("logs.all_providers")}</option>
                {providerOptions.map(p => <option key={p.id} value={p.name}>{p.name}</option>)}
              </select>
            )}
            {modelOptions.length > 0 && (
              <select
                value={modelFilter}
                onChange={(e) => setModelFilter(e.target.value)}
                className="form-input !w-40 shrink-0"
                title={t("logs.filter_model")}
              >
                <option value="">{t("logs.all_models")}</option>
                {modelOptions.map((m) => <option key={m} value={m}>{m}</option>)}
              </select>
            )}
            {routeProfileOptions.length > 0 && (
              <select
                value={routeProfileFilter}
                onChange={(e) => setRouteProfileFilter(e.target.value)}
                className="form-input !w-40 shrink-0"
                title={t("logs.filter_route_profile")}
              >
                <option value="">{t("logs.all_routes")}</option>
                {routeProfileOptions.map((profile) => (
                  <option key={profile.id} value={profile.id}>{profile.name}</option>
                ))}
              </select>
            )}
            <button
              type="button"
              onClick={() => setShowAdvancedFilters((v) => !v)}
              className={`shrink-0 whitespace-nowrap rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${showAdvancedFilters ? "bg-card-secondary text-text-primary" : "text-text-muted hover:bg-card-secondary hover:text-text-primary"}`}
            >
              {showAdvancedFilters ? "收起筛选" : `高级筛选${advancedFilterCount > 0 ? ` (${advancedFilterCount})` : ""}`}
            </button>
          </div>

          {showAdvancedFilters && (
            <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border pt-3">
              <select
                value={errorTypeFilter}
                onChange={(e) => setErrorTypeFilter(e.target.value)}
                className="form-input !w-36 shrink-0"
                title={t("logs.filter_error_type")}
              >
                <option value="">{t("logs.all_error_types")}</option>
                <option value="auth_failed">{t("logs.error_type.auth_failed")}</option>
                <option value="rate_limited">{t("logs.error_type.rate_limited")}</option>
                <option value="quota_or_balance">{t("logs.error_type.quota_or_balance")}</option>
                <option value="server_error">{t("logs.error_type.server_error")}</option>
                <option value="network_error">{t("logs.error_type.network_error")}</option>
                <option value="protocol_error">{t("logs.error_type.protocol_error")}</option>
                <option value="other_error">{t("logs.error_type.other_error")}</option>
              </select>
              <select
                value={clientFilter}
                onChange={(e) => setClientFilter(e.target.value)}
                className="form-input !w-32 shrink-0"
                title={t("logs.filter_client")}
              >
                <option value="">{t("logs.all_clients")}</option>
                {KNOWN_CLIENTS.map(c => <option key={c} value={c}>{c}</option>)}
              </select>
              <select
                value={sourceFilter}
                onChange={(e) => setSourceFilter(e.target.value)}
                className="form-input !w-36 shrink-0"
                title="按来源过滤：网关 / 各客户端本地日志"
              >
                <option value="">全部来源</option>
                <option value="gateway">{sourceLabel("gateway")}</option>
                <option value="session_log">客户端日志（全部）</option>
                <option value="claude_session">{sourceLabel("claude_session")}</option>
                <option value="codex_session">{sourceLabel("codex_session")}</option>
                <option value="gemini_session">{sourceLabel("gemini_session")}</option>
              </select>
              {sessionIdFilter && (
                <button
                  type="button"
                  onClick={() => setSessionIdFilter("")}
                  className="max-w-[260px] truncate rounded-md bg-card-secondary px-2.5 py-1.5 font-mono text-[11px] text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
                  title={sessionIdFilter}
                >
                  session:{sessionIdFilter}
                </button>
              )}
            </div>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* 列表/会话两种视图切换——列表按时间逐条，会话按 session_id 聚合 */}
          <div className="flex shrink-0 items-center rounded-md bg-card-secondary p-0.5">
            <button
              onClick={() => setViewMode("list")}
              className={`flex items-center gap-1 rounded px-2.5 py-1 text-xs font-medium transition-colors ${viewMode === "list" ? "bg-card text-text-primary" : "text-text-muted hover:text-text-primary"}`}
              title="按时间逐条"
            >
              <LayoutList className="h-3 w-3" />
              请求
            </button>
            <button
              onClick={() => setViewMode("session")}
              className={`flex items-center gap-1 rounded px-2.5 py-1 text-xs font-medium transition-colors ${viewMode === "session" ? "bg-card text-text-primary" : "text-text-muted hover:text-text-primary"}`}
              title="按会话聚合用量"
            >
              <Layers className="h-3 w-3" />
              会话
            </button>
          </div>
          <button
            onClick={loadLogs}
            disabled={loading}
            className="flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
          >
            <RefreshCcw className={`h-3 w-3 ${loading ? "animate-spin" : ""}`} />
            {t("common.refresh")}
          </button>
          <button
            type="button"
            onClick={() => setShowSyncActions((v) => !v)}
            className={`flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${showSyncActions ? "bg-card-secondary text-text-primary" : "text-text-muted hover:bg-card-secondary hover:text-text-primary"}`}
          >
            <Download className="h-3 w-3" />
            同步日志
          </button>
          <button
            onClick={() => setConfirmClear(true)}
            className="shrink-0 whitespace-nowrap rounded-md px-3 py-1.5 text-xs font-medium text-text-muted transition-colors hover:bg-card-secondary hover:text-text-primary"
          >
            {t("logs.clear")}
          </button>
        </div>

        {showSyncActions && (
          <div className="flex flex-wrap items-center gap-2 rounded-xl border border-border bg-card p-3">
            <button
              onClick={handleSyncClaude}
              disabled={syncing !== null}
              className="flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-50"
              title="扫描 ~/.claude/projects/ 下的会话日志，补齐绕过网关使用 Claude Code 时的用量记录"
            >
              <Download className={`h-3 w-3 ${syncing === "claude" ? "animate-pulse" : ""}`} />
              同步 Claude
            </button>
            <button
              onClick={handleSyncCodex}
              disabled={syncing !== null}
              className="flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-50"
              title="扫描 ~/.codex/sessions/ 下的会话日志，补齐绕过网关使用 Codex 时的用量记录"
            >
              <Download className={`h-3 w-3 ${syncing === "codex" ? "animate-pulse" : ""}`} />
              同步 Codex
            </button>
            <button
              onClick={handleSyncGemini}
              disabled={syncing !== null}
              className="flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-50"
              title="扫描 ~/.gemini/tmp/ 下的会话日志，补齐绕过网关使用 Gemini CLI 时的用量记录"
            >
              <Download className={`h-3 w-3 ${syncing === "gemini" ? "animate-pulse" : ""}`} />
              同步 Gemini
            </button>
          </div>
        )}
      </div>

      {viewMode === "session" ? (
        <SessionGroupView
          filter={{
            keyword: keyword || undefined,
            status: statusFilter || undefined,
            provider: providerFilter || undefined,
            model: modelFilter || undefined,
            route_profile_id: routeProfileFilter || undefined,
            error_type: errorTypeFilter || undefined,
            client: clientFilter || undefined,
            source: sourceFilter || undefined,
            session_id: sessionIdFilter || undefined,
          }}
          onPickSession={(sid) => {
            setViewMode("list");
            setSessionIdFilter(sid);
          }}
        />
      ) : loading ? (
        <p className="text-xs text-text-muted">{t("common.loading")}</p>
      ) : logs.length === 0 ? (
        <EmptyState
          icon={ScrollText}
          title={t("logs.no_logs")}
          description={t("logs.no_logs_desc")}
        />
      ) : (
        <>
          <RequestLogTable requests={logs} onSelect={handleSelect} />
          {totalPages > 1 && (
            <div className="flex items-center justify-between rounded-xl border border-border bg-card px-5 py-2.5">
              <span className="text-xs text-text-muted">
                第 <span className="font-mono text-text-primary">{page}</span> / {totalPages} 页
                <span className="ml-3 text-text-muted/60">
                  显示 {(page - 1) * PAGE_SIZE + 1}–{Math.min(page * PAGE_SIZE, total)} 条
                </span>
              </span>
              <div className="flex items-center gap-1">
                <button
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                  disabled={page <= 1 || loading}
                  className="flex items-center gap-1 rounded-md bg-card-secondary px-2.5 py-1 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-40 disabled:hover:bg-card-secondary disabled:hover:text-text-secondary"
                >
                  <ChevronLeft className="h-3 w-3" />
                  上一页
                </button>
                <button
                  onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
                  disabled={page >= totalPages || loading}
                  className="flex items-center gap-1 rounded-md bg-card-secondary px-2.5 py-1 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-40 disabled:hover:bg-card-secondary disabled:hover:text-text-secondary"
                >
                  下一页
                  <ChevronRight className="h-3 w-3" />
                </button>
              </div>
            </div>
          )}
        </>
      )}

      <RequestDetailDrawer
        request={selected}
        onClose={() => setSelected(null)}
      />

      <ConfirmDialog
        open={confirmClear}
        title={t("logs.clear_title")}
        message={t("logs.clear_msg")}
        confirmLabel={t("logs.clear_confirm")}
        variant="danger"
        onConfirm={handleClear}
        onCancel={() => setConfirmClear(false)}
      />
    </div>
  );
}

function LogSummaryItem({ label, value, hint }: { label: string; value: string; hint: string }) {
  return (
    <div className="rounded-xl border border-border bg-card px-4 py-3">
      <div className="text-[11px] font-medium text-text-muted">{label}</div>
      <div className="mt-1 flex items-baseline gap-2">
        <span className="font-mono text-lg font-semibold text-text-primary">{value}</span>
        <span className="truncate text-[11px] text-text-muted">{hint}</span>
      </div>
    </div>
  );
}
