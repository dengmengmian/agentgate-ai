export interface RouteProfileView {
  id: string;
  name: string;
  input_protocol: string;
  mode: string;
  selection_strategy: string;   // "priority" | "cheapest" | "fastest"
  active_provider_id: string | null;
  active_provider_name: string | null;
  enabled: boolean;
  is_default: boolean;
  providers_count: number;
  created_at: string;
  updated_at: string;
}

export interface RouteProfileDetail {
  profile: RouteProfileView;
  providers: RouteProfileProviderView[];
}

export interface RouteProfileProviderView {
  id: string;
  provider_id: string;
  provider_name: string;
  provider_type: string;
  provider_protocol: string;
  has_anthropic_url: boolean;
  supports_vision: boolean | null;
  model_capabilities: string | null;
  priority: number;
  enabled: boolean;
  model_override: string | null;
  cooldown_seconds: number;
  failover_on_status_codes: string | null;
  failover_on_error_keywords: string | null;
  routing_conditions: string | null;
  runtime_available: boolean;
  cooldown_until: string | null;
  consecutive_failures: number;
}

export interface CreateRouteProfileInput {
  name: string;
  input_protocol: string;
  mode?: string;
}

export interface UpdateRouteProfileInput {
  name?: string;
  mode?: string;
  selection_strategy?: string;
  enabled?: boolean;
}

export interface AddProviderToRouteInput {
  priority?: number;
  model_override?: string;
  cooldown_seconds?: number;
  failover_on_status_codes?: string;
  failover_on_error_keywords?: string;
  routing_conditions?: string;
}

export interface RoutingConditions {
  min_input_chars?: number | null;
  max_input_chars?: number | null;
  has_images?: boolean | null;
  has_tools?: boolean | null;
  system_keywords?: string[] | null;
  model_override?: string | null;
}

export interface ProviderRuntimeStatus {
  provider_id: string;
  available: boolean;
  consecutive_failures: number;
  last_error: string | null;
  last_error_code: string | null;
  last_error_at: string | null;
  cooldown_until: string | null;
  quota_exhausted: boolean;
  // 主动健康探测结果（仅展示，不参与路由）
  last_probe_ok: boolean | null;
  last_probe_at: string | null;
  last_probe_latency_ms: number | null;
  last_probe_error: string | null;
  updated_at: string;
}
