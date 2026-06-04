import type { RecentError } from "@/types/stats";

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
