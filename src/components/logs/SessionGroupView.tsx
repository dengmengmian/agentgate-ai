import { useEffect, useState } from "react";
import { Loader2, Layers } from "lucide-react";
import { EmptyState } from "@/components/common/EmptyState";
import { formatTimestamp } from "@/lib/utils";
import { toast } from "@/components/common/Toast";
import { sourceLabel } from "@/components/logs/RequestLogTable";
import * as api from "@/lib/api";
import type { SessionUsageSummary } from "@/types/request-log";

interface SessionGroupViewProps {
  /// 点击某行 session 时回调——父组件可以切回「列表」视图并过滤到该 session。
  onPickSession: (sessionId: string) => void;
}

/// Logs 页「按会话聚合」视图。
///
/// 数据来源：`aggregate_request_logs_by_session` Tauri command，对 request_logs
/// 按 `session_id` GROUP BY，跨 gateway / 各客户端本地日志聚合。同一 session_id
/// 同时跨多源时 source 字段返回 'mixed'。
export function SessionGroupView({ onPickSession }: SessionGroupViewProps) {
  const [rows, setRows] = useState<SessionUsageSummary[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      setLoading(true);
      try {
        const data = await api.aggregateRequestLogsBySession(100);
        if (!cancelled) setRows(data);
      } catch (err) {
        if (!cancelled) toast("error", (err as api.AppError).message);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  if (loading) {
    return (
      <div className="flex items-center gap-2 text-xs text-text-muted">
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
        加载中…
      </div>
    );
  }

  if (rows.length === 0) {
    return (
      <EmptyState
        icon={Layers}
        title="还没有会话级数据"
        description="网关请求会按 session_id 自动聚合；客户端本地日志在「同步」之后也会出现在这里。"
      />
    );
  }

  return (
    <div className="overflow-x-auto rounded-xl border border-border bg-card">
      <table className="w-full text-left text-xs">
        <thead>
          <tr className="border-b border-border text-text-muted">
            <th className="px-5 py-3 font-medium">会话</th>
            <th className="px-5 py-3 font-medium">来源</th>
            <th className="px-5 py-3 font-medium">最后活跃</th>
            <th className="px-5 py-3 font-medium text-right">请求数</th>
            <th className="px-5 py-3 font-medium text-right">输入 / 输出</th>
            <th className="px-5 py-3 font-medium text-right">缓存读</th>
            <th className="px-5 py-3 font-medium text-right">费用</th>
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
                <div className="truncate max-w-[280px]" title={row.session_id}>{row.session_id}</div>
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
  );
}

function SourceChip({ source }: { source: string }) {
  const isMixed = source === "mixed";
  const color = source === "gateway"
    ? "bg-accent/15 text-accent"
    : isMixed
      ? "bg-warning/15 text-warning"
      : "bg-card-secondary text-text-secondary";
  return (
    <span className={`inline-flex items-center rounded px-2 py-0.5 text-[10px] font-medium ${color}`}>
      {sourceLabel(source)}
    </span>
  );
}
