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
  is_agentgate_active: boolean;
  openai_key_polluted: boolean;
  has_saved_official: boolean;
}

export interface ToggleResult {
  success: boolean;
  new_provider: string;
  config_path: string;
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

export interface OpenCodeConfigStatus {
  config_path: string;
  exists: boolean;
  has_agentgate: boolean;
  current_model: string | null;
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
  has_saved_official: boolean;
}

export interface ProfileDetection {
  path: string;
  exists: boolean;
  has_anthropic_vars: boolean;
  var_count: number;
}

export interface GeminiCliConfigStatus {
  config_path: string;
  exists: boolean;
  has_agentgate: boolean;
  current_model: string | null;
  has_saved_official: boolean;
}

export interface AtomCodeConfigStatus {
  config_path: string;
  exists: boolean;
  has_agentgate: boolean;
  current_model: string | null;
  has_saved_official: boolean;
}

