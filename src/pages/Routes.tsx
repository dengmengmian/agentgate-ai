import { useState, useEffect, useCallback, useRef } from "react";
import {
  Plus,
  Star,
  ChevronUp,
  ChevronDown,
  Trash2,
  RotateCcw,
  Shield,
  Zap,
  Inbox,
  X,
  Pencil,
  Check,
  Filter,
  SlidersHorizontal,
  Image as ImageIcon,
  Brain,
  FileText,
  Wrench,
  GitBranch,
  Route,
  type LucideIcon,
} from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { CapabilityIcons } from "@/components/common/CapabilityIcons";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import { usePolling } from "@/lib/usePolling";
import * as api from "@/lib/api";
import type { RouteProfileView, RouteProfileDetail, RoutingConditions } from "@/types/route-profile";
import type { ProviderView } from "@/types/provider";

const PROTOCOL_LABELS: Record<string, string> = {
  openai_responses: "OpenAI Responses (Codex)",
  anthropic_messages: "Anthropic Messages (Claude Code)",
  openai_chat_completions: "Chat Completions (OpenCode)",
};

function protocolLabel(proto: string): string {
  return PROTOCOL_LABELS[proto] ?? proto;
}

export function Routes() {
  const { t } = useI18n();
  const [profiles, setProfiles] = useState<RouteProfileView[]>([]);
  const [detail, setDetail] = useState<RouteProfileDetail | null>(null);
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [loading, setLoading] = useState(true);
  const [deleteTarget, setDeleteTarget] = useState<RouteProfileView | null>(null);
  const selectedIdRef = useRef<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newProtocol, setNewProtocol] = useState("openai_responses");
  const [editingName, setEditingName] = useState(false);
  const [editName, setEditName] = useState("");
  const [addProviderId, setAddProviderId] = useState("");
  const [conditionsTarget, setConditionsTarget] = useState<{ profileId: string; providerId: string; providerName: string; inputProtocol: string; current: RoutingConditions } | null>(null);

  const load = useCallback(async () => {
    try {
      const [p, prov] = await Promise.all([
        api.listRouteProfiles(),
        api.listProviders(),
      ]);
      setProfiles(p);
      setProviders(prov);
      if (p.length > 0) {
        const currentId = selectedIdRef.current;
        const toLoad = currentId && p.find((x) => x.id === currentId) ? currentId : p[0].id;
        const d = await api.getRouteProfile(toLoad);
        selectedIdRef.current = toLoad;
        setDetail(d);
      }
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);
  // 周期 + focus 刷新——让后台 cooldown 变化、新加的 provider 立即可见
  usePolling(load);

  const selectProfile = async (id: string) => {
    try {
      const d = await api.getRouteProfile(id);
      selectedIdRef.current = id;
      setDetail(d);
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleSetDefault = async (id: string) => {
    try { await api.setDefaultRouteProfile(id); toast("success", t("routes.default_updated")); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleToggleMode = async () => {
    if (!detail) return;
    const newMode = detail.profile.mode === "manual" ? "failover" : "manual";
    try { await api.setRouteProfileMode(detail.profile.id, newMode); toast("success", `${t("routes.mode_changed")} ${newMode}`); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleStrategyChange = async (strategy: string) => {
    if (!detail) return;
    try { await api.updateRouteProfile(detail.profile.id, { selection_strategy: strategy }); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleSetActive = async (providerId: string) => {
    if (!detail) return;
    try { await api.setRouteActiveProvider(detail.profile.id, providerId); toast("success", t("routes.active_updated")); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleAddProvider = async (providerId: string) => {
    if (!detail) return;
    try { await api.addProviderToRoute(detail.profile.id, providerId, {}); toast("success", t("routes.provider_added")); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleRemoveProvider = async (providerId: string) => {
    if (!detail) return;
    try { await api.removeProviderFromRoute(detail.profile.id, providerId); toast("success", t("routes.provider_removed")); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleReorder = async (providerId: string, direction: "up" | "down") => {
    if (!detail) return;
    const ids = detail.providers.map((p) => p.provider_id);
    const idx = ids.indexOf(providerId);
    if (idx < 0) return;
    const swapIdx = direction === "up" ? idx - 1 : idx + 1;
    if (swapIdx < 0 || swapIdx >= ids.length) return;
    [ids[idx], ids[swapIdx]] = [ids[swapIdx], ids[idx]];
    try { await api.reorderRouteProviders(detail.profile.id, ids); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleResetCooldown = async (providerId: string) => {
    try { await api.resetProviderRuntimeStatus(providerId); toast("success", t("routes.cooldown_reset")); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    try { await api.deleteRouteProfile(deleteTarget.id); toast("success", t("routes.deleted")); setDeleteTarget(null); selectedIdRef.current = null; setDetail(null); load(); }
    catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleRename = async () => {
    if (!detail || !editName.trim()) return;
    try {
      await api.updateRouteProfile(detail.profile.id, { name: editName.trim() });
      setEditingName(false);
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleCreate = async () => {
    if (!newName.trim()) return;
    try {
      await api.createRouteProfile({ name: newName.trim(), input_protocol: newProtocol });
      toast("success", t("routes.created"));
      setShowCreate(false);
      setNewName("");
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const availableProviders = detail
    ? providers.filter((p) => !detail.providers.some((rp) => rp.provider_id === p.id))
    : [];

  if (loading) return <p className="text-xs text-text-muted">{t("common.loading")}</p>;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <p className="text-xs text-text-muted">
          {profiles.length} {t("routes.route_profiles")}
        </p>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent/90"
        >
          <Plus className="h-3 w-3" />{t("routes.create_profile")}
        </button>
      </div>

      {/* Create form */}
      {showCreate && (
        <div className="rounded-xl border border-accent/30 bg-card p-4">
          <div className="mb-3 flex items-center justify-between">
            <h4 className="text-xs font-semibold text-text-primary">{t("routes.create_profile")}</h4>
            <button onClick={() => setShowCreate(false)} className="text-text-muted hover:text-text-primary"><X className="h-3.5 w-3.5" /></button>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="mb-1 block text-[11px] text-text-muted">{t("routes.profile_name")}</label>
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="My Route"
                className="form-input w-full"
              />
            </div>
            <div>
              <label className="mb-1 block text-[11px] text-text-muted">{t("routes.protocol")}</label>
              <select
                value={newProtocol}
                onChange={(e) => setNewProtocol(e.target.value)}
                className="form-input w-full"
              >
                {Object.entries(PROTOCOL_LABELS).map(([val, label]) => (
                  <option key={val} value={val}>{label}</option>
                ))}
              </select>
            </div>
          </div>
          <div className="mt-3 flex justify-end gap-2">
            <button onClick={() => setShowCreate(false)} className="btn-secondary">{t("common.cancel")}</button>
            <button onClick={handleCreate} disabled={!newName.trim()} className="btn-primary">{t("common.save")}</button>
          </div>
        </div>
      )}

      {profiles.length === 0 ? (
        <EmptyState icon={Inbox} title={t("routes.no_profiles")} description={t("routes.auto_created")} />
      ) : (
        <div className="flex gap-6">
          {/* Profile selector (left) */}
          <div className="w-64 shrink-0 space-y-2">
            {profiles.map((p) => (
              <button
                key={p.id}
                onClick={() => selectProfile(p.id)}
                className={`w-full rounded-lg border p-3 text-left transition-colors ${
                  detail?.profile.id === p.id
                    ? "border-accent/40 bg-card"
                    : "border-border bg-card hover:bg-hover"
                }`}
              >
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium text-text-primary">{p.name}</span>
                  <StatusBadge variant={p.mode === "failover" ? "accent" : "muted"}>
                    {p.mode === "failover" ? t("routes.mode_failover") : t("routes.mode_manual")}
                  </StatusBadge>
                </div>
                <p className="mt-1 text-[11px] text-text-muted">
                  {protocolLabel(p.input_protocol)} · {p.providers_count} provider{p.providers_count !== 1 ? "s" : ""}
                </p>
              </button>
            ))}
          </div>

          {/* Profile detail (right) */}
          {detail && (
            <div className="flex-1 space-y-4">
              {/* Header */}
              <div className="rounded-xl border border-border bg-card p-5">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    {editingName ? (
                      <div className="flex items-center gap-2">
                        <input
                          type="text"
                          value={editName}
                          onChange={(e) => setEditName(e.target.value)}
                          onKeyDown={(e) => e.key === "Enter" && handleRename()}
                          className="form-input text-sm font-semibold"
                          autoFocus
                        />
                        <button onClick={handleRename} className="rounded p-1 text-accent hover:bg-accent-soft"><Check className="h-3.5 w-3.5" /></button>
                        <button onClick={() => setEditingName(false)} className="rounded p-1 text-text-muted hover:bg-border"><X className="h-3.5 w-3.5" /></button>
                      </div>
                    ) : (
                      <div className="flex items-center gap-2">
                        <h3 className="text-sm font-semibold text-text-primary">{detail.profile.name}</h3>
                        <button
                          onClick={() => { setEditName(detail.profile.name); setEditingName(true); }}
                          className="rounded p-1 text-text-muted hover:bg-border hover:text-text-primary"
                        >
                          <Pencil className="h-3 w-3" />
                        </button>
                      </div>
                    )}
                    <p className="text-[11px] text-text-muted">
                      {protocolLabel(detail.profile.input_protocol)}
                    </p>
                  </div>
                  <div className="flex flex-wrap items-center justify-end gap-2">
                    <div className="flex rounded-md border border-border bg-card-secondary p-0.5">
                      <button
                        onClick={() => detail.profile.mode !== "manual" && handleToggleMode()}
                        className={`flex items-center gap-1.5 rounded px-2.5 py-1 text-[11px] font-medium transition-colors ${
                          detail.profile.mode === "manual"
                            ? "bg-card text-text-primary shadow-sm"
                            : "text-text-muted hover:text-text-primary"
                        }`}
                      >
                        <Shield className="h-3 w-3" />{t("routes.mode_manual")}
                      </button>
                      <button
                        onClick={() => detail.profile.mode !== "failover" && handleToggleMode()}
                        className={`flex items-center gap-1.5 rounded px-2.5 py-1 text-[11px] font-medium transition-colors ${
                          detail.profile.mode === "failover"
                            ? "bg-card text-accent shadow-sm"
                            : "text-text-muted hover:text-text-primary"
                        }`}
                      >
                        <Zap className="h-3 w-3" />{t("routes.mode_failover")}
                      </button>
                    </div>
                    {detail.profile.mode === "failover" && (
                      <label className="flex items-center gap-1.5 rounded-md border border-border bg-card-secondary px-2.5 py-1 text-[11px] text-text-secondary">
                        {t("routes.strategy")}
                        <select
                          value={detail.profile.selection_strategy}
                          onChange={(e) => handleStrategyChange(e.target.value)}
                          className="bg-transparent text-text-primary outline-none"
                          title={t("routes.strategy_hint")}
                        >
                          <option value="priority">{t("routes.strategy_priority")}</option>
                          <option value="cheapest">{t("routes.strategy_cheapest")}</option>
                          <option value="fastest">{t("routes.strategy_fastest")}</option>
                        </select>
                      </label>
                    )}
                    {!detail.profile.is_default && profiles.filter(p => p.input_protocol === detail.profile.input_protocol).length > 1 && (
                      <button
                        onClick={() => handleSetDefault(detail.profile.id)}
                        className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
                      >
                        <Star className="h-3 w-3" />{t("routes.set_default")}
                      </button>
                    )}
                    <button
                      onClick={() => setDeleteTarget(detail.profile)}
                      className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-error/20 hover:text-error"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                </div>
              </div>

              {/* Strategy overview */}
              <div className="grid grid-cols-1 gap-3 lg:grid-cols-4">
                <SummaryTile
                  label={t("routes.overview_protocol")}
                  value={protocolLabel(detail.profile.input_protocol)}
                  hint={detail.profile.is_default ? t("routes.default_profile") : t("routes.custom_profile")}
                />
                <SummaryTile
                  label={t("routes.overview_mode")}
                  value={detail.profile.mode === "failover" ? t("routes.mode_failover") : t("routes.mode_manual")}
                  hint={detail.profile.mode === "failover" ? t("routes.failover_hint") : t("routes.manual_hint")}
                />
                <SummaryTile
                  label={t("routes.overview_primary")}
                  value={detail.profile.active_provider_name ?? t("common.none")}
                  hint={t("routes.primary_hint")}
                />
                <SummaryTile
                  label={t("routes.overview_fallback")}
                  value={detail.profile.mode === "failover" ? `${Math.max(detail.providers.length - 1, 0)} ${t("routes.fallback_count")}` : t("routes.not_enabled")}
                  hint={detail.profile.mode === "failover" ? t("routes.fallback_hint") : t("routes.no_fallback_hint")}
                />
              </div>

              {/* Conditions */}
              <div className="rounded-xl border border-border bg-card p-5">
                <div className="mb-3">
                  <h4 className="text-xs font-semibold text-text-primary">{t("routes.conditions_section")}</h4>
                  <p className="mt-0.5 text-[11px] text-text-muted">{t("routes.conditions_section_hint")}</p>
                </div>
                {detail.providers.some((rp) => rp.routing_conditions) ? (
                  <div className="space-y-2">
                    {detail.providers.filter((rp) => rp.routing_conditions).map((rp) => (
                      <div key={rp.id} className="flex items-center justify-between rounded-md border border-border/50 bg-card-secondary px-4 py-3">
                        <div className="min-w-0">
                          <p className="truncate text-sm font-medium text-text-primary">{rp.provider_name}</p>
                          <p className="mt-0.5 text-[11px] text-text-muted">{describeRoutingConditions(rp.routing_conditions, t)}</p>
                        </div>
                        <button
                          onClick={() => {
                            let current: RoutingConditions = {};
                            try { if (rp.routing_conditions) current = JSON.parse(rp.routing_conditions); } catch {}
                            setConditionsTarget({ profileId: detail.profile.id, providerId: rp.provider_id, providerName: rp.provider_name, inputProtocol: detail.profile.input_protocol, current });
                          }}
                          className="ml-3 rounded p-1 text-text-muted hover:bg-border hover:text-accent"
                          title={t("routes.edit_conditions")}
                        >
                          <Filter className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className="rounded-md border border-border/50 bg-card-secondary px-4 py-3 text-[11px] text-text-muted">
                    {t("routes.no_conditions_summary")}
                  </p>
                )}
              </div>

              {/* Fallback */}
              <div className="rounded-xl border border-border bg-card p-5">
                <div className="mb-3 flex items-center justify-between gap-3">
                  <div>
                    <h4 className="text-xs font-semibold text-text-primary">{t("routes.fallback_section")}</h4>
                    <p className="mt-0.5 text-[11px] text-text-muted">
                      {detail.profile.mode === "failover" ? t("routes.fallback_section_hint") : t("routes.fallback_disabled_hint")}
                    </p>
                  </div>
                  {detail.profile.mode === "failover" && (
                    <StatusBadge variant="accent">{t("routes.strategy")}: {strategyLabel(detail.profile.selection_strategy, t)}</StatusBadge>
                  )}
                </div>
                {detail.profile.mode === "failover" ? (
                  detail.providers.length > 1 ? (
                    <div className="flex flex-wrap items-center gap-2">
                      {detail.providers.map((rp, idx) => (
                        <div key={rp.id} className="flex items-center gap-2">
                          <span className={`rounded-md border px-3 py-1.5 text-xs ${idx === 0 ? "border-accent/30 bg-accent/5 text-accent" : "border-border bg-card-secondary text-text-secondary"}`}>
                            {idx === 0 ? t("routes.primary_provider") : `${t("routes.fallback_provider")} ${idx}`} · {rp.provider_name}
                          </span>
                          {idx < detail.providers.length - 1 && <Route className="h-3.5 w-3.5 text-text-muted" />}
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="rounded-md border border-warning/30 bg-warning/10 px-4 py-3 text-[11px] text-warning">
                      {t("routes.fallback_needs_more")}
                    </p>
                  )
                ) : (
                  <p className="rounded-md border border-border/50 bg-card-secondary px-4 py-3 text-[11px] text-text-muted">
                    {t("routes.manual_no_fallback")}
                  </p>
                )}
              </div>

              {/* Provider order */}
              <div className="rounded-xl border border-border bg-card p-5">
                <div className="mb-3 flex items-center justify-between">
                  <div>
                    <h4 className="text-xs font-semibold text-text-primary">{t("routes.provider_order")}</h4>
                    <p className="mt-0.5 text-[11px] text-text-muted">{t("routes.provider_order_hint")}</p>
                  </div>
                  {detail.profile.active_provider_name && (
                    <span className="text-[11px] text-text-muted">
                      {t("routes.active")}: <span className="text-text-primary">{detail.profile.active_provider_name}</span>
                    </span>
                  )}
                </div>
                <div className="space-y-2">
                  {detail.providers.map((rp, idx) => {
                    const isActive = rp.provider_id === detail.profile.active_provider_id;
                    const isCooldown = rp.cooldown_until && new Date(rp.cooldown_until) > new Date();
                    return (
                      <div
                        key={rp.id}
                        className={`flex items-center justify-between rounded-md border px-4 py-3 ${
                          isActive ? "border-accent/30 bg-accent/5" : "border-border/50 bg-card-secondary"
                        }`}
                      >
                        <div className="flex items-center gap-3">
                          <span className="w-5 text-center text-xs font-mono text-text-muted">{rp.priority}</span>
                          <div>
                            <div className="flex items-center gap-2">
                              <span className="text-sm font-medium text-text-primary">{rp.provider_name}</span>
                              {isActive && <StatusBadge variant="accent">{t("routes.active")}</StatusBadge>}
                              {isCooldown && <StatusBadge variant="warning">{t("routes.cooldown")}</StatusBadge>}
                              {rp.consecutive_failures > 0 && (
                                <StatusBadge variant="error">{rp.consecutive_failures} {t("routes.failures")}</StatusBadge>
                              )}
                              {(() => {
                                const providerProtocols: string[] = (() => {
                                  try { return JSON.parse(rp.provider_protocol); } catch { return [rp.provider_protocol]; }
                                })();
                                const inputProto = detail.profile.input_protocol;
                                const isTransparent =
                                  (inputProto === "openai_chat_completions" && providerProtocols.includes("openai_chat_completions")) ||
                                  (inputProto === "anthropic_messages" && rp.has_anthropic_url) ||
                                  (inputProto === "openai_responses" && providerProtocols.includes("openai_responses"));
                                return (
                                  <StatusBadge variant={isTransparent ? "muted" : "success"}>
                                    {isTransparent ? t("routes.proxy_mode_transparent") : t("routes.proxy_mode_convert")}
                                  </StatusBadge>
                                );
                              })()}
                              <CapabilityIcons
                                modelCapabilities={rp.model_capabilities}
                                legacyVision={rp.supports_vision}
                              />
                              {rp.routing_conditions && (
                                <StatusBadge variant="success"><Filter className="inline h-2.5 w-2.5 mr-0.5" />{t("routes.has_conditions")}</StatusBadge>
                              )}
                            </div>
                            <p className="text-[11px] text-text-muted">
                              {rp.provider_type}
                              {rp.model_override && <> · model: {rp.model_override}</>}
                              {rp.routing_conditions && (() => {
                                try {
                                  const c: RoutingConditions = JSON.parse(rp.routing_conditions);
                                  const parts: string[] = [];
                                  if (c.has_images === true) parts.push("images");
                                  if (c.has_tools === true) parts.push("tools");
                                  if (c.min_input_chars) parts.push(`≥${(c.min_input_chars/1000).toFixed(0)}K chars`);
                                  if (c.system_keywords?.length) parts.push(`keywords: ${c.system_keywords.join(",")}`);
                                  if (c.model_override) parts.push(`→ ${c.model_override}`);
                                  return parts.length > 0 ? <> · <span className="text-accent">{parts.join(" + ")}</span></> : null;
                                } catch { return <> · <span className="text-error">{t("routes.invalid_conditions")}</span></>; }
                              })()}
                            </p>
                          </div>
                        </div>
                        <div className="flex items-center gap-1">
                          <button onClick={() => {
                            let current: RoutingConditions = {};
                            try { if (rp.routing_conditions) current = JSON.parse(rp.routing_conditions); } catch {}
                            setConditionsTarget({ profileId: detail.profile.id, providerId: rp.provider_id, providerName: rp.provider_name, inputProtocol: detail.profile.input_protocol, current });
                          }} className="rounded p-1 text-text-muted hover:bg-border hover:text-accent" title={t("routes.edit_conditions")}>
                            <Filter className="h-3.5 w-3.5" />
                          </button>
                          {!isActive && (
                            <button onClick={() => handleSetActive(rp.provider_id)} className="rounded p-1 text-text-muted hover:bg-border hover:text-text-primary" title={t("routes.set_active")}>
                              <Star className="h-3.5 w-3.5" />
                            </button>
                          )}
                          <button onClick={() => handleReorder(rp.provider_id, "up")} disabled={idx === 0} className="rounded p-1 text-text-muted hover:bg-border hover:text-text-primary disabled:opacity-30" title={t("routes.move_up")}>
                            <ChevronUp className="h-3.5 w-3.5" />
                          </button>
                          <button onClick={() => handleReorder(rp.provider_id, "down")} disabled={idx === detail.providers.length - 1} className="rounded p-1 text-text-muted hover:bg-border hover:text-text-primary disabled:opacity-30" title={t("routes.move_down")}>
                            <ChevronDown className="h-3.5 w-3.5" />
                          </button>
                          {isCooldown && (
                            <button onClick={() => handleResetCooldown(rp.provider_id)} className="rounded p-1 text-text-muted hover:bg-border hover:text-warning" title={t("routes.reset_cooldown")}>
                              <RotateCcw className="h-3.5 w-3.5" />
                            </button>
                          )}
                          <button onClick={() => handleRemoveProvider(rp.provider_id)} className="rounded p-1 text-text-muted hover:bg-error/20 hover:text-error" title={t("common.delete")}>
                            <Trash2 className="h-3.5 w-3.5" />
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>

                {/* Add provider */}
                {availableProviders.length > 0 && (
                  <div className="mt-3 flex items-center gap-2">
                    <select value={addProviderId} onChange={(e) => setAddProviderId(e.target.value)} className="form-input flex-1">
                      <option value="" disabled>{t("routes.add_provider")}</option>
                      {availableProviders.map((p) => (
                        <option key={p.id} value={p.id}>{p.name}</option>
                      ))}
                    </select>
                    <button
                      onClick={() => {
                        if (addProviderId) { handleAddProvider(addProviderId); setAddProviderId(""); }
                      }}
                      className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent/90"
                    >
                      <Plus className="h-3 w-3" />{t("routes.add")}
                    </button>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      )}

      <ConfirmDialog
        open={!!deleteTarget}
        title={t("routes.delete_title")}
        message={`${t("common.delete")} "${deleteTarget?.name}"? ${t("routes.delete_confirm")}`}
        confirmLabel={t("common.delete")}
        variant="danger"
        onConfirm={handleDelete}
        onCancel={() => setDeleteTarget(null)}
      />

      {/* Routing Conditions Dialog */}
      {conditionsTarget && (
        <ConditionsDialog
          target={conditionsTarget}
          onSave={async (conditions) => {
            const json = Object.values(conditions).some(v => v != null && v !== "" && !(Array.isArray(v) && v.length === 0))
              ? JSON.stringify(conditions)
              : null;
            try {
              await api.updateRouteProviderConditions(conditionsTarget.profileId, conditionsTarget.providerId, json);
              toast("success", t("routes.conditions_saved"));
              setConditionsTarget(null);
              load();
            } catch (err) { toast("error", (err as api.AppError).message); }
          }}
          onClose={() => setConditionsTarget(null)}
        />
      )}
    </div>
  );
}

function SummaryTile({ label, value, hint }: { label: string; value: string; hint: string }) {
  return (
    <div className="rounded-xl border border-border bg-card p-4">
      <p className="text-[10px] uppercase tracking-wide text-text-muted">{label}</p>
      <p className="mt-1 truncate text-sm font-semibold text-text-primary" title={value}>{value}</p>
      <p className="mt-1 text-[11px] text-text-muted">{hint}</p>
    </div>
  );
}

type ConditionPreset = {
  key: string;
  icon: LucideIcon;
  group: "primary" | "advanced";
  conditions: RoutingConditions;
};

const CONDITION_PRESETS: ConditionPreset[] = [
  { key: "images", icon: ImageIcon, group: "primary", conditions: { has_images: true } },
  { key: "long_text", icon: FileText, group: "primary", conditions: { min_input_chars: 100000 } },
  { key: "tools", icon: Wrench, group: "primary", conditions: { has_tools: true } },
  { key: "reasoning", icon: Brain, group: "advanced", conditions: { system_keywords: ["reason", "think", "analyze", "深度", "推理"] } },
  { key: "background", icon: GitBranch, group: "advanced", conditions: { system_keywords: ["background", "subagent", "后台"] } },
];

function strategyLabel(strategy: string, t: (key: string) => string): string {
  if (strategy === "cheapest") return t("routes.strategy_cheapest");
  if (strategy === "fastest") return t("routes.strategy_fastest");
  return t("routes.strategy_priority");
}

function describeRoutingConditions(json: string | null, t: (key: string) => string): string {
  if (!json) return t("routes.no_conditions_summary");
  try {
    const c: RoutingConditions = JSON.parse(json);
    const parts: string[] = [];
    if (c.has_images === true) parts.push(t("routes.condition_images_required"));
    if (c.has_images === false) parts.push(t("routes.condition_images_excluded"));
    if (c.has_tools === true) parts.push(t("routes.condition_tools_required"));
    if (c.has_tools === false) parts.push(t("routes.condition_tools_excluded"));
    if (c.min_input_chars) parts.push(`${t("routes.condition_min_chars")} ${(c.min_input_chars / 1000).toFixed(0)}K`);
    if (c.max_input_chars) parts.push(`${t("routes.condition_max_chars")} ${(c.max_input_chars / 1000).toFixed(0)}K`);
    if (c.system_keywords?.length) parts.push(`${t("routes.condition_keywords")} ${c.system_keywords.join(", ")}`);
    if (c.model_override) parts.push(`${t("routes.condition_use_model")} ${c.model_override}`);
    return parts.length > 0 ? parts.join(" · ") : t("routes.no_conditions_summary");
  } catch {
    return t("routes.invalid_conditions");
  }
}

function hasCustomOnlyConditions(c: RoutingConditions): boolean {
  if (c.max_input_chars != null) return true;
  if (c.has_images === false || c.has_tools === false) return true;
  if (c.min_input_chars != null && c.min_input_chars !== 100000) return true;
  const knownKeywords = new Set(["reason", "think", "analyze", "深度", "推理", "background", "subagent", "后台"]);
  return Boolean(c.system_keywords?.some(k => !knownKeywords.has(k)));
}

function detectCheckedPresets(c: RoutingConditions): Set<string> {
  const checked = new Set<string>();
  if (c.has_images === true) checked.add("images");
  if (c.has_tools === true) checked.add("tools");
  if (c.min_input_chars && c.min_input_chars >= 100000) checked.add("long_text");
  if (c.system_keywords?.some(k => ["reason", "think", "analyze", "推理"].includes(k))) checked.add("reasoning");
  if (c.system_keywords?.some(k => ["background", "subagent", "后台"].includes(k))) checked.add("background");
  return checked;
}

function mergePresetConditions(checked: Set<string>): RoutingConditions {
  const c: RoutingConditions = {};
  if (checked.has("images")) c.has_images = true;
  if (checked.has("tools")) c.has_tools = true;
  if (checked.has("long_text")) c.min_input_chars = 100000;
  const allKeywords: string[] = [];
  if (checked.has("reasoning")) allKeywords.push("reason", "think", "analyze", "深度", "推理");
  if (checked.has("background")) allKeywords.push("background", "subagent", "后台");
  if (allKeywords.length > 0) c.system_keywords = allKeywords;
  return c;
}

function ConditionsDialog({ target, onSave, onClose }: {
  target: { providerName: string; inputProtocol: string; current: RoutingConditions };
  onSave: (c: RoutingConditions) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [checked, setChecked] = useState(() => detectCheckedPresets(target.current));
  const [showCustom, setShowCustom] = useState(() => hasCustomOnlyConditions(target.current));
  const [showAdvanced, setShowAdvanced] = useState(() => {
    const initial = detectCheckedPresets(target.current);
    return initial.has("reasoning") || initial.has("background") || hasCustomOnlyConditions(target.current);
  });
  const [modelOverride, setModelOverride] = useState(target.current.model_override ?? "");

  // Custom fields (only used in custom mode)
  const [minChars, setMinChars] = useState(target.current.min_input_chars?.toString() ?? "");
  const [maxChars, setMaxChars] = useState(target.current.max_input_chars?.toString() ?? "");
  const [hasImages, setHasImages] = useState<string>(target.current.has_images === true ? "true" : target.current.has_images === false ? "false" : "");
  const [hasTools, setHasTools] = useState<string>(target.current.has_tools === true ? "true" : target.current.has_tools === false ? "false" : "");
  const [keywords, setKeywords] = useState(target.current.system_keywords?.join(", ") ?? "");

  const toggle = (key: string) => {
    const next = new Set(checked);
    if (next.has(key)) next.delete(key); else next.add(key);
    setChecked(next);
  };

  const handleSave = () => {
    let c: RoutingConditions;

    if (showCustom) {
      // Custom mode: build from raw fields
      c = {};
      if (minChars) c.min_input_chars = parseInt(minChars, 10) || null;
      if (maxChars) c.max_input_chars = parseInt(maxChars, 10) || null;
      if (hasImages === "true") c.has_images = true;
      else if (hasImages === "false") c.has_images = false;
      if (hasTools === "true") c.has_tools = true;
      else if (hasTools === "false") c.has_tools = false;
      if (keywords.trim()) c.system_keywords = keywords.split(",").map(s => s.trim()).filter(Boolean);
    } else {
      // Preset mode: merge checked presets
      c = mergePresetConditions(checked);
    }

    if (modelOverride.trim()) c.model_override = modelOverride.trim();
    onSave(c);
  };

  const hasAny = checked.size > 0 || showCustom || modelOverride.trim().length > 0;
  const primaryPresets = CONDITION_PRESETS.filter(p => p.group === "primary");
  const advancedPresets = CONDITION_PRESETS.filter(p => p.group === "advanced");
  const isResponsesProfile = target.inputProtocol === "openai_responses";

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative z-10 w-full max-w-md rounded-lg border border-border bg-card shadow-xl">
        <div className="flex items-center justify-between border-b border-border px-5 py-3">
          <h3 className="text-sm font-semibold text-text-primary">
            {t("routes.edit_conditions")} — {target.providerName}
          </h3>
          <button onClick={onClose} className="rounded p-1 text-text-muted hover:text-text-primary"><X className="h-4 w-4" /></button>
        </div>
        <div className="space-y-3 p-5">
          <p className="text-[11px] text-text-muted">{t("routes.conditions_hint")}</p>
          {!isResponsesProfile && (
            <p className="rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-[11px] text-warning">
              {t("routes.conditions_protocol_note")}
            </p>
          )}

          {/* Multi-select scene checkboxes */}
          {!showCustom && (
            <>
              <div className="grid grid-cols-2 gap-2">
                {primaryPresets.map(p => {
                  const Icon = p.icon;
                  return (
                    <label key={p.key} className={`flex cursor-pointer items-center gap-2 rounded-md border px-3 py-2 text-xs transition-colors ${checked.has(p.key) ? "border-accent bg-accent-soft text-accent" : "border-border text-text-secondary hover:border-accent/50"}`}>
                      <input type="checkbox" checked={checked.has(p.key)} onChange={() => toggle(p.key)} className="accent-accent" />
                      <Icon className="h-3.5 w-3.5" /> {t(`routes.scene_${p.key}`)}
                    </label>
                  );
                })}
              </div>

              <button onClick={() => setShowAdvanced(!showAdvanced)} className="flex items-center gap-1.5 text-[11px] text-accent hover:text-accent/80">
                <SlidersHorizontal className="h-3 w-3" />
                {showAdvanced ? t("routes.hide_advanced_conditions") : t("routes.show_advanced_conditions")}
              </button>

              {showAdvanced && (
                <div className="space-y-2 rounded-md border border-border/50 bg-card-secondary p-3">
                  <div className="grid grid-cols-2 gap-2">
                    {advancedPresets.map(p => {
                      const Icon = p.icon;
                      return (
                        <label key={p.key} className={`flex cursor-pointer items-center gap-2 rounded-md border px-3 py-2 text-xs transition-colors ${checked.has(p.key) ? "border-accent bg-accent-soft text-accent" : "border-border text-text-secondary hover:border-accent/50"}`}>
                          <input type="checkbox" checked={checked.has(p.key)} onChange={() => toggle(p.key)} className="accent-accent" />
                          <Icon className="h-3.5 w-3.5" /> {t(`routes.scene_${p.key}`)}
                        </label>
                      );
                    })}
                  </div>
                  <button onClick={() => setShowCustom(true)} className="text-[11px] text-accent hover:text-accent/80">
                    {t("routes.scene_custom")}
                  </button>
                </div>
              )}
            </>
          )}

          {/* Toggle custom mode */}
          {showCustom && (
            <button onClick={() => setShowCustom(false)} className="text-[11px] text-accent hover:text-accent/80">
              {t("routes.back_to_presets")}
            </button>
          )}

          {/* Custom fields */}
          {showCustom && (
            <div className="space-y-3 rounded-md border border-border/50 bg-card-secondary p-3">
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="mb-1 block text-[10px] text-text-muted">{t("routes.min_chars")}</label>
                  <input type="number" value={minChars} onChange={(e) => setMinChars(e.target.value)} placeholder="100000" className="form-input w-full" />
                </div>
                <div>
                  <label className="mb-1 block text-[10px] text-text-muted">{t("routes.max_chars")}</label>
                  <input type="number" value={maxChars} onChange={(e) => setMaxChars(e.target.value)} placeholder="500000" className="form-input w-full" />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="mb-1 block text-[10px] text-text-muted">{t("routes.has_images")}</label>
                  <select value={hasImages} onChange={(e) => setHasImages(e.target.value)} className="form-input w-full">
                    <option value="">{t("routes.any")}</option>
                    <option value="true">{t("routes.required")}</option>
                    <option value="false">{t("routes.excluded")}</option>
                  </select>
                </div>
                <div>
                  <label className="mb-1 block text-[10px] text-text-muted">{t("routes.has_tools")}</label>
                  <select value={hasTools} onChange={(e) => setHasTools(e.target.value)} className="form-input w-full">
                    <option value="">{t("routes.any")}</option>
                    <option value="true">{t("routes.required")}</option>
                    <option value="false">{t("routes.excluded")}</option>
                  </select>
                </div>
              </div>
              <div>
                <label className="mb-1 block text-[10px] text-text-muted">{t("routes.system_keywords")}</label>
                <input value={keywords} onChange={(e) => setKeywords(e.target.value)} placeholder="background, subagent" className="form-input w-full" />
                <p className="mt-0.5 text-[10px] text-text-muted">{t("routes.keywords_hint")}</p>
              </div>
            </div>
          )}

          {/* Model override */}
          {hasAny && (
            <div>
              <label className="mb-1 block text-[10px] text-text-muted">{t("routes.condition_model_override")}</label>
              <input value={modelOverride} onChange={(e) => setModelOverride(e.target.value)} placeholder="e.g. deepseek-v4-flash" className="form-input w-full" />
              <p className="mt-0.5 text-[10px] text-text-muted">{t("routes.model_override_hint")}</p>
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <button onClick={() => { onSave({}); }} className="rounded-md bg-card-secondary px-4 py-1.5 text-xs text-text-secondary hover:bg-border">{t("routes.clear_conditions")}</button>
          <button onClick={onClose} className="rounded-md bg-card-secondary px-4 py-1.5 text-xs text-text-secondary hover:bg-border">{t("common.cancel")}</button>
          <button onClick={handleSave} className="rounded-md bg-accent px-4 py-1.5 text-xs font-medium text-white hover:bg-accent/90">{t("common.save")}</button>
        </div>
      </div>
    </div>
  );
}
