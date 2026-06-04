export interface GatewayStatus {
  running: boolean;
  host: string;
  port: number;
  active_provider: string | null;
  input_protocol: string;
  output_protocol: string;
  started_at: string | null;
}

export interface GatewaySettings {
  id: number;
  host: string;
  port: number;
  active_provider_id: string | null;
  input_protocol: string;
  output_protocol: string;
  auto_start: boolean;
  log_retention_days: number;
  /// 全局总闸：3 个 refiner 默认全关，开 = 全局开启（仍可被每个 provider 的 enabled 强制关闭）。
  body_filter_global: boolean;
  thinking_rectifier_global: boolean;
  error_mapper_global: boolean;
  /// 后台主动健康探测开关（默认关）
  health_probe_enabled: boolean;
  updated_at: string;
}

export interface UpdateGatewaySettingsInput {
  host?: string;
  port?: number;
  active_provider_id?: string;
  input_protocol?: string;
  output_protocol?: string;
  auto_start?: boolean;
  log_retention_days?: number;
  body_filter_global?: boolean;
  thinking_rectifier_global?: boolean;
  error_mapper_global?: boolean;
  health_probe_enabled?: boolean;
}
