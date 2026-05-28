import { useState, useEffect, useCallback } from "react";
import { ScrollText, RefreshCcw, ChevronLeft, ChevronRight } from "lucide-react";
import { RequestLogTable } from "@/components/logs/RequestLogTable";
import { RequestDetailDrawer } from "@/components/logs/RequestDetailDrawer";
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
  }, [keyword, statusFilter, providerFilter, clientFilter]);

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      const filter = {
        keyword: keyword || undefined,
        status: statusFilter || undefined,
        provider: providerFilter || undefined,
        client: clientFilter || undefined,
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
  }, [keyword, statusFilter, providerFilter, clientFilter, page]);

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
        </div>
        <div className="flex items-center gap-2">
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

      {loading ? (
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
