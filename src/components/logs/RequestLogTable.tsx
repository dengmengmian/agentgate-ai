import { StatusBadge } from "@/components/common/StatusBadge";
import { formatTimestamp, formatLatency } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";
import type { RequestLogListItem } from "@/types/request-log";

interface RequestLogTableProps {
  requests: RequestLogListItem[];
  onSelect: (req: RequestLogListItem) => void;
}

export function RequestLogTable({ requests, onSelect }: RequestLogTableProps) {
  const { t } = useI18n();

  return (
    <div className="overflow-x-auto rounded-xl border border-border bg-card">
      <table className="w-full text-left text-xs">
        <thead>
          <tr className="border-b border-border text-text-muted">
            <th className="px-5 py-3 font-medium">{t("logs.time")}</th>
            <th className="px-5 py-3 font-medium">{t("logs.route")}</th>
            <th className="px-5 py-3 font-medium">{t("logs.client")}</th>
            <th className="px-5 py-3 font-medium">{t("logs.provider")}</th>
            <th className="px-5 py-3 font-medium">{t("logs.model")}</th>
            <th className="px-5 py-3 font-medium">{t("logs.status")}</th>
            <th className="px-5 py-3 font-medium text-right">{t("logs.latency")}</th>
          </tr>
        </thead>
        <tbody>
          {requests.map((req) => {
            const isError =
              req.status_code !== null &&
              (req.status_code >= 400 || req.status_code < 200);
            return (
              <tr
                key={req.id}
                onClick={() => onSelect(req)}
                className="cursor-pointer border-b border-border/50 transition-colors hover:bg-hover"
              >
                <td className="px-5 py-2.5 font-mono text-text-muted">
                  {formatTimestamp(req.timestamp)}
                </td>
                <td className="px-5 py-2.5 font-mono text-text-secondary">
                  {req.route ?? "—"}
                </td>
                <td className="px-5 py-2.5 text-text-primary">
                  {req.client ?? "—"}
                </td>
                <td className="px-5 py-2.5 text-text-secondary">
                  {req.provider ?? "—"}
                </td>
                <td className="px-5 py-2.5 font-mono text-text-secondary">
                  {req.model ?? "—"}
                </td>
                <td className="px-5 py-2.5">
                  <StatusBadge variant={isError ? "error" : "success"}>
                    {req.status_code ?? "—"}
                  </StatusBadge>
                </td>
                <td className="px-5 py-2.5 text-right font-mono text-text-secondary">
                  {req.latency_ms !== null ? formatLatency(req.latency_ms) : "—"}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
