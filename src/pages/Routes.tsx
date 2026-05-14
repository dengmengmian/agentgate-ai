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
} from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { RouteProfileView, RouteProfileDetail } from "@/types/route-profile";
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
        <div className="rounded-lg border border-accent/30 bg-card p-4">
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
                    : "border-border bg-card hover:bg-card-secondary"
                }`}
              >
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium text-text-primary">{p.name}</span>
                  <StatusBadge variant={p.mode === "failover" ? "accent" : "muted"}>
                    {p.mode}
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
              <div className="rounded-lg border border-border bg-card p-5">
                <div className="mb-3 flex items-center justify-between">
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
                        <button onClick={handleRename} className="rounded p-1 text-accent hover:bg-accent/10"><Check className="h-3.5 w-3.5" /></button>
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
                  <div className="flex items-center gap-2">
                    <button
                      onClick={handleToggleMode}
                      className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
                    >
                      {detail.profile.mode === "manual" ? (
                        <><Zap className="h-3 w-3" />{t("routes.enable_failover")}</>
                      ) : (
                        <><Shield className="h-3 w-3" />{t("routes.switch_manual")}</>
                      )}
                    </button>
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
                <div className="flex gap-3 text-[11px]">
                  <span className="rounded-md bg-card-secondary px-2.5 py-1 text-text-secondary">
                    {t("routes.mode")}: <span className={detail.profile.mode === "failover" ? "text-accent" : "text-text-primary"}>{detail.profile.mode}</span>
                  </span>
                  <span className="rounded-md bg-card-secondary px-2.5 py-1 text-text-secondary">
                    {t("routes.active")}: <span className="text-text-primary">{detail.profile.active_provider_name ?? t("common.none")}</span>
                  </span>
                </div>
              </div>

              {/* Provider chain */}
              <div className="rounded-lg border border-border bg-card p-5">
                <h4 className="mb-3 text-xs font-semibold text-text-primary">{t("routes.provider_chain")}</h4>
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
                            </div>
                            <p className="text-[11px] text-text-muted">
                              {rp.provider_type}
                              {rp.model_override && <> · model: {rp.model_override}</>}
                            </p>
                          </div>
                        </div>
                        <div className="flex items-center gap-1">
                          {!isActive && (
                            <button onClick={() => handleSetActive(rp.provider_id)} className="rounded p-1 text-text-muted hover:bg-border hover:text-text-primary" title="Set as active">
                              <Star className="h-3.5 w-3.5" />
                            </button>
                          )}
                          <button onClick={() => handleReorder(rp.provider_id, "up")} disabled={idx === 0} className="rounded p-1 text-text-muted hover:bg-border hover:text-text-primary disabled:opacity-30">
                            <ChevronUp className="h-3.5 w-3.5" />
                          </button>
                          <button onClick={() => handleReorder(rp.provider_id, "down")} disabled={idx === detail.providers.length - 1} className="rounded p-1 text-text-muted hover:bg-border hover:text-text-primary disabled:opacity-30">
                            <ChevronDown className="h-3.5 w-3.5" />
                          </button>
                          {isCooldown && (
                            <button onClick={() => handleResetCooldown(rp.provider_id)} className="rounded p-1 text-text-muted hover:bg-border hover:text-warning" title="Reset cooldown">
                              <RotateCcw className="h-3.5 w-3.5" />
                            </button>
                          )}
                          <button onClick={() => handleRemoveProvider(rp.provider_id)} className="rounded p-1 text-text-muted hover:bg-error/20 hover:text-error">
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
                    <select id="add-provider-select" className="form-input flex-1" defaultValue="">
                      <option value="" disabled>{t("routes.add_provider")}</option>
                      {availableProviders.map((p) => (
                        <option key={p.id} value={p.id}>{p.name}</option>
                      ))}
                    </select>
                    <button
                      onClick={() => {
                        const sel = document.getElementById("add-provider-select") as HTMLSelectElement;
                        if (sel.value) handleAddProvider(sel.value);
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
    </div>
  );
}
