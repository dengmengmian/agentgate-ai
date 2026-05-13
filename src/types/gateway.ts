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
}
