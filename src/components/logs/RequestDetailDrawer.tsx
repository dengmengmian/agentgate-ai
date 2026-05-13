import { DetailDrawer } from "@/components/layout/DetailDrawer";
import { JsonCodeBlock } from "@/components/common/JsonCodeBlock";
import { ErrorExplanationCard } from "@/components/common/ErrorExplanationCard";
import { StatusBadge } from "@/components/common/StatusBadge";
import { formatTimestamp, formatLatency } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";
import type { RequestLogDetail } from "@/types/request-log";

interface RequestDetailDrawerProps {
  request: RequestLogDetail | null;
  onClose: () => void;
}

export function RequestDetailDrawer({
  request,
  onClose,
}: RequestDetailDrawerProps) {
  const { t } = useI18n();

  if (!request) return null;

  const isError =
    request.status_code !== null &&
    (request.status_code >= 400 || request.status_code < 200);

  return (
    <DetailDrawer
      open={!!request}
      onClose={onClose}
      title={`${t("common.details")} ${request.request_id}`}
    >
      <div className="space-y-5">
        <div className="grid grid-cols-2 gap-3 text-xs">
          <div>
            <span className="text-text-muted">{t("logs.time")}</span>
            <p className="font-mono text-text-primary">
              {formatTimestamp(request.timestamp)}
            </p>
          </div>
          <div>
            <span className="text-text-muted">{t("logs.status")}</span>
            <p className="mt-0.5">
              <StatusBadge variant={isError ? "error" : "success"}>
                {request.status_code ?? "—"}
              </StatusBadge>
            </p>
          </div>
          <div>
            <span className="text-text-muted">{t("logs.client")}</span>
            <p className="text-text-primary">{request.client ?? "—"}</p>
          </div>
          <div>
            <span className="text-text-muted">{t("logs.provider")}</span>
            <p className="text-text-primary">{request.provider ?? "—"}</p>
          </div>
          <div>
            <span className="text-text-muted">{t("logs.model")}</span>
            <p className="font-mono text-text-primary">{request.model ?? "—"}</p>
          </div>
          <div>
            <span className="text-text-muted">{t("logs.latency")}</span>
            <p className="font-mono text-text-primary">
              {request.latency_ms !== null ? formatLatency(request.latency_ms) : "—"}
            </p>
          </div>
          <div>
            <span className="text-text-muted">{t("logs.route")}</span>
            <p className="font-mono text-text-primary">{request.route ?? "—"}</p>
          </div>
          {request.input_tokens !== null && (
            <div>
              <span className="text-text-muted">{t("logs.tokens")}</span>
              <p className="font-mono text-text-primary">
                {request.input_tokens} in / {request.output_tokens ?? 0} out
              </p>
            </div>
          )}
        </div>

        {request.error_message && (
          <ErrorExplanationCard
            statusCode={request.status_code ?? 0}
            message={request.error_message}
          />
        )}

        {request.raw_request && (
          <JsonCodeBlock title={t("logs.raw_request")} content={request.raw_request} />
        )}
        {request.converted_request && (
          <JsonCodeBlock title={t("logs.converted_request")} content={request.converted_request} />
        )}
        {request.raw_response && (
          <JsonCodeBlock title={t("logs.raw_response")} content={request.raw_response} />
        )}
        {request.converted_response && (
          <JsonCodeBlock title={t("logs.converted_response")} content={request.converted_response} />
        )}
        {request.tool_calls && (
          <JsonCodeBlock title={t("logs.tool_calls")} content={request.tool_calls} />
        )}
        {request.trace_json && (
          <JsonCodeBlock title={t("logs.trace")} content={request.trace_json} />
        )}
      </div>
    </DetailDrawer>
  );
}
