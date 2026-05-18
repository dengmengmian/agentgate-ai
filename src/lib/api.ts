import { invoke } from "@tauri-apps/api/core";
import type {
  ProviderView,
  CreateProviderInput,
  UpdateProviderInput,
  ProviderTestResult,
} from "@/types/provider";
import type {
  GatewayStatus,
  GatewaySettings,
  UpdateGatewaySettingsInput,
} from "@/types/gateway";
import type {
  RequestLogListItem,
  RequestLogDetail,
  RequestLogFilter,
} from "@/types/request-log";
import type { ToolConfigView } from "@/types/tool";

// ── Error handling ─────────────────────────────────────────────

export interface AppError {
  code: string;
  message: string;
  detail?: string;
  suggestion?: string;
}

function extractError(err: unknown): AppError {
  if (typeof err === "object" && err !== null && "message" in err) {
    return err as AppError;
  }
  if (typeof err === "string") {
    return { code: "UNKNOWN", message: err };
  }
  return { code: "UNKNOWN", message: "An unexpected error occurred" };
}

async function cmd<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    throw extractError(err);
  }
}

// ── Providers ──────────────────────────────────────────────────

export async function listProviders(): Promise<ProviderView[]> {
  return cmd("list_providers");
}

export async function getProvider(id: string): Promise<ProviderView> {
  return cmd("get_provider", { id });
}

export async function createProvider(input: CreateProviderInput): Promise<ProviderView> {
  return cmd("create_provider", { input });
}

export async function updateProvider(
  id: string,
  input: UpdateProviderInput
): Promise<ProviderView> {
  return cmd("update_provider", { id, input });
}

export async function deleteProvider(id: string): Promise<boolean> {
  return cmd("delete_provider", { id });
}

export async function setActiveProvider(id: string): Promise<ProviderView> {
  return cmd("set_active_provider", { id });
}

export async function fetchProviderModels(id: string): Promise<string[]> {
  return cmd("fetch_provider_models", { id });
}

export async function testProvider(id: string): Promise<ProviderTestResult> {
  return cmd("test_provider", { id });
}

export async function detectProviderVision(id: string): Promise<ProviderTestResult> {
  return cmd("detect_provider_vision", { id });
}

export async function detectProviderCache(id: string): Promise<ProviderTestResult> {
  return cmd("detect_provider_cache", { id });
}

// ── Gateway ────────────────────────────────────────────────────

export async function getGatewayStatus(): Promise<GatewayStatus> {
  return cmd("get_gateway_status");
}

export async function getGatewaySettings(): Promise<GatewaySettings> {
  return cmd("get_gateway_settings");
}

export async function updateGatewaySettings(
  input: UpdateGatewaySettingsInput
): Promise<GatewaySettings> {
  return cmd("update_gateway_settings", { input });
}

export async function startGateway(): Promise<GatewayStatus> {
  return cmd("start_gateway");
}

export async function stopGateway(): Promise<GatewayStatus> {
  return cmd("stop_gateway");
}

export async function restartGateway(): Promise<GatewayStatus> {
  return cmd("restart_gateway");
}

// ── Logs ───────────────────────────────────────────────────────

export async function listRequestLogs(
  filter: RequestLogFilter
): Promise<RequestLogListItem[]> {
  return cmd("list_request_logs", { filter });
}

export async function getRequestLogDetail(
  id: string
): Promise<RequestLogDetail> {
  return cmd("get_request_log_detail", { id });
}

export async function clearRequestLogs(): Promise<boolean> {
  return cmd("clear_request_logs");
}

// ── Tools ──────────────────────────────────────────────────────

export async function listTools(): Promise<ToolConfigView[]> {
  return cmd("list_tools");
}

export async function generateCodexConfig(): Promise<string> {
  return cmd("generate_codex_config");
}

// ── Gateway Auth ───────────────────────────────────────────────

export async function getGatewayAuthSettings(): Promise<GatewayAuthSettings> {
  return cmd("get_gateway_auth_settings");
}

export async function regenerateLocalAccessToken(): Promise<GatewayAuthSettings> {
  return cmd("regenerate_local_access_token");
}

export async function getLocalAccessToken(): Promise<string> {
  return cmd("get_local_access_token");
}

export async function ensureLocalAccessToken(): Promise<GatewayAuthSettings> {
  return cmd("ensure_local_access_token");
}

export async function openTokenFolder(): Promise<boolean> {
  return cmd("open_token_folder");
}

