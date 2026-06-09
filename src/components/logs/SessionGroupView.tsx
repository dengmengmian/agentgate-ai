import { useEffect, useState } from "react";
import { Loader2, Layers, MessageSquare, Trash2 } from "lucide-react";
import { EmptyState } from "@/components/common/EmptyState";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { ConversationModal } from "@/components/logs/ConversationModal";
import { formatTimestamp } from "@/lib/utils";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import { sourceLabel } from "@/components/logs/RequestLogTable";
import * as api from "@/lib/api";
import type { SessionUsageSummary, RequestLogFilter } from "@/types/request-log";

interface SessionGroupViewProps {
  /// 当前筛选条件——会话视图跟着客户端/来源/模型等筛选走。
  filter: RequestLogFilter;
  /// 点击某行 session 时回调——父组件可以切回「列表」视图并过滤到该 session。
  onPickSession: (sessionId: string) => void;
}

/// Logs 页「按会话聚合」视图。
///
/// 数据来源：`aggregate_request_logs_by_session` Tauri command，对 request_logs
/// 按 `session_id` GROUP BY，跨 gateway / 各客户端本地日志聚合。同一 session_id
/// 同时跨多源时 source 字段返回 'mixed'。
export function SessionGroupView({ filter, onPickSession }: SessionGroupViewProps) {
  const { t } = useI18n();
  const [rows, setRows] = useState<SessionUsageSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [convo, setConvo] = useState<{ sessionId: string; source: string } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  const handleDelete = async () => {
    if (!deleteTarget) return;
    try {
      await api.deleteSession(deleteTarget);
      setRows((prev) => prev.filter((r) => r.session_id !== deleteTarget));
      toast("success", t("logs.session_deleted"));
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
    setDeleteTarget(null);
  };

  useEffect(() => {
    let cancelled = false;
    (async () => {
      setLoading(true);
      try {
        const data = await api.aggregateRequestLogsBySession(filter, 100);
        if (!cancelled) setRows(data);
      } catch (err) {
        if (!cancelled) toast("error", (err as api.AppError).message);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
    // filter 是对象,用 JSON 比较避免每次 render 重跑
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(filter)]);

  if (loading) {
    return (
      <div className="flex items-center gap-2 text-xs text-text-muted">
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
        {t("common.loading")}
      </div>
    );
  }

  if (rows.length === 0) {
    return (
      <EmptyState
        icon={Layers}
        title={t("logs.session_empty_title")}
        description={t("logs.session_empty_desc")}
      />
    );
  }

  return (
    <>
    <div className="overflow-x-auto rounded-xl border border-border bg-card">
      <table className="w-full text-left text-xs">
        <thead>
          <tr className="border-b border-border text-text-muted">
            <th className="px-5 py-3 font-medium">{t("logs.session_col_session")}</th>
            <th className="px-5 py-3 font-medium">{t("logs.session_col_source")}</th>
            <th className="px-5 py-3 font-medium">{t("logs.session_col_last_seen")}</th>
            <th className="px-5 py-3 font-medium text-right">{t("logs.session_col_requests")}</th>
            <th className="px-5 py-3 font-medium text-right">{t("logs.session_col_in_out")}</th>
            <th className="px-5 py-3 font-medium text-right">{t("logs.session_col_cache_read")}</th>
            <th className="px-5 py-3 font-medium text-right">{t("logs.session_col_cost")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.session_id}
              onClick={() => onPickSession(row.session_id)}
              className="cursor-pointer border-b border-border/50 transition-colors hover:bg-hover"
            >
              <td className="px-5 py-2.5 font-mono text-text-primary">
                <div className="flex items-center gap-2">
                  <button
                    onClick={(e) => { e.stopPropagation(); setConvo({ sessionId: row.session_id, source: row.source }); }}
                    className="shrink-0 text-text-muted transition-colors hover:text-accent"
                    title={t("logs.session_view_convo")}
                  >
                    <MessageSquare className="h-3.5 w-3.5" />
                  </button>
                  <button
                    onClick={(e) => { e.stopPropagation(); setDeleteTarget(row.session_id); }}
                    className="shrink-0 text-text-muted transition-colors hover:text-error"
                    title={t("logs.session_delete")}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                  <div className="truncate max-w-[240px]" title={row.session_id}>{row.session_id}</div>
                </div>
                {row.model && (
                  <div className="font-mono text-[10px] text-text-muted truncate" title={row.model}>{row.model}</div>
                )}
              </td>
              <td className="px-5 py-2.5">
                <SourceChip source={row.source} />
              </td>
              <td className="px-5 py-2.5 font-mono text-text-muted">
                {formatTimestamp(row.last_seen)}
              </td>
              <td className="px-5 py-2.5 text-right font-mono text-text-secondary">
                {row.request_count.toLocaleString()}
              </td>
              <td className="px-5 py-2.5 text-right font-mono text-text-secondary">
                {row.input_tokens.toLocaleString()} / {row.output_tokens.toLocaleString()}
              </td>
              <td className="px-5 py-2.5 text-right font-mono text-text-muted">
                {row.cache_read_tokens > 0 ? row.cache_read_tokens.toLocaleString() : "—"}
              </td>
              <td className="px-5 py-2.5 text-right font-mono text-text-primary">
                {row.cost > 0 ? `$${row.cost.toFixed(4)}` : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
    {convo && (
      <ConversationModal sessionId={convo.sessionId} source={convo.source} onClose={() => setConvo(null)} />
    )}
    <ConfirmDialog
      open={!!deleteTarget}
      title={t("logs.session_delete_title")}
      message={t("logs.session_delete_msg")}
      confirmLabel={t("common.delete")}
      variant="danger"
      onConfirm={handleDelete}
      onCancel={() => setDeleteTarget(null)}
    />
    </>
  );
}

function SourceChip({ source }: { source: string }) {
  const { t } = useI18n();
  const isMixed = source === "mixed";
  const color = source === "gateway"
    ? "bg-accent/15 text-accent"
    : isMixed
      ? "bg-warning/15 text-warning"
      : "bg-card-secondary text-text-secondary";
  return (
    <span className={`inline-flex items-center rounded px-2 py-0.5 text-[10px] font-medium ${color}`}>
      {sourceLabel(source, t)}
    </span>
  );
}
