import { useState } from "react";
import { Info, MessageSquare } from "lucide-react";
import { DetailDrawer } from "@/components/layout/DetailDrawer";
import { ConversationModal } from "@/components/logs/ConversationModal";
import { JsonCodeBlock } from "@/components/common/JsonCodeBlock";
import { ErrorExplanationCard } from "@/components/common/ErrorExplanationCard";
import { StatusBadge } from "@/components/common/StatusBadge";
import { formatTimestamp, formatOptionalLatency } from "@/lib/utils";
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
    skip_reasons?: string[];
  }>;
  fallback_chain?: Array<{
    provider_name?: string;
    role?: "primary" | "fallback";
    step?: number;
    selected?: boolean;
  }>;
}

interface RequestTrace {
  route_decision?: RouteDecisionTrace;
  error_mapper?: {
    upstream_code?: string | null;
    upstream_message?: string | null;
    mapped_code?: string;
    mapped_message?: string;
  };
  circuit_breaker?: {
    observed_state?: string;
    transition?: string | null;
    provider_id?: string;
  };
  degradation?: {
    requested_model?: string;
    chain?: string[];
    picked?: string | null;
    reason?: string;
  };
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
  const [convoOpen, setConvoOpen] = useState(false);

  if (!request) return null;

  const isError =
    request.status_code !== null &&
    (request.status_code >= 400 || request.status_code < 200);
  const trace = parseTrace(request.trace_json);
  const routeDecision = trace?.route_decision ?? null;
  const totalTokens = (request.input_tokens ?? 0) + (request.output_tokens ?? 0);

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

