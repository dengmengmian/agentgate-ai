import { useState, useEffect, useCallback } from "react";
import { ScrollText, RefreshCcw } from "lucide-react";
import { RequestLogTable } from "@/components/logs/RequestLogTable";
import { RequestDetailDrawer } from "@/components/logs/RequestDetailDrawer";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { RequestLogListItem } from "@/types/request-log";
import type { RequestLogDetail } from "@/types/request-log";

export function Logs() {
  const { t } = useI18n();
  const [logs, setLogs] = useState<RequestLogListItem[]>([]);
  const [selected, setSelected] = useState<RequestLogDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [confirmClear, setConfirmClear] = useState(false);

  // Filters
  const [keyword, setKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState("");

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.listRequestLogs({
        keyword: keyword || undefined,
        status: statusFilter || undefined,
        limit: 200,
      });
      setLogs(data);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setLoading(false);
    }
  }, [keyword, statusFilter]);

  useEffect(() => {
    loadLogs();
  }, [loadLogs]);

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
            {logs.length} {t("logs.requests")}
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
            className="form-input w-32"
          >
            <option value="">{t("logs.all")}</option>
            <option value="success">{t("logs.success")}</option>
            <option value="error">{t("logs.error")}</option>
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
        <RequestLogTable requests={logs} onSelect={handleSelect} />
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
