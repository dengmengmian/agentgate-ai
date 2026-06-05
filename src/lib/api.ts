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

/// Plain-text api keys in storage order. Used by the edit form so each key
/// slot can be repopulated; the masked view alone hides which key is which.
export async function getProviderKeys(id: string): Promise<string[]> {
  return cmd("get_provider_keys", { id });
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

export async function providerSpeedtest(id: string): Promise<import("@/types/provider").ProviderSpeedReport> {
  return cmd("provider_speedtest", { id });
}

export async function providerSpeedtestAll(): Promise<import("@/types/provider").ProviderSpeedReport[]> {
  return cmd("provider_speedtest_all", {});
}

export async function detectProviderVision(id: string): Promise<ProviderTestResult> {
  return cmd("detect_provider_vision", { id });
}

export async function detectProviderCache(id: string): Promise<ProviderTestResult> {
  return cmd("detect_provider_cache", { id });
}

export async function seedModelCapabilities(
  providerType: string,
  modelIds: string[],
): Promise<Record<string, string[]>> {
  return cmd("seed_model_capabilities", { providerType, modelIds });
}

/// Fill missing rows in the provider's model_capabilities matrix from the seed
/// function (provider_type + model id pattern). Preserves manually-edited rows.
/// Returns the number of rows added.
export async function autofillProviderCapabilities(id: string): Promise<number> {
  return cmd("autofill_provider_capabilities", { id });
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

export async function listLogModels(): Promise<string[]> {
  return cmd("list_log_models");
}

export async function getSessionConversation(
  sessionId: string,
): Promise<import("@/types/request-log").ConversationMessage[]> {
  return cmd("get_session_conversation", { sessionId });
}

export async function deleteSession(sessionId: string): Promise<void> {
  return cmd("delete_session", { sessionId });
}

export async function countRequestLogs(filter: RequestLogFilter): Promise<number> {
  return cmd("count_request_logs", { filter });
}

export async function getRequestLogDetail(
  id: string
): Promise<RequestLogDetail> {
  return cmd("get_request_log_detail", { id });
}

export async function clearRequestLogs(): Promise<boolean> {
  return cmd("clear_request_logs");
}

export async function aggregateRequestLogsBySession(
  filter: RequestLogFilter,
  limit?: number,
): Promise<import("@/types/request-log").SessionUsageSummary[]> {
  return cmd("aggregate_request_logs_by_session", { filter, limit });
}

export async function aggregateCostByModel(
  days?: number,
  limit?: number,
): Promise<import("@/types/request-log").CostBreakdown[]> {
  return cmd("aggregate_cost_by_model", { days, limit });
}

export async function aggregateCostByClient(
  days?: number,
  limit?: number,
): Promise<import("@/types/request-log").CostBreakdown[]> {
  return cmd("aggregate_cost_by_client", { days, limit });
}

export async function aggregateProviderDetailStats(
  provider: string,
  days?: number,
  limit?: number,
): Promise<import("@/types/request-log").ProviderDetailStats> {
  return cmd("aggregate_provider_detail_stats", { provider, days, limit });
}

export async function aggregateRouteProfileStats(
  days?: number,
): Promise<import("@/types/route-profile").RouteProfileStats[]> {
  return cmd("aggregate_route_profile_stats", { days });
}

export interface SyncResult {
  files_scanned: number;
  imported: number;
  skipped: number;
  errors: string[];
}

export async function syncClaudeSessions(): Promise<SyncResult> {
  return cmd("sync_claude_sessions");
}

export async function syncCodexSessions(): Promise<SyncResult> {
  return cmd("sync_codex_sessions");
}

export async function syncGeminiSessions(): Promise<SyncResult> {
  return cmd("sync_gemini_sessions");
}

// ── Tools ──────────────────────────────────────────────────────

export async function listTools(): Promise<ToolConfigView[]> {
  return cmd("list_tools");
}

export async function generateCodexConfig(): Promise<string> {
  return cmd("generate_codex_config");
}

// ── Claude Desktop（接入 AgentGate 网关）──
export async function detectClaudeDesktop(): Promise<import("@/types/config").ClaudeDesktopStatus> {
  return cmd("detect_claude_desktop");
}

export async function previewClaudeDesktopProfile(): Promise<string> {
  return cmd("preview_claude_desktop_profile");
}

export async function applyClaudeDesktopConfig(): Promise<import("@/types/config").ClaudeDesktopApplyResult> {
  return cmd("apply_claude_desktop_config");
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

/** Switch Codex back to native mode (official ChatGPT path). Used to bring
 * IDE plugin entries back from their compat-mode grey state. */
export async function disableCodexAgentgate(): Promise<ApplyConfigResult> {
  return cmd("disable_codex_agentgate");
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

// ── Post-apply process detection ───────────────────────────────

export interface RunningProcess {
  pid: number;
  command: string;
}

/// Match `client_id` ∈ {codex, claude_code, opencode, gemini, atomcode}.
/// Returns empty list on Windows (no detection yet) — caller should treat
/// empty as "couldn't detect" rather than "definitely not running".
export async function detectClientRunning(clientId: string): Promise<RunningProcess[]> {
  return cmd("detect_client_running", { clientId });
}

export interface CodexRestartResult {
  supported: boolean;
  platform: string;
  was_running: boolean;
  killed: number;
  relaunched: boolean;
}

/// Restart Codex Desktop so the freshly-written config picks up. macOS only
/// for now; on other platforms returns `supported: false` and the UI hides
/// the button.
export async function restartCodexDesktop(): Promise<CodexRestartResult> {
  return cmd("restart_codex_desktop");
}

// ── Client apply history ───────────────────────────────────────

export interface ClientApplyHistoryEntry {
  id: string;
  client_id: string;
  /// 'apply' | 'disable' | 'toggle'
  action: string;
  snapshot_json: string;
  summary: string;
  is_initial: boolean;
  agentgate_version: string;
  /// RFC 3339
  created_at: string;
}

export async function listClientApplyHistory(
  clientId: string
): Promise<ClientApplyHistoryEntry[]> {
  return cmd("list_client_apply_history", { clientId });
}

/// 曾经 apply 过配置的客户端 id 列表（用于配置漂移判断）。
export async function clientsWithApplyHistory(): Promise<string[]> {
  return cmd("clients_with_apply_history");
}

export async function rollbackClientApply(
  historyId: string
): Promise<ClientApplyHistoryEntry> {
  return cmd("rollback_client_apply", { historyId });
}

// ── Global Instructions (CLAUDE.md / AGENTS.md) ────────────────

export type InstructionsScope = "claude_global" | "codex_global";
export type InstructionsApplyMode = "overwrite" | "append";

export interface InstructionsTemplate {
  id: string;
  title: string;
  description: string;
  /// `"claude"` / `"codex"` / `"all"`
  scopes: string[];
  content: string;
}

export interface InstructionsStatus {
  scope: string;
  path: string;
  exists: boolean;
  content: string;
  size_bytes: number;
}

export async function listInstructionsTemplates(): Promise<InstructionsTemplate[]> {
  return cmd("list_instructions_templates");
}

export async function readGlobalInstructions(
  scope: InstructionsScope
): Promise<InstructionsStatus> {
  return cmd("read_global_instructions", { scope });
}

export async function writeGlobalInstructions(
  scope: InstructionsScope,
  content: string
): Promise<InstructionsStatus> {
  return cmd("write_global_instructions", { scope, content });
}

export async function applyInstructionsTemplate(
  scope: InstructionsScope,
  templateId: string,
  mode: InstructionsApplyMode
): Promise<InstructionsStatus> {
  return cmd("apply_instructions_template", { scope, templateId, mode });
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

import type { RequestStats, ModelPricing, ProviderHealth, RuntimeKpis } from "@/types/stats";

export async function getRequestStats(): Promise<RequestStats> {
  return cmd("get_request_stats");
}

/** Stats over a sliding window — used by the Dashboard date-range tabs. */
export async function getRequestStatsRange(days: number): Promise<RequestStats> {
  return cmd("get_request_stats_range", { days });
}

/** Live gateway KPIs (active requests, uptime, today success rate). */
export async function getRuntimeKpis(): Promise<RuntimeKpis> {
  return cmd("get_runtime_kpis");
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

// ── Tool Connection Test ──────────────────────────────────────

export interface ConnectionTestResult {
  config_ok: boolean;
  gateway_ok: boolean;
  provider_ok: boolean;
  test_model?: string;
  error?: string;
}

export async function testToolConnection(): Promise<ConnectionTestResult> {
  return cmd("test_tool_connection");
}

// ── Pet ───────────────────────────────────────────────────────

import type { PetSettings, UpdatePetSettingsInput, PetGatewayInfo } from "@/types/pet";

export async function getPetSettings(): Promise<PetSettings> {
  return cmd("get_pet_settings");
}

export async function updatePetSettings(input: UpdatePetSettingsInput): Promise<PetSettings> {
  return cmd("update_pet_settings", { input });
}

export async function setPetVisible(visible: boolean): Promise<PetSettings> {
  return cmd("set_pet_visible", { visible });
}

export async function getPetGatewayState(): Promise<PetGatewayInfo> {
  return cmd("get_pet_gateway_state");
}

export async function getPetMemory(): Promise<string> {
  return cmd("get_pet_memory");
}

export async function savePetMemory(memory: string): Promise<boolean> {
  return cmd("save_pet_memory", { memory });
}

export async function petChat(messages: Array<{ role: string; content: string }>): Promise<string> {
  return cmd("pet_chat", { messages });
}

// ── Config Import / Export ─────────────────────────────────────

export interface ConfigImportSummary {
  providers_imported: number;
  route_profiles_imported: number;
  members_imported: number;
  secrets_applied: boolean;
}

export async function exportConfigJson(includeSecrets: boolean): Promise<string> {
  return cmd("export_config_json", { includeSecrets });
}

export async function importConfigJson(json: string): Promise<ConfigImportSummary> {
  return cmd("import_config_json", { json });
}