        {/* 7.4 日志→会话:有 session_id 就给一个入口直接看整段会话对话 */}
        {request.session_id && (
          <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-card-secondary px-3 py-2">
            <div className="min-w-0">
              <span className="text-[11px] text-text-muted">所属会话</span>
              <p className="truncate font-mono text-[11px] text-text-primary" title={request.session_id}>{request.session_id}</p>
            </div>
            <button
              onClick={() => setConvoOpen(true)}
              className="flex shrink-0 items-center gap-1 rounded-md border border-border px-2 py-1 text-[11px] text-text-secondary transition-colors hover:text-accent"
            >
              <MessageSquare className="h-3.5 w-3.5" /> 查看会话对话
            </button>
          </div>
        )}
        {convoOpen && request.session_id && (
          <ConversationModal
            sessionId={request.session_id}
            source={request.source ?? ""}
            onClose={() => setConvoOpen(false)}
          />
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
              {formatOptionalLatency(request.latency_ms)}
            </p>
          </div>
          <div>
            <span className="text-text-muted">{t("logs.route")}</span>
            <p className="font-mono text-text-primary">{request.route ?? "—"}</p>
          </div>
        </div>

        <div className="rounded-lg border border-border bg-card-secondary p-4">
          <h4 className="mb-3 text-xs font-semibold text-text-primary">{t("logs.usage_and_cost")}</h4>
          <div className="grid grid-cols-2 gap-3 text-xs">
            <Metric label={t("logs.tokens_input")} value={request.input_tokens?.toLocaleString() ?? "—"} />
            <Metric label={t("logs.tokens_output")} value={request.output_tokens?.toLocaleString() ?? "—"} />
            <Metric label={t("logs.tokens_total")} value={totalTokens > 0 ? totalTokens.toLocaleString() : "—"} />
            <Metric label={t("logs.cost")} value={formatCost(request.cost)} />
            <Metric label={t("logs.cache_write")} value={request.cache_write_tokens?.toLocaleString() ?? "—"} />
            <Metric label={t("logs.cache_read")} value={request.cache_read_tokens?.toLocaleString() ?? "—"} />
          </div>
        </div>

        {request.error_message && (
          <ErrorExplanationCard
            statusCode={request.status_code ?? 0}
            message={request.error_message}
          />
        )}

        {isError && <ErrorChainCard request={request} trace={trace} />}

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

function parseTrace(traceJson: string | null): RequestTrace | null {
  if (!traceJson) return null;
  try {
    return JSON.parse(traceJson) as RequestTrace;
  } catch {
    return null;
  }
}

function formatCost(cost: number | null): string {
  if (cost == null) return "—";
  if (cost <= 0) return "$0.00";
  if (cost < 0.01) return `$${cost.toFixed(4)}`;
  return `$${cost.toFixed(2)}`;
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span className="text-text-muted">{label}</span>
      <p className="font-mono text-text-primary">{value}</p>
    </div>
  );
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
                {candidate.has_conditions ? ` · ${t("routes.has_conditions")}` : ""}
                {candidate.skip_reasons?.length ? ` · ${candidate.skip_reasons.map((r) => skipReasonLabel(r, t)).join(", ")}` : ""}
              </span>
            ))}
          </div>
        </div>
      )}
      {decision.fallback_chain && decision.fallback_chain.length > 0 && (
        <div className="mt-3 space-y-1">
          <span className="text-[11px] text-text-muted">{t("logs.fallback_chain")}</span>
          <div className="flex flex-wrap gap-1.5">
            {decision.fallback_chain.map((step, idx) => (
              <span
                key={`${step.provider_name ?? "fallback"}-${idx}`}
                className={`rounded-md border px-2 py-1 text-[11px] ${
                  step.selected
                    ? "border-accent/40 bg-accent/10 text-accent"
                    : "border-border bg-card text-text-secondary"
                }`}
              >
                {step.step ?? idx + 1}. {step.role === "primary" ? t("logs.fallback_primary") : t("logs.fallback_backup")} · {step.provider_name ?? "—"}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function ErrorChainCard({ request, trace }: { request: RequestLogDetail; trace: RequestTrace | null }) {
  const { t } = useI18n();
  const mapper = trace?.error_mapper;
  const breaker = trace?.circuit_breaker;
  const degradation = trace?.degradation;

  return (
    <div className="rounded-lg border border-error/20 bg-error/5 p-4">
      <h4 className="mb-3 text-xs font-semibold text-text-primary">{t("logs.error_chain")}</h4>
      <div className="space-y-2 text-xs">
        <div className="rounded-md border border-border bg-card px-3 py-2">
          <span className="text-text-muted">{t("logs.error_final")}</span>
          <p className="mt-1 text-text-primary">
            HTTP {request.status_code ?? "—"} · {request.error_message ?? "—"}
          </p>
        </div>
        {mapper && (
          <div className="rounded-md border border-border bg-card px-3 py-2">
            <span className="text-text-muted">{t("logs.error_mapper")}</span>
            <p className="mt-1 text-text-primary">
              {mapper.upstream_code ?? "upstream"} → {mapper.mapped_code ?? "mapped"}
            </p>
            <p className="mt-1 truncate text-text-muted" title={mapper.upstream_message ?? mapper.mapped_message ?? ""}>
              {mapper.upstream_message ?? mapper.mapped_message ?? "—"}
            </p>
          </div>
        )}
        {breaker && (
          <div className="rounded-md border border-border bg-card px-3 py-2">
            <span className="text-text-muted">{t("logs.circuit_breaker")}</span>
            <p className="mt-1 text-text-primary">
              {breaker.observed_state ?? "—"}{breaker.transition ? ` · ${breaker.transition}` : ""}
            </p>
          </div>
        )}
        {degradation && (
          <div className="rounded-md border border-border bg-card px-3 py-2">
            <span className="text-text-muted">{t("logs.model_degradation")}</span>
            <p className="mt-1 text-text-primary">
              {degradation.requested_model ?? "—"} → {degradation.picked ?? "—"}
            </p>
            {degradation.chain?.length ? (
              <p className="mt-1 font-mono text-[11px] text-text-muted">{degradation.chain.join(" → ")}</p>
            ) : null}
          </div>
        )}
      </div>
    </div>
  );
}

function skipReasonLabel(reason: string, t: (key: string) => string): string {
  const labels: Record<string, string> = {
    disabled: t("logs.skip_disabled"),
    runtime_unavailable: t("logs.skip_runtime_unavailable"),
    cooldown: t("logs.skip_cooldown"),
    unsupported_vision: t("logs.skip_unsupported_vision"),
  };
  return labels[reason] ?? reason;
}
