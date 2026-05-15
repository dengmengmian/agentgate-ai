export interface RouteProfileView {
  id: string;
  name: string;
  input_protocol: string;
  mode: string;
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
  priority: number;
  enabled: boolean;
  model_override: string | null;
  cooldown_seconds: number;
  failover_on_status_codes: string | null;
  failover_on_error_keywords: string | null;
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
  enabled?: boolean;
}

export interface AddProviderToRouteInput {
  priority?: number;
  model_override?: string;
  cooldown_seconds?: number;
  failover_on_status_codes?: string;
  failover_on_error_keywords?: string;
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
  updated_at: string;
}
