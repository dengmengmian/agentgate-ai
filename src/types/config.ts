export interface GatewayAuthSettings {
  gateway_auth_enabled: boolean;
  auth_mode: string;
  token_path: string;
  masked_token: string;
  codex_auth_type: string;
  claude_code_auth_type: string;
}

export interface CodexConfigStatus {
  config_path: string;
  auth_json_path: string;
  exists: boolean;
  auth_json_exists: boolean;
  has_agentgate: boolean;
  has_agentgate_auth: boolean;
  current_provider: string | null;
  current_model: string | null;
  auth_mode: string;
  token_path: string;
}

export interface ConfigPreview {
  config_path: string;
  auth_json_path: string;
  exists: boolean;
  current_summary: string | null;
  proposed_snippet: string;
  proposed_auth_json: string;
  warnings: string[];
  auth_mode: string;
  token_path: string;
}

export interface ApplyConfigResult {
  success: boolean;
  config_path: string;
  auth_json_path?: string;
  backup_path: string | null;
  auth_backup_path?: string | null;
  token_path?: string;
  changed_keys: string[];
  warnings: string[];
}

export interface ConfigBackup {
  id: string;
  tool_type: string;
  source_path: string;
  backup_path: string;
  backup_kind: string;
  created_at: string;
  metadata_json: string | null;
}

export interface ClaudeCodeEnvStatus {
  settings_path: string;
  settings_exists: boolean;
  current_env: Record<string, string>;
  detected_profiles: ProfileDetection[];
  conflicts: string[];
  active_base_url: string | null;
  active_model: string | null;
  has_api_key: boolean;
  has_auth_token: boolean;
  has_agentgate: boolean;
  auth_mode: string;
  recommendations: string[];
}

export interface ProfileDetection {
  path: string;
  exists: boolean;
  has_anthropic_vars: boolean;
  var_count: number;
}

export interface ClaudeCodeConfigPreview {
  config_path: string;
  exists: boolean;
  current_summary: string | null;
  proposed_env: Record<string, string>;
  warnings: string[];
  conflicts: string[];
  auth_mode: string;
  masked_local_token: string;
}
