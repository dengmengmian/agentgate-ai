import { useState, useEffect } from "react";
import {
  Cloud,
  Key,
  ExternalLink,
  Pencil,
  Trash2,
  Star,
  Loader2,
  Database,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { CapabilityIcons } from "@/components/common/CapabilityIcons";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import {
  getProviderCooldownSummary,
  getProviderSignalSummary,
  summarizeProviderErrorStatuses,
  type ProviderErrorStatus,
} from "@/lib/providerHealth";
import type { ProviderView } from "@/types/provider";
import type { ProviderHealth } from "@/types/stats";
import type { ProviderRuntimeStatus } from "@/types/route-profile";

interface ProviderCardProps {
  provider: ProviderView;
  onEdit: (provider: ProviderView) => void;
  onDelete: (provider: ProviderView) => void;
  onSetActive: (provider: ProviderView) => void;
  onTest: (provider: ProviderView) => void;
  onDetails?: (provider: ProviderView) => void;
  testing?: boolean;
  runtime?: ProviderRuntimeStatus;
  onResetRuntime?: (providerId: string) => void;
}

export function ProviderCard({
  provider,
  onEdit,
  onDelete,
  onSetActive,
  onTest,
  onDetails,
  testing,
  runtime,
  onResetRuntime,
}: ProviderCardProps) {
  const { t } = useI18n();
  const [health, setHealth] = useState<ProviderHealth | null>(null);
  const [showDetails, setShowDetails] = useState(false);

  useEffect(() => {
    api
      .getProviderHealth(provider.name)
      .then(setHealth)
      .catch(() => {});
  }, [provider.name]);

  // ── status dot color ──
  const statusDotColor =
    provider.status === "connected"
      ? "bg-success"
      : provider.status === "failed"
        ? "bg-error"
        : "bg-text-muted";
  const statusLabel =
    provider.status === "connected"
      ? t("providers.status_connected")
      : provider.status === "failed"
        ? t("providers.status_failed")
        : t("providers.status_not_tested");

  // ── parsed protocol labels ──
  const protocolList: string[] = (() => {
    try {
      return JSON.parse(provider.protocol);
    } catch {
      return [provider.protocol];
    }
  })();
  const protocolLabels: string[] = (() => {
    const labels: Record<string, string> = {
      openai_chat_completions: "Chat Completions",
      openai_responses: "Responses",
      anthropic_messages: "Anthropic Messages",
    };
    return protocolList.map((p) => labels[p] || p);
  })();
  // 直连 chips：每个 protocol 表示上游原生支持的入口（=直连路径）。
  // 客户端若用 list 里没有的协议，网关会做协议转换。
  const passThroughChips: { key: string; label: string }[] = (() => {
    const shortLabel: Record<string, string> = {
      openai_chat_completions: "Chat",
      openai_responses: "Responses",
      anthropic_messages: "Anthropic",
    };
    return protocolList.map((p) => ({ key: p, label: shortLabel[p] || p }));
  })();

  // ── cache capability inference (anthropic-style only) ──
  const isAnthropicCapable =
    provider.provider_type === "anthropic" || !!provider.anthropic_base_url;
  const cacheEnabled =
    provider.supports_cache === true ||
    (provider.supports_cache == null &&
      provider.auto_cache_control !== false &&
      isAnthropicCapable);
  const cacheUnsupported =
    provider.supports_cache === false && isAnthropicCapable;

  // ── 故障自愈运行时状态 ──
  // 数据由父页周期刷新（usePolling）拉取并下传；只在「有问题」时亮出，
  // 平时卡片保持干净。cooldown 剩余秒为加载时刻快照，靠周期刷新更新。
  const cooldownSummary = getProviderCooldownSummary(runtime);
  const inCooldown = !!cooldownSummary;
  const h24Failures = health
    ? Math.max(health.h24_total - health.h24_success, 0)
    : 0;
  const latestError = health?.recent_errors[0];
  const errorStatuses = summarizeProviderErrorStatuses(
    health?.recent_errors ?? []
  );
  const signalSummary = getProviderSignalSummary(runtime, health);
  const errorStatusVariant: Record<
    ProviderErrorStatus,
    "error" | "warning" | "muted"
  > = {
    rate_limited: "warning",
    quota_exhausted: "error",
    insufficient_balance: "error",
    auth_failed: "error",
    other_error: "muted",
  };

  return (
    <div
      className={`rounded-xl border bg-card p-5 ${provider.is_active ? "border-accent/40 border-l-2 border-l-accent" : "border-border"}`}
      style={{ boxShadow: "var(--shadow-sm)" }}
    >
      {/* ── Header: icon + name + url ; status dot + capability icons ── */}
      <div className="mb-3 flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-accent-soft">
            <Cloud className="h-4 w-4 text-accent" />
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-sm font-semibold text-text-primary">
                {provider.name}
              </h3>
              {provider.is_active && (
                <StatusBadge variant="accent">
                  {t("providers.active")}
                </StatusBadge>
              )}
            </div>
            <p
              className="truncate font-mono text-[11px] text-text-muted"
              title={provider.base_url}
            >
              {provider.base_url}
            </p>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className="flex items-center gap-1.5" title={statusLabel}>
            <span
              className={`inline-block h-2 w-2 rounded-full ${statusDotColor}`}
            />
          </span>
          <CapabilityIcons
            modelCapabilities={provider.model_capabilities}
            legacyVision={provider.supports_vision}
          />
          {cacheEnabled && (
            <Database
              className="h-3.5 w-3.5 text-accent"
              aria-label={t("providers.cache_enabled")}
            />
          )}
          {cacheUnsupported && (
            <Database
              className="h-3.5 w-3.5 text-text-muted/60"
              aria-label={t("providers.cache_not_supported")}
            />
          )}
        </div>
      </div>

      {/* ── Essentials: model · key · timeout · 直连 chips ── */}
      <div className="mb-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
        <span className="font-mono text-text-primary">
          {provider.default_model}
        </span>
        {provider.masked_api_key && (
          <span className="flex items-center gap-1 text-text-muted">
            <Key className="h-3 w-3" />
            <span className="font-mono text-[11px]">
              {provider.masked_api_key}
            </span>
          </span>
        )}
        <span className="text-text-muted">{provider.timeout_seconds}s</span>
      </div>
      {/* 上游原生支持的协议入口——勾掉的客户端协议靠网关协议转换 */}
      {passThroughChips.length > 0 && (
        <div className="mb-3 flex flex-wrap items-center gap-1">
          {passThroughChips.map((c) => (
            <span
              key={c.key}
              className="rounded bg-success/10 px-1.5 py-0.5 text-[10px] font-medium text-success"
              title={t("providers.pass_through_tooltip")}
            >
              {t("providers.pass_through_prefix")} {c.label}
            </span>
          ))}
        </div>
      )}

      {/* ── Operational status: runtime + probe + real traffic in one vocabulary ── */}
      {(runtime || health) && (
        <div className="mb-3 space-y-1.5 rounded-md border border-border/50 bg-card-secondary/40 px-3 py-2 text-[11px] text-text-muted">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <span className="font-medium text-text-secondary">
              {t("providers.operational_status")}
            </span>
            <StatusBadge
              variant={signalSummary.runtime.variant}
              className="px-1.5 py-0 text-[10px]"
            >
              {t(`providers.runtime_status.${signalSummary.runtime.status}`)}
              {inCooldown && cooldownSummary
                ? ` ${cooldownSummary.remainingSeconds}s`
                : ""}
            </StatusBadge>
            <StatusBadge
              variant={signalSummary.probe.variant}
              className="px-1.5 py-0 text-[10px]"
            >
              {t("providers.probe")} ·{" "}
              {t(`providers.probe_status.${signalSummary.probe.status}`)}
              {signalSummary.probe.latencyMs != null
                ? ` ${signalSummary.probe.latencyMs}ms`
                : ""}
            </StatusBadge>
            <StatusBadge
              variant={signalSummary.traffic.variant}
              className="px-1.5 py-0 text-[10px]"
            >
              {t("providers.traffic")} ·{" "}
              {t(`providers.traffic_status.${signalSummary.traffic.status}`)}
            </StatusBadge>
            {health && health.h24_total > 0 && (
              <>
                <span>1h {health.h1_success_rate}%</span>
                <span>24h {health.h24_success_rate}%</span>
                <span>{health.h1_avg_latency_ms}ms avg</span>
                <span>P95 {health.h1_p95_latency_ms}ms</span>
                <span>
                  {h24Failures} {t("providers.health_failures")}
                </span>
              </>
            )}
            {errorStatuses.map((status) => (
              <StatusBadge
                key={status}
                variant={errorStatusVariant[status]}
                className="px-1.5 py-0 text-[10px]"
              >
                {t(`providers.error_status.${status}`)}
              </StatusBadge>
            ))}
            {onResetRuntime &&
              runtime &&
              signalSummary.runtime.status !== "available" &&
              signalSummary.runtime.status !== "unknown" && (
                <button
                  onClick={() => onResetRuntime(provider.id)}
                  className="text-accent transition-colors hover:underline"
                >
                  {t("providers.runtime_reset")}
                </button>
              )}
          </div>
          {cooldownSummary && (
            <div
              className="truncate"
              title={cooldownSummary.reason ?? undefined}
            >
              {t("providers.runtime_recovers_at")}{" "}
              {new Date(cooldownSummary.recoverAt).toLocaleTimeString([], {
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
              })}
              {cooldownSummary.reason
                ? ` · ${t("providers.runtime_reason")} ${cooldownSummary.reason}`
                : ""}
            </div>
          )}
          {runtime?.consecutive_failures ? (
            <div>
              {t("providers.runtime_failures")} {runtime.consecutive_failures}
            </div>
          ) : null}
          {signalSummary.probe.error && (
            <div className="truncate" title={signalSummary.probe.error}>
              {t("providers.probe")}: {signalSummary.probe.error}
            </div>
          )}
          {runtime?.last_error && (
            <div
              className="truncate"
              title={`${runtime.last_error_code ?? ""} ${runtime.last_error}`.trim()}
            >
              {t("providers.runtime_reason")}:{" "}
              {runtime.last_error_code ?? runtime.last_error}
            </div>
          )}
          {latestError && (
            <div
              className="truncate text-[11px] text-text-muted"
              title={`${latestError.status_code} ${latestError.message}`}
            >
              {t("providers.health_recent_errors")}: {latestError.status_code} ·{" "}
              {latestError.message}
            </div>
          )}
        </div>
      )}

      {/* ── Collapsible details ── */}
      {showDetails && (
        <div className="mb-3 grid grid-cols-2 gap-y-2 rounded-md bg-card-secondary/40 p-3 text-xs">
          <div>
            <span className="text-text-muted">{t("providers.type")}</span>
            <p className="text-text-primary">{provider.provider_type}</p>
          </div>
          <div>
            <span className="text-text-muted">{t("providers.protocol")}</span>
            <p className="flex flex-wrap gap-1">
              {protocolLabels.map((p) => (
                <span
                  key={p}
                  className="rounded bg-card-secondary px-1.5 py-0.5 text-[11px] text-text-primary"
                >
                  {p}
                </span>
              ))}
            </p>
          </div>
          {provider.reasoning_model && (
            <div className="col-span-2">
              <span className="text-text-muted">
                {t("providers.reasoning_model")}
              </span>
              <p className="font-mono text-text-primary">
                {provider.reasoning_model}
              </p>
            </div>
          )}
        </div>
      )}

      {/* ── Actions ── */}
      <div className="flex items-center justify-between">
        <div className="flex flex-wrap gap-2">
          <button
            onClick={() => onEdit(provider)}
            className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
          >
            <Pencil className="h-3 w-3" />
            {t("common.edit")}
          </button>
          <button
            onClick={() => onTest(provider)}
            disabled={testing}
            className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-50"
          >
            {testing ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <ExternalLink className="h-3 w-3" />
            )}
            {t("providers.test")}
          </button>
          {onDetails && (
            <button
              onClick={() => onDetails(provider)}
              className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
            >
              <ExternalLink className="h-3 w-3" />
              {t("common.details")}
            </button>
          )}
          {!provider.is_active && (
            <button
              onClick={() => onSetActive(provider)}
              className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
            >
              <Star className="h-3 w-3" />
              {t("providers.set_active")}
            </button>
          )}
          <button
            onClick={() => onDelete(provider)}
            className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-error/20 hover:text-error"
          >
            <Trash2 className="h-3 w-3" />
            {t("common.delete")}
          </button>
        </div>
        <button
          onClick={() => setShowDetails((v) => !v)}
          className="flex items-center gap-1 text-[11px] text-text-muted transition-colors hover:text-text-primary"
        >
          {showDetails ? (
            <ChevronUp className="h-3 w-3" />
          ) : (
            <ChevronDown className="h-3 w-3" />
          )}
          {t("providers.details")}
        </button>
      </div>
    </div>
  );
}
