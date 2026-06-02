import { Info } from "lucide-react";
import { DetailDrawer } from "@/components/layout/DetailDrawer";
import { JsonCodeBlock } from "@/components/common/JsonCodeBlock";
import { ErrorExplanationCard } from "@/components/common/ErrorExplanationCard";
import { StatusBadge } from "@/components/common/StatusBadge";
import { formatTimestamp, formatLatency } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";
import { sourceLabel } from "@/components/logs/RequestLogTable";
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
        {/* 非 gateway 来源：raw_request / SSE / tool_calls 等大部分字段为 NULL，
            给个 banner 解释为啥下面一堆字段是空的。 */}
        {request.source && request.source !== "gateway" && (
          <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-3">
            <Info className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning" />
            <div className="text-[11px] leading-relaxed text-text-secondary">
              <span className="font-medium text-text-primary">{sourceLabel(request.source)} 客户端日志条目</span>
              ：从本地会话文件解析而来，只含模型、token、费用等用量字段。
              请求体 / 响应体 / SSE / 工具调用等完整内容**不会**保存到本地日志，因此下方相关区块为空。
              如需完整链路，请让对应客户端走 AgentGate 网关。
            </div>
          </div>
        )}

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
