import { AlertTriangle } from "lucide-react";
import { useI18n } from "@/lib/i18n";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: "danger" | "default";
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  variant = "default",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useI18n();
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[90] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onCancel} />
      <div className="relative z-10 w-full max-w-md rounded-lg border border-border bg-card p-6 shadow-xl">
        <div className="mb-4 flex items-center gap-3">
          {variant === "danger" && (
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-error-soft">
              <AlertTriangle className="h-4 w-4 text-error" />
            </div>
          )}
          <div>
            <h3 className="text-sm font-semibold text-text-primary">{title}</h3>
            <p className="mt-1 text-xs text-text-secondary">{message}</p>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="rounded-md bg-card-secondary px-4 py-2 text-xs font-medium text-text-secondary transition-colors hover:bg-border hover:text-text-primary"
          >
            {cancelLabel || t("common.cancel")}
          </button>
          <button
            onClick={onConfirm}
            className={`rounded-md px-4 py-2 text-xs font-medium text-white transition-colors ${
              variant === "danger"
                ? "bg-error hover:bg-error/90"
                : "bg-accent hover:bg-accent/90"
            }`}
          >
            {confirmLabel || t("common.confirm")}
          </button>
        </div>
      </div>
    </div>
  );
}
