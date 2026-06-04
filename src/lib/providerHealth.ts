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

export function classifyProviderErrorStatus(error: Pick<RecentError, "status_code" | "message">): ProviderErrorStatus {
  const message = error.message.toLowerCase();

  if (BALANCE_MARKERS.some((marker) => message.includes(marker))) {
    return "insufficient_balance";
  }
  if (QUOTA_MARKERS.some((marker) => message.includes(marker))) {
    return "quota_exhausted";
  }
  if (error.status_code === 401 || error.status_code === 403 || AUTH_MARKERS.some((marker) => message.includes(marker))) {
    return "auth_failed";
  }
  if (error.status_code === 429) {
    return "rate_limited";
  }

  return "other_error";
}

export function summarizeProviderErrorStatuses(errors: RecentError[]): ProviderErrorStatus[] {
  const statuses = errors.map(classifyProviderErrorStatus);
  return Array.from(new Set(statuses)).filter((status) => status !== "other_error");
}

export interface ProviderCooldownSummary {
  remainingSeconds: number;
  recoverAt: string;
  reason: string | null;
}

export function getProviderCooldownSummary(
  runtime: Pick<ProviderRuntimeStatus, "cooldown_until" | "last_error" | "last_error_code"> | null | undefined,
  nowMs = Date.now(),
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
