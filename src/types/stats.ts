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
}

export interface ProviderStat {
  name: string;
  count: number;
}
