import { useState, useEffect, useCallback } from "react";
import { Plus, Inbox } from "lucide-react";
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
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [loading, setLoading] = useState(true);
  const [formOpen, setFormOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<ProviderView | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ProviderView | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);

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
      toast("success", t("providers.created"));
      setFormOpen(false);
      // Auto-detect vision support for new provider
      api.detectProviderVision(created.id).catch(() => {});
      api.detectProviderCache(created.id).catch(() => {});
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
      const result = await api.testProvider(provider.id);
      if (result.success) {
        toast("success", result.message);
        // Auto-detect vision + cache support after successful connection test
        try {
          const visionResult = await api.detectProviderVision(provider.id);
          toast("success", visionResult.message);
        } catch { /* best-effort */ }
        try {
          const cacheResult = await api.detectProviderCache(provider.id);
          if (cacheResult.success) toast("success", cacheResult.message);
        } catch { /* best-effort */ }
      } else {
        toast("error", result.message);
      }
      loadProviders();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setTestingId(null);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <p className="text-xs text-text-muted">
          {providers.length} {t("providers.configured")}
        </p>
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
      ) : (
        <div className="grid grid-cols-2 gap-4">
          {providers.map((provider) => (
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
