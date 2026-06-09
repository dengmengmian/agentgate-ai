import { useEffect, useState } from "react";
import { History, X, RotateCcw, Loader2, Trash2 } from "lucide-react";
import * as api from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import { toast } from "@/components/common/Toast";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";

interface Props {
  clientId: string;
  clientName: string;
  /// 回滚成功后通知外部刷新检测状态。
  onRollbackDone?: () => void;
}

/// 客户端配置历史按钮 + 抽屉。每次 apply/disable/toggle 写入前，后端会先
/// snapshot 一次盘上配置；点击「回滚」就把那个时点的文件原文写回去。
/// 仅本地操作，不影响 AgentGate 内部 state（active provider 等）。
export function ClientHistoryButton({ clientId, clientName, onRollbackDone }: Props) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);

  return (
    <>
      <button onClick={() => setOpen(true)} className="btn-secondary">
        <History className="h-3 w-3" />
        {t("tools.history.button")}
      </button>
      {open && (
        <ClientHistoryDrawer
          clientId={clientId}
          clientName={clientName}
          onClose={() => setOpen(false)}
          onRollbackDone={onRollbackDone}
        />
      )}
    </>
  );
}

function ClientHistoryDrawer({
  clientId,
  clientName,
  onClose,
  onRollbackDone,
}: Props & { onClose: () => void }) {
  const { t, locale } = useI18n();
  const [entries, setEntries] = useState<api.ClientApplyHistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [rollbackTarget, setRollbackTarget] = useState<api.ClientApplyHistoryEntry | null>(null);
  const [rollingBack, setRollingBack] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<api.ClientApplyHistoryEntry | null>(null);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api
      .listClientApplyHistory(clientId)
      .then((rows) => {
        if (!cancelled) setEntries(rows);
      })
      .catch(() => {
        if (!cancelled) setEntries([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [clientId]);

  const performRollback = async (entry: api.ClientApplyHistoryEntry) => {
    setRollingBack(true);
    try {
      await api.rollbackClientApply(entry.id);
      toast("success", t("tools.history.rollback_done"));
      setRollbackTarget(null);
      onClose();
      onRollbackDone?.();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setRollingBack(false);
    }
  };

  const performDelete = async (entry: api.ClientApplyHistoryEntry) => {
    setDeleting(true);
    try {
      await api.deleteClientApplyHistory(entry.id);
      setEntries((prev) => prev.filter((e) => e.id !== entry.id));
      toast("success", t("tools.history.delete_done"));
      setDeleteTarget(null);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setDeleting(false);
    }
  };

  const fmtTime = (rfc3339: string) => {
    try {
      const d = new Date(rfc3339);
      return d.toLocaleString(locale === "zh" ? "zh-CN" : "en-US", {
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
    } catch {
      return rfc3339;
    }
  };

  const actionLabel = (entry: api.ClientApplyHistoryEntry) => {
    if (entry.is_initial) return t("tools.history.initial");
    return t(`tools.history.action.${entry.action}`) || entry.action;
  };

  return (
    <>
      <div className="fixed inset-0 z-[110]">
        <div className="fixed inset-0 bg-black/40 backdrop-blur-sm" onClick={onClose} />
        <div
          className="animate-slide-in-right fixed right-0 top-0 flex h-full w-full max-w-md flex-col border-l border-border bg-card"
          style={{ boxShadow: "var(--shadow-lg)" }}
        >
          <div className="flex items-start justify-between border-b border-border p-4">
            <div>
              <h3 className="text-sm font-semibold text-text-primary">
                {t("tools.history.title").replace("{name}", clientName)}
              </h3>
              <p className="mt-0.5 text-[11px] text-text-muted">{t("tools.history.subtitle")}</p>
            </div>
            <button
              onClick={onClose}
              className="rounded-md p-1 text-text-muted hover:bg-hover hover:text-text-primary"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto p-4">
            {loading ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-4 w-4 animate-spin text-text-muted" />
              </div>
            ) : entries.length === 0 ? (
              <div className="rounded-md border border-border bg-card-secondary p-4 text-center">
                <p className="text-xs text-text-secondary">{t("tools.history.empty")}</p>
                <p className="mt-1 text-[11px] text-text-muted">{t("tools.history.empty_hint")}</p>
              </div>
            ) : (
              <ol className="space-y-2">
                {entries.map((entry) => (
                  <li
                    key={entry.id}
                    className="rounded-md border border-border bg-card-secondary p-3"
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span
                            className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${
                              entry.is_initial
                                ? "bg-accent-soft text-accent"
                                : "bg-card text-text-secondary"
                            }`}
                          >
                            {actionLabel(entry)}
                          </span>
                          <span className="text-[11px] text-text-muted">
                            {fmtTime(entry.created_at)}
                          </span>
                        </div>
                        <p className="mt-1 text-[11px] text-text-secondary">
                          v{entry.agentgate_version}
                          {entry.summary ? ` · ${entry.summary}` : ""}
                        </p>
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        <button
                          type="button"
                          onClick={() => setRollbackTarget(entry)}
                          className="rounded border border-border bg-card px-2 py-1 text-[10px] font-medium text-text-primary hover:bg-hover"
                        >
                          <RotateCcw className="mr-1 inline h-3 w-3" />
                          {t("tools.history.rollback")}
                        </button>
                        {!entry.is_initial && (
                          <button
                            type="button"
                            onClick={() => setDeleteTarget(entry)}
                            title={t("tools.history.delete")}
                            aria-label={t("tools.history.delete")}
                            className="rounded border border-border bg-card p-1 text-text-muted hover:bg-hover hover:text-text-primary"
                          >
                            <Trash2 className="h-3 w-3" />
                          </button>
                        )}
                      </div>
                    </div>
                  </li>
                ))}
              </ol>
            )}
          </div>
        </div>
      </div>

      <ConfirmDialog
        open={!!rollbackTarget && !rollingBack}
        variant="danger"
        title={t("tools.history.confirm_title")}
        message={
          rollbackTarget
            ? t("tools.history.confirm_msg")
                .replace("{name}", clientName)
                .replace("{time}", fmtTime(rollbackTarget.created_at))
            : ""
        }
        confirmLabel={t("tools.history.confirm_btn")}
        onConfirm={() => rollbackTarget && performRollback(rollbackTarget)}
        onCancel={() => setRollbackTarget(null)}
      />

      <ConfirmDialog
        open={!!deleteTarget && !deleting}
        variant="danger"
        title={t("tools.history.delete_confirm_title")}
        message={t("tools.history.delete_confirm_msg")}
        confirmLabel={t("tools.history.delete")}
        onConfirm={() => deleteTarget && performDelete(deleteTarget)}
        onCancel={() => setDeleteTarget(null)}
      />
    </>
  );
}
