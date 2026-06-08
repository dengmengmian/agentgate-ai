// 从 bindings re-export。RequestLogFilter 用 Partial 兼容旧 partial 构造。
import type {
  RequestLogListItem,
  RequestLogDetail,
  RequestLogFilter as Wide,
  SessionUsageSummary,
  CostBreakdown,
  ConversationMessage,
  ProviderLatencyPoint,
  ProviderModelStats,
  ProviderDetailStats,
} from "@/lib/bindings";

export type {
  RequestLogListItem,
  RequestLogDetail,
  SessionUsageSummary,
  CostBreakdown,
  ConversationMessage,
  ProviderLatencyPoint,
  ProviderModelStats,
  ProviderDetailStats,
};

export type RequestLogFilter = {
  [K in keyof Wide]?: Wide[K] | undefined;
};
