import type { RecentError } from "@/types/stats";
import type { ProviderRuntimeStatus } from "@/types/route-profile";

export type ProviderErrorStatus =
  | "rate_limited"
  | "quota_exhausted"
  | "insufficient_balance"
  | "auth_failed"
  | "other_error";

const BALANCE_MARKERS = [
  "insufficient_balance",
  "insufficient balance",
  "credit_balance",
  "out of credits",
  "balance",
  "余额不足",
];

const QUOTA_MARKERS = [
  "insufficient_quota",
  "insufficient quota",
  "quota_exceeded",
  "quota exceeded",
  "exceeded your current quota",
  "quota",
];

const AUTH_MARKERS = [
  "invalid api key",
  "invalid_api_key",
  "authentication_error",
  "authentication failed",
  "unauthorized",
  "permission_denied",
];

export function classifyProviderErrorStatus(
  error: Pick<RecentError, "status_code" | "message">
): ProviderErrorStatus {
  const message = error.message.toLowerCase();

  if (BALANCE_MARKERS.some((marker) => message.includes(marker))) {
    return "insufficient_balance";
  }
  if (QUOTA_MARKERS.some((marker) => message.includes(marker))) {
    return "quota_exhausted";
  }
  if (
    error.status_code === 401 ||
    error.status_code === 403 ||
    AUTH_MARKERS.some((marker) => message.includes(marker))
  ) {
    return "auth_failed";
  }
  if (error.status_code === 429) {
    return "rate_limited";
  }

  return "other_error";
}

export function summarizeProviderErrorStatuses(
  errors: RecentError[]
): ProviderErrorStatus[] {
  const statuses = errors.map(classifyProviderErrorStatus);
  return Array.from(new Set(statuses)).filter(
    (status) => status !== "other_error"
  );
}

export interface ProviderCooldownSummary {
  remainingSeconds: number;
  recoverAt: string;
  reason: string | null;
}

export function getProviderCooldownSummary(
  runtime:
    | Pick<
        ProviderRuntimeStatus,
        "cooldown_until" | "last_error" | "last_error_code"
      >
    | null
    | undefined,
  nowMs = Date.now()
): ProviderCooldownSummary | null {
  if (!runtime?.cooldown_until) return null;

  const recoverAtMs = new Date(runtime.cooldown_until).getTime();
  if (!Number.isFinite(recoverAtMs)) return null;

  const remainingMs = recoverAtMs - nowMs;
  if (remainingMs <= 0) return null;

  return {
    remainingSeconds: Math.ceil(remainingMs / 1000),
    recoverAt: runtime.cooldown_until,
    reason: runtime.last_error_code ?? runtime.last_error,
  };
}

export type ProviderSignalVariant = "success" | "error" | "warning" | "muted";

export interface ProviderSignalSummary {
  runtime: {
    status:
      | "available"
      | "cooldown"
      | "quota_exhausted"
      | "unavailable"
      | "degraded"
      | "unknown";
    variant: ProviderSignalVariant;
  };
  probe: {
    status: "ok" | "failed" | "unknown";
    variant: ProviderSignalVariant;
    latencyMs: number | null;
    error: string | null;
  };
  traffic: {
    status: "healthy" | "degraded" | "unhealthy" | "no_traffic" | "unknown";
    variant: ProviderSignalVariant;
  };
}

type RuntimeSignalInput = Pick<
  ProviderRuntimeStatus,
  | "available"
  | "consecutive_failures"
  | "cooldown_until"
  | "last_error"
  | "last_error_code"
  | "quota_exhausted"
  | "last_probe_ok"
  | "last_probe_latency_ms"
  | "last_probe_error"
>;

interface TrafficSignalInput {
  h24_total: number;
  h24_success_rate: number;
}

export function getProviderSignalSummary(
  runtime: RuntimeSignalInput | null | undefined,
  traffic: TrafficSignalInput | null | undefined,
  nowMs = Date.now()
): ProviderSignalSummary {
  const cooldown = getProviderCooldownSummary(runtime, nowMs);
  const runtimeSignal: ProviderSignalSummary["runtime"] = !runtime
    ? { status: "unknown", variant: "muted" }
    : runtime.quota_exhausted
      ? { status: "quota_exhausted", variant: "error" }
      : cooldown
        ? { status: "cooldown", variant: "warning" }
        : !runtime.available
          ? { status: "unavailable", variant: "error" }
          : runtime.consecutive_failures > 0
            ? { status: "degraded", variant: "warning" }
            : { status: "available", variant: "success" };

  const probeSignal: ProviderSignalSummary["probe"] =
    runtime?.last_probe_ok == null
      ? { status: "unknown", variant: "muted", latencyMs: null, error: null }
      : runtime.last_probe_ok
        ? {
            status: "ok",
            variant: "success",
            latencyMs: runtime.last_probe_latency_ms,
            error: null,
          }
        : {
            status: "failed",
            variant: "error",
            latencyMs: runtime.last_probe_latency_ms,
            error: runtime.last_probe_error,
          };

  const trafficSignal: ProviderSignalSummary["traffic"] = !traffic
    ? { status: "unknown", variant: "muted" }
    : traffic.h24_total === 0
      ? { status: "no_traffic", variant: "muted" }
      : traffic.h24_success_rate >= 95
        ? { status: "healthy", variant: "success" }
        : traffic.h24_success_rate >= 80
          ? { status: "degraded", variant: "warning" }
          : { status: "unhealthy", variant: "error" };

  return {
    runtime: runtimeSignal,
    probe: probeSignal,
    traffic: trafficSignal,
  };
}
