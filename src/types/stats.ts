export interface RequestStats {
  total: number;
  success: number;
  errors: number;
  success_rate: number;
  avg_latency_ms: number;
  today_total: number;
  today_errors: number;
  total_input_tokens: number;
  total_output_tokens: number;
  today_input_tokens: number;
  today_output_tokens: number;
  total_cost: number;
  today_cost: number;
  total_cache_write_tokens: number;
  total_cache_read_tokens: number;
  today_cache_write_tokens: number;
  today_cache_read_tokens: number;
  daily: DailyStat[];
  providers: ProviderStat[];
}

export interface DailyStat {
  date: string;
  total: number;
  errors: number;
  success: number;
  input_tokens: number;
  output_tokens: number;
  cost: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
}

export interface RuntimeKpis {
  active_requests: number;
  uptime_seconds: number;
  gateway_running: boolean;
  gateway_port: number;
  success_rate_today: number;
  total_today: number;
}

export interface ProviderStat {
  name: string;
  count: number;
}

export interface ProviderHealth {
  provider: string;
  h1_total: number;
  h1_success: number;
  h1_success_rate: number;
  h1_avg_latency_ms: number;
  h1_p95_latency_ms: number;
  h24_total: number;
  h24_success: number;
  h24_success_rate: number;
  h24_avg_latency_ms: number;
  recent_errors: RecentError[];
}

export interface RecentError {
  timestamp: string;
  status_code: number;
  message: string;
}

export interface ModelPricing {
  id: string;
  provider: string;
  model_pattern: string;
  input_price: number;
  output_price: number;
  is_custom: boolean;
  updated_at: string;
}
