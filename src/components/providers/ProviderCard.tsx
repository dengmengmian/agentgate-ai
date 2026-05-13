import { Cloud, Key, ExternalLink, Pencil, Trash2, Star, Loader2 } from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { useI18n } from "@/lib/i18n";
import type { ProviderView } from "@/types/provider";

interface ProviderCardProps {
  provider: ProviderView;
  onEdit: (provider: ProviderView) => void;
  onDelete: (provider: ProviderView) => void;
  onSetActive: (provider: ProviderView) => void;
  onTest: (provider: ProviderView) => void;
  testing?: boolean;
}

export function ProviderCard({
  provider,
  onEdit,
  onDelete,
  onSetActive,
  onTest,
  testing,
}: ProviderCardProps) {
  const { t } = useI18n();

  return (
    <div className={`rounded-lg border bg-card p-5 ${provider.is_active ? "border-accent/40" : "border-border"}`}>
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-accent/10">
            <Cloud className="h-4 w-4 text-accent" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h3 className="text-sm font-semibold text-text-primary">{provider.name}</h3>
              {provider.is_active && <StatusBadge variant="accent">{t("providers.active")}</StatusBadge>}
            </div>
            <p className="font-mono text-[11px] text-text-muted">{provider.base_url}</p>
          </div>
        </div>
        <StatusBadge variant={provider.status === "connected" ? "success" : provider.status === "failed" ? "error" : "muted"}>
          {provider.status}
        </StatusBadge>
      </div>

      <div className="mb-4 grid grid-cols-2 gap-y-2.5 text-xs">
        <div>
          <span className="text-text-muted">{t("providers.type")}</span>
          <p className="text-text-primary">{provider.provider_type}</p>
        </div>
        <div>
          <span className="text-text-muted">{t("providers.protocol")}</span>
          <p className="text-text-primary">{provider.protocol}</p>
        </div>
        <div>
          <span className="text-text-muted">{t("providers.api_key")}</span>
          <p className="flex items-center gap-1 text-text-primary">
            <Key className="h-3 w-3" />
            {provider.masked_api_key ? (
              <span className="font-mono text-[11px]">{provider.masked_api_key}</span>
            ) : (
              t("providers.not_set")
            )}
          </p>
        </div>
        <div>
          <span className="text-text-muted">{t("providers.default_model")}</span>
          <p className="font-mono text-text-primary">{provider.default_model}</p>
        </div>
        {provider.reasoning_model && (
          <div>
            <span className="text-text-muted">{t("providers.reasoning_model")}</span>
            <p className="font-mono text-text-primary">{provider.reasoning_model}</p>
          </div>
        )}
        <div>
          <span className="text-text-muted">{t("providers.timeout")}</span>
          <p className="text-text-primary">{provider.timeout_seconds}s</p>
        </div>
      </div>

      <div className="flex flex-wrap gap-2">
        <button onClick={() => onEdit(provider)} className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary">
          <Pencil className="h-3 w-3" />{t("common.edit")}
        </button>
        <button onClick={() => onTest(provider)} disabled={testing} className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary disabled:opacity-50">
          {testing ? <Loader2 className="h-3 w-3 animate-spin" /> : <ExternalLink className="h-3 w-3" />}
          {t("providers.test")}
        </button>
        {!provider.is_active && (
          <button onClick={() => onSetActive(provider)} className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary">
            <Star className="h-3 w-3" />{t("providers.set_active")}
          </button>
        )}
        <button onClick={() => onDelete(provider)} className="flex items-center gap-1.5 rounded-md bg-card-secondary px-3 py-1.5 text-[11px] font-medium text-text-secondary transition-colors hover:bg-error/20 hover:text-error">
          <Trash2 className="h-3 w-3" />{t("common.delete")}
        </button>
      </div>
    </div>
  );
}
