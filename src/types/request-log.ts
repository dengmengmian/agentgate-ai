export interface RequestLogListItem {
  id: string;
  request_id: string;
  timestamp: string;
  client: string | null;
  provider: string | null;
  model: string | null;
  route: string | null;
  status_code: number | null;
  latency_ms: number | null;
  error_message: string | null;
  // 'gateway' / 'claude_session' / 'codex_session' / 'gemini_session'
  source: string | null;
  session_id: string | null;
}

export interface RequestLogDetail {
  id: string;
  request_id: string;
  timestamp: string;
  client: string | null;
  provider: string | null;
  model: string | null;
  route: string | null;
  status_code: number | null;
  latency_ms: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  cost: number | null;
  cache_write_tokens: number | null;
  cache_read_tokens: number | null;
  raw_request: string | null;
  converted_request: string | null;
  raw_response: string | null;
  converted_response: string | null;
  sse_events: string | null;
  tool_calls: string | null;
  error_message: string | null;
  trace_json: string | null;
  source: string | null;
  session_id: string | null;
  external_id: string | null;
}

export interface RequestLogFilter {
  client?: string;
  provider?: string;
  model?: string;
  route_profile_id?: string;
  status?: string;
  error_type?: string;
  keyword?: string;
  // 'gateway' / 'claude_session' / 'codex_session' / 'gemini_session' /
  // 'session_log'（聚合：所有非 gateway 来源）
  source?: string;
  session_id?: string;
  limit?: number;
  offset?: number;
}

/// 按 session 聚合的用量摘要——Logs 页「按会话分组」视图用。
export interface SessionUsageSummary {
  session_id: string;
  source: string;             // 'gateway' / 'claude_session' / 'codex_session' / 'gemini_session' / 'mixed'
  provider: string | null;
  model: string | null;
  first_seen: string;
  last_seen: string;
  request_count: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  cost: number;
}

// 成本仪表盘：按模型 / 按客户端聚合的成本分解。
export interface CostBreakdown {
  key: string;                // 模型名 或 客户端名
  provider: string | null;
  request_count: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  cost: number;
  has_price: boolean;   // 该模型价格表里有没有价；false 时 UI 标"无价格"而非误导的 $0
}

export interface ProviderLatencyPoint {
  timestamp: string;
  model: string | null;
  latency_ms: number;
  status_code: number | null;
}

export interface ProviderModelStats {
  model: string;
  request_count: number;
  success_count: number;
  error_count: number;
  success_rate: number;
  avg_latency_ms: number;
  cost: number;
}

export interface ProviderDetailStats {
  provider: string;
  latency_points: ProviderLatencyPoint[];
  model_stats: ProviderModelStats[];
}