// ── Codex Config ───────────────────────────────────────────────

import type {
  GatewayAuthSettings,
  CodexConfigStatus,
  ApplyConfigResult,
  ClaudeCodeEnvStatus,
  ToggleResult,
} from "@/types/config";

export async function detectCodexConfig(): Promise<CodexConfigStatus> {
  return cmd("detect_codex_config");
}

export async function applyCodexConfig(): Promise<ApplyConfigResult> {
  return cmd("apply_codex_config");
}

export async function toggleCodexProvider(): Promise<ToggleResult> {
  return cmd("toggle_codex_provider");
}

export async function openCodexConfig(): Promise<boolean> {
  return cmd("open_codex_config");
}

// ── Claude Code ────────────────────────────────────────────────

export async function detectClaudeCodeEnv(): Promise<ClaudeCodeEnvStatus> {
  return cmd("detect_claude_code_env");
}



export async function applyClaudeCodeConfig(): Promise<ApplyConfigResult> {
  return cmd("apply_claude_code_config");
}

export async function toggleClaudeCodeProvider(): Promise<ToggleResult> {
  return cmd("toggle_claude_code_provider");
}

export async function openClaudeCodeConfig(): Promise<boolean> {
  return cmd("open_claude_code_config");
}

export async function generateClaudeCodeEnv(): Promise<string> {
  return cmd("generate_claude_code_env");
}

// ── OpenCode Config ───────────────────────────────────────────

import type { OpenCodeConfigStatus, GeminiCliConfigStatus, AtomCodeConfigStatus } from "@/types/config";

export async function detectOpenCodeConfig(): Promise<OpenCodeConfigStatus> {
  return cmd("detect_opencode_config");
}

export async function applyOpenCodeConfig(): Promise<ApplyConfigResult> {
  return cmd("apply_opencode_config");
}

export async function generateOpenCodeConfig(): Promise<string> {
  return cmd("generate_opencode_config");
}

export async function openOpenCodeConfig(): Promise<boolean> {
  return cmd("open_opencode_config");
}

// ── Gemini CLI ────────────────────────────────────────────────

export async function detectGeminiConfig(): Promise<GeminiCliConfigStatus> {
  return cmd("detect_gemini_config");
}

export async function applyGeminiConfig(): Promise<ApplyConfigResult> {
  return cmd("apply_gemini_config");
}

export async function generateGeminiConfig(): Promise<string> {
  return cmd("generate_gemini_config");
}

export async function toggleGeminiProvider(): Promise<ToggleResult> {
  return cmd("toggle_gemini_provider");
}

export async function openGeminiConfig(): Promise<boolean> {
  return cmd("open_gemini_config");
}

// ── AtomCode ──────────────────────────────────────────────────

export async function detectAtomCodeConfig(): Promise<AtomCodeConfigStatus> {
  return cmd("detect_atomcode_config");
}

export async function applyAtomCodeConfig(): Promise<ApplyConfigResult> {
  return cmd("apply_atomcode_config");
}

export async function generateAtomCodeConfig(): Promise<string> {
  return cmd("generate_atomcode_config");
}

export async function toggleAtomCodeProvider(): Promise<ToggleResult> {
  return cmd("toggle_atomcode_provider");
}

export async function openAtomCodeConfig(): Promise<boolean> {
  return cmd("open_atomcode_config");
}

// ── MCP ───────────────────────────────────────────────────────

import type { McpOverview } from "@/types/mcp";

export async function getMcpOverview(): Promise<McpOverview> {
  return cmd("get_mcp_overview");
}

export async function addMcpServer(client: string, name: string, command: string, args: string[], timeout?: number): Promise<boolean> {
  return cmd("add_mcp_server", { client, name, command, args, timeout });
}

export async function removeMcpServer(client: string, name: string): Promise<boolean> {
  return cmd("remove_mcp_server", { client, name });
}

export async function toggleMcpServer(client: string, name: string, enabled: boolean): Promise<boolean> {
  return cmd("toggle_mcp_server", { client, name, enabled });
}

// ── Route Profiles ─────────────────────────────────────────────

import type {
  RouteProfileView,
  RouteProfileDetail,
  CreateRouteProfileInput,
  UpdateRouteProfileInput,
  AddProviderToRouteInput,
  ProviderRuntimeStatus,
} from "@/types/route-profile";

export async function listRouteProfiles(): Promise<RouteProfileView[]> {
  return cmd("list_route_profiles");
}

