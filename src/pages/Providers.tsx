import { useState, useEffect, useCallback, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { Plus, Inbox, Search } from "lucide-react";
import { ProviderCard } from "@/components/providers/ProviderCard";
import { ProviderFormDialog } from "@/components/providers/ProviderFormDialog";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type {
  ProviderView,
  CreateProviderInput,
  UpdateProviderInput,
} from "@/types/provider";

export function Providers() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [loading, setLoading] = useState(true);
  const [formOpen, setFormOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<ProviderView | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ProviderView | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  // 过滤：name / provider_type / default_model 任一匹配（大小写不敏感）。
  // provider 数少时不显示搜索框（减少视觉噪音），>= 5 个才出现。
  const filteredProviders = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return providers;
    return providers.filter((p) =>
      p.name.toLowerCase().includes(q) ||
      p.provider_type.toLowerCase().includes(q) ||
      p.default_model.toLowerCase().includes(q)
    );
  }, [providers, search]);

  const loadProviders = useCallback(async () => {
    try {
      const data = await api.listProviders();
      setProviders(data);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadProviders();
  }, [loadProviders]);

  const handleCreate = async (input: CreateProviderInput | UpdateProviderInput) => {
    try {
      const created = await api.createProvider(input as CreateProviderInput);
      setFormOpen(false);
      // Auto-detect vision support for new provider
      api.detectProviderVision(created.id).catch(() => {});
      api.detectProviderCache(created.id).catch(() => {});
      // 添加后总 provider ≥2 时提示用户可以做失败转移——跨页提示，避免
      // 用户配完想做 failover 还得自己探索"Routing"页在哪。
      if (providers.length >= 1) {
        toast("success", `${t("providers.created")} · ${t("providers.hint_setup_failover")}`, {
          action: { label: t("providers.go_routing"), onClick: () => navigate("/routes") },
        });
      } else {
        toast("success", t("providers.created"));
      }
      loadProviders();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleEdit = (provider: ProviderView) => {
    setEditTarget(provider);
    setFormOpen(true);
  };

  const handleUpdate = async (input: CreateProviderInput | UpdateProviderInput) => {
    if (!editTarget) return;
    try {
      await api.updateProvider(editTarget.id, input as UpdateProviderInput);
      toast("success", t("providers.updated"));
      setFormOpen(false);
      setEditTarget(null);
      // Auto-detect vision support after update
      api.detectProviderVision(editTarget.id).catch(() => {});
      api.detectProviderCache(editTarget.id).catch(() => {});
      loadProviders();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    try {
      await api.deleteProvider(deleteTarget.id);
      toast("success", `"${deleteTarget.name}" ${t("providers.deleted")}`);
      setDeleteTarget(null);
      loadProviders();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleSetActive = async (provider: ProviderView) => {
    try {
      await api.setActiveProvider(provider.id);
      toast("success", `"${provider.name}" ${t("providers.now_active")}`);
      loadProviders();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleTest = async (provider: ProviderView) => {
    setTestingId(provider.id);
    try {
      // 1. Connectivity check — verifies API key + base_url.
      const result = await api.testProvider(provider.id);
      if (!result.success) {
        toast("error", result.message);
        loadProviders();
        return;
      }
      toast("success", result.message);

      // 2. Auto-fill missing rows in the capability matrix from seed defaults.
      //    Preserves manual edits. No upstream calls — purely name-pattern based.
      try {
        const filled = await api.autofillProviderCapabilities(provider.id);
        if (filled > 0) toast("success", `已自动识别 ${filled} 个模型的能力`);
      } catch { /* best-effort */ }

      // 3. Cache probe (anthropic-style providers only).
      try {
        const cacheResult = await api.detectProviderCache(provider.id);
        if (cacheResult.success) toast("success", cacheResult.message);
      } catch { /* best-effort */ }

      loadProviders();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setTestingId(null);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-3">
        <div className="flex flex-1 items-center gap-3">
          <p className="shrink-0 text-xs text-text-muted">
            {search ? `${filteredProviders.length} / ${providers.length}` : providers.length} {t("providers.configured")}
          </p>
          {/* provider 数 >=5 时显示搜索框——少于 5 个时直接看完更快 */}
          {providers.length >= 5 && (
            <div className="relative max-w-xs flex-1">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted" />
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder={t("providers.search_placeholder")}
                className="form-input w-full pl-8 text-xs"
              />
            </div>
          )}
        </div>
        <button
          onClick={() => {
            setEditTarget(null);
            setFormOpen(true);
          }}
          className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-accent/90"
        >
          <Plus className="h-3.5 w-3.5" />
          {t("providers.add")}
        </button>
      </div>

      {loading ? (
        <p className="text-xs text-text-muted">{t("common.loading")}</p>
      ) : providers.length === 0 ? (
        <EmptyState
          icon={Inbox}
          title={t("providers.no_providers")}
          description={t("providers.add_first")}
        />
      ) : filteredProviders.length === 0 ? (
        <p className="text-xs text-text-muted">{t("providers.no_match")}</p>
      ) : (
        <div className="grid grid-cols-2 gap-4">
          {filteredProviders.map((provider) => (
            <ProviderCard
              key={provider.id}
              provider={provider}
              onEdit={handleEdit}
              onDelete={setDeleteTarget}
              onSetActive={handleSetActive}
              onTest={handleTest}
              testing={testingId === provider.id}
            />
          ))}
        </div>
      )}

      <ProviderFormDialog
        open={formOpen}
        provider={editTarget}
        onSubmit={editTarget ? handleUpdate : handleCreate}
        onClose={() => {
          setFormOpen(false);
          setEditTarget(null);
        }}
      />

      <ConfirmDialog
        open={!!deleteTarget}
        title={t("providers.delete")}
        message={`${t("providers.delete_confirm")} "${deleteTarget?.name}"? ${t("providers.delete_warn")}`}
        confirmLabel={t("common.delete")}
        variant="danger"
        onConfirm={handleDelete}
        onCancel={() => setDeleteTarget(null)}
      />
    </div>
  );
}
