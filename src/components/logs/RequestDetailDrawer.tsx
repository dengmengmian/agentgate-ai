import { Info } from "lucide-react";
import { DetailDrawer } from "@/components/layout/DetailDrawer";
import { JsonCodeBlock } from "@/components/common/JsonCodeBlock";
import { ErrorExplanationCard } from "@/components/common/ErrorExplanationCard";
import { StatusBadge } from "@/components/common/StatusBadge";
import { formatTimestamp, formatLatency } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";
import { sourceLabel } from "@/components/logs/RequestLogTable";
import type { RequestLogDetail } from "@/types/request-log";

interface RouteDecisionTrace {
  profile_name?: string;
  mode?: string;
  selected_provider_name?: string;
  selected_model?: string;
  matched_conditions?: Record<string, unknown> | null;
  candidates?: Array<{
    provider_name?: string;
    priority?: number;
    model?: string | null;
    in_cooldown?: boolean;
    has_conditions?: boolean;
  }>;
}

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
  const routeDecision = parseRouteDecision(request.trace_json);

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

        {routeDecision && <RouteDecisionCard decision={routeDecision} />}

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

function parseRouteDecision(traceJson: string | null): RouteDecisionTrace | null {
  if (!traceJson) return null;
  try {
    const parsed = JSON.parse(traceJson) as { route_decision?: RouteDecisionTrace };
    return parsed.route_decision ?? null;
  } catch {
    return null;
  }
}

function formatConditions(conditions: Record<string, unknown> | null | undefined): string {
  if (!conditions || Object.keys(conditions).length === 0) return "—";
  return Object.entries(conditions)
    .map(([key, value]) => {
      if (Array.isArray(value)) return `${key}: ${value.join(", ")}`;
      return `${key}: ${String(value)}`;
    })
    .join(" · ");
}

function RouteDecisionCard({ decision }: { decision: RouteDecisionTrace }) {
  const { t } = useI18n();

  return (
    <div className="rounded-lg border border-border bg-card-secondary p-4">
      <div className="mb-3 flex items-center justify-between">
        <h4 className="text-xs font-semibold text-text-primary">{t("logs.route_decision")}</h4>
        {decision.mode && <StatusBadge variant="muted">{decision.mode}</StatusBadge>}
      </div>
      <div className="grid grid-cols-2 gap-3 text-xs">
        <div>
          <span className="text-text-muted">{t("logs.route_profile")}</span>
          <p className="text-text-primary">{decision.profile_name ?? "—"}</p>
        </div>
        <div>
          <span className="text-text-muted">{t("logs.selected_provider")}</span>
          <p className="text-text-primary">{decision.selected_provider_name ?? "—"}</p>
        </div>
        <div>
          <span className="text-text-muted">{t("logs.selected_model")}</span>
          <p className="font-mono text-text-primary">{decision.selected_model ?? "—"}</p>
        </div>
        <div>
          <span className="text-text-muted">{t("logs.matched_conditions")}</span>
          <p className="text-text-primary">{formatConditions(decision.matched_conditions)}</p>
        </div>
      </div>
      {decision.candidates && decision.candidates.length > 0 && (
        <div className="mt-3 space-y-1">
          <span className="text-[11px] text-text-muted">{t("logs.route_candidates")}</span>
          <div className="flex flex-wrap gap-1.5">
            {decision.candidates.map((candidate, idx) => (
              <span
                key={`${candidate.provider_name ?? "candidate"}-${idx}`}
                className="rounded-md border border-border bg-card px-2 py-1 text-[11px] text-text-secondary"
              >
                {candidate.priority ?? idx + 1}. {candidate.provider_name ?? "—"}
                {candidate.in_cooldown ? ` · ${t("routes.cooldown")}` : ""}
                {candidate.has_conditions ? ` · ${t("routes.has_conditions")}` : ""}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