export async function getRouteProfile(id: string): Promise<RouteProfileDetail> {
  return cmd("get_route_profile", { id });
}

export async function createRouteProfile(input: CreateRouteProfileInput): Promise<RouteProfileView> {
  return cmd("create_route_profile", { input });
}

export async function updateRouteProfile(id: string, input: UpdateRouteProfileInput): Promise<RouteProfileView> {
  return cmd("update_route_profile", { id, input });
}

export async function deleteRouteProfile(id: string): Promise<boolean> {
  return cmd("delete_route_profile", { id });
}

export async function setDefaultRouteProfile(id: string): Promise<boolean> {
  return cmd("set_default_route_profile", { id });
}

export async function setRouteProfileMode(id: string, mode: string): Promise<boolean> {
  return cmd("set_route_profile_mode", { id, mode });
}

export async function setRouteActiveProvider(routeProfileId: string, providerId: string): Promise<boolean> {
  return cmd("set_route_active_provider", { routeProfileId, providerId });
}

export async function addProviderToRoute(routeProfileId: string, providerId: string, input: AddProviderToRouteInput): Promise<boolean> {
  return cmd("add_provider_to_route", { routeProfileId, providerId, input });
}

export async function removeProviderFromRoute(routeProfileId: string, providerId: string): Promise<boolean> {
  return cmd("remove_provider_from_route", { routeProfileId, providerId });
}

export async function reorderRouteProviders(routeProfileId: string, providerIds: string[]): Promise<boolean> {
  return cmd("reorder_route_providers", { routeProfileId, providerIds });
}

export async function updateRouteProviderConditions(routeProfileId: string, providerId: string, routingConditions: string | null): Promise<boolean> {
  return cmd("update_route_provider_conditions", { routeProfileId, providerId, routingConditions });
}

export async function listProviderRuntimeStatus(): Promise<ProviderRuntimeStatus[]> {
  return cmd("list_provider_runtime_status");
}

export async function resetProviderRuntimeStatus(providerId: string): Promise<ProviderRuntimeStatus> {
  return cmd("reset_provider_runtime_status", { providerId });
}

export async function resetAllProviderRuntimeStatus(): Promise<boolean> {
  return cmd("reset_all_provider_runtime_status");
}

// ── Stats ──────────────────────────────────────────────────────

import type { RequestStats, ModelPricing, ProviderHealth } from "@/types/stats";

export async function getRequestStats(): Promise<RequestStats> {
  return cmd("get_request_stats");
}

export async function getProviderHealth(provider: string): Promise<ProviderHealth> {
  return cmd("get_provider_health", { provider });
}

// ── Pricing ───────────────────────────────────────────────────

export async function listModelPricing(): Promise<ModelPricing[]> {
  return cmd("list_model_pricing");
}

export async function upsertModelPricing(provider: string, model_pattern: string, input_price: number, output_price: number): Promise<ModelPricing> {
  return cmd("upsert_model_pricing", { provider, modelPattern: model_pattern, inputPrice: input_price, outputPrice: output_price });
}

export async function deleteModelPricing(id: string): Promise<boolean> {
  return cmd("delete_model_pricing", { id });
}

// ── Diagnostics ────────────────────────────────────────────────

import type { CheckReport, FullSelfTestReport, ExportResult } from "@/types/diagnostics";

export async function runHealthCheck(): Promise<CheckReport> {
  return cmd("run_health_check");
}

export async function runDatabaseCheck(): Promise<CheckReport> {
  return cmd("run_database_check");
}

export async function runGatewayAuthCheck(): Promise<CheckReport> {
  return cmd("run_gateway_auth_check");
}

export async function runProviderCheck(): Promise<CheckReport> {
  return cmd("run_provider_check");
}

export async function runCodexConfigCheck(): Promise<CheckReport> {
  return cmd("run_codex_config_check");
}

export async function runClaudeCodeConfigCheck(): Promise<CheckReport> {
  return cmd("run_claude_code_config_check");
}

export async function runRouteProfileCheck(): Promise<CheckReport> {
  return cmd("run_route_profile_check");
}

export async function runFullSelfTest(): Promise<FullSelfTestReport> {
  return cmd("run_full_self_test");
}

export async function exportDiagnosticBundle(includeLogs?: boolean, maxLogs?: number): Promise<ExportResult> {
  return cmd("export_diagnostic_bundle", { includeLogs, maxLogs });
}

export async function openAppDataDir(): Promise<boolean> {
  return cmd("open_app_data_dir");
}
