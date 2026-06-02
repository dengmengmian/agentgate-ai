import { useState, useEffect, useCallback } from "react";
import { ScrollText, RefreshCcw, ChevronLeft, ChevronRight, LayoutList, Layers, Download } from "lucide-react";
import { RequestLogTable, sourceLabel } from "@/components/logs/RequestLogTable";
import { RequestDetailDrawer } from "@/components/logs/RequestDetailDrawer";
import { SessionGroupView } from "@/components/logs/SessionGroupView";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { RequestLogListItem } from "@/types/request-log";
import type { RequestLogDetail } from "@/types/request-log";
import type { ProviderView } from "@/types/provider";

// 客户端候选——detect_client_from_ua 用的固定列表。
const KNOWN_CLIENTS = ["Codex", "Claude Code", "OpenCode", "Gemini CLI", "AtomCode", "Generic"];

const PAGE_SIZE = 100;

export function Logs() {
  const { t } = useI18n();
  const [logs, setLogs] = useState<RequestLogListItem[]>([]);
  const [selected, setSelected] = useState<RequestLogDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [confirmClear, setConfirmClear] = useState(false);

  // Filters
  const [keyword, setKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState("");
  const [providerFilter, setProviderFilter] = useState("");
  const [clientFilter, setClientFilter] = useState("");
  // 'all' / 'gateway' / 'session_log'（聚合所有客户端日志）/ 单一来源（'claude_session' 等）
  const [sourceFilter, setSourceFilter] = useState<string>("");
  // 'list'（按时间逐条）/ 'session'（按会话聚合）
  const [viewMode, setViewMode] = useState<"list" | "session">("list");
  const [providerOptions, setProviderOptions] = useState<ProviderView[]>([]);

  // Pagination
  const [page, setPage] = useState(1); // 1-indexed
  const [total, setTotal] = useState(0);

  // 初次加载 provider 候选——用 name 而不是 id，因为 request_logs.provider
  // 字段存的是 name 字符串（见 routes.rs log_request_success 调用）。
  useEffect(() => {
    api.listProviders().then(setProviderOptions).catch(() => {});
  }, []);

  // Reset to page 1 whenever filters change.
  useEffect(() => {
    setPage(1);
  }, [keyword, statusFilter, providerFilter, clientFilter, sourceFilter]);

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      const filter = {
        keyword: keyword || undefined,
        status: statusFilter || undefined,
        provider: providerFilter || undefined,
        client: clientFilter || undefined,
        source: sourceFilter || undefined,
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
  }, [keyword, statusFilter, providerFilter, clientFilter, sourceFilter, page]);

  useEffect(() => {
    loadLogs();
  }, [loadLogs]);

  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

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
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <p className="shrink-0 text-xs text-text-muted whitespace-nowrap">
            共 {total.toLocaleString()} {t("logs.requests")}
          </p>
          <input
            type="text"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            placeholder={t("logs.search")}
            className="form-input w-48"
          />
          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value)}
            className="form-input w-28"
          >
            <option value="">{t("logs.all")}</option>
            <option value="success">{t("logs.success")}</option>
            <option value="error">{t("logs.error")}</option>
          </select>
          {providerOptions.length > 1 && (
            <select
              value={providerFilter}
              onChange={(e) => setProviderFilter(e.target.value)}
              className="form-input w-36"
              title={t("logs.filter_provider")}
            >
              <option value="">{t("logs.all_providers")}</option>
              {providerOptions.map(p => <option key={p.id} value={p.name}>{p.name}</option>)}
            </select>
          )}
          <select
            value={clientFilter}
            onChange={(e) => setClientFilter(e.target.value)}
            className="form-input w-32"
            title={t("logs.filter_client")}
          >
            <option value="">{t("logs.all_clients")}</option>
            {KNOWN_CLIENTS.map(c => <option key={c} value={c}>{c}</option>)}
          </select>
          <select
            value={sourceFilter}
            onChange={(e) => setSourceFilter(e.target.value)}
            className="form-input w-32"
            title="按来源过滤：网关 / 各客户端本地日志"
          >
            <option value="">全部来源</option>
            <option value="gateway">{sourceLabel("gateway")}</option>
            <option value="session_log">客户端日志（全部）</option>
            <option value="claude_session">{sourceLabel("claude_session")}</option>
            <option value="codex_session">{sourceLabel("codex_session")}</option>
            <option value="gemini_session">{sourceLabel("gemini_session")}</option>
          </select>
        </div>
        <div className="flex items-center gap-2">
          {/* 列表/会话两种视图切换——列表按时间逐条，会话按 session_id 聚合 */}
          <div className="flex items-center rounded-md bg-card-secondary p-0.5">
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
            onClick={handleSyncClaude}
            disabled={syncing !== null}
            className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-50"
            title="扫描 ~/.claude/projects/ 下的会话日志，补齐绕过网关使用 Claude Code 时的用量记录"
          >
            <Download className={`h-3 w-3 ${syncing === "claude" ? "animate-pulse" : ""}`} />
            同步 Claude
          </button>
          <button
            onClick={handleSyncCodex}
            disabled={syncing !== null}
            className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-50"
            title="扫描 ~/.codex/sessions/ 下的会话日志，补齐绕过网关使用 Codex 时的用量记录"
          >
            <Download className={`h-3 w-3 ${syncing === "codex" ? "animate-pulse" : ""}`} />
            同步 Codex
          </button>
          <button
            onClick={handleSyncGemini}
            disabled={syncing !== null}
            className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-50"
            title="扫描 ~/.gemini/tmp/ 下的会话日志，补齐绕过网关使用 Gemini CLI 时的用量记录"
          >
            <Download className={`h-3 w-3 ${syncing === "gemini" ? "animate-pulse" : ""}`} />
            同步 Gemini
          </button>
          <button
            onClick={loadLogs}
            disabled={loading}
            className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
          >
            <RefreshCcw className={`h-3 w-3 ${loading ? "animate-spin" : ""}`} />
            {t("common.refresh")}
          </button>
          <button
            onClick={() => setConfirmClear(true)}
            className="rounded-md bg-card-secondary px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
          >
            {t("logs.clear")}
          </button>
        </div>
      </div>

      {viewMode === "session" ? (
        <SessionGroupView
          onPickSession={(sid) => {
            // 点击某会话 → 切回列表视图并 filter 到该 session_id
            setViewMode("list");
            // session_id filter 在 RequestLogFilter 里，但目前 UI 没有 input；
            // 简化：点会话直接显示 keyword 过滤，让用户看到属于这个 session 的所有条目
            setKeyword(sid.slice(0, 16));
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
