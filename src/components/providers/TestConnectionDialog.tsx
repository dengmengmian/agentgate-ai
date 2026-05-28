import { useEffect, useState } from "react";
import { CheckCircle, XCircle, Loader2, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { ProviderView } from "@/types/provider";

type StepStatus = "pending" | "running" | "ok" | "error" | "skipped";

interface StepState {
  status: StepStatus;
  detail?: string;
}

interface Props {
  /// 要测的 provider；null 时 dialog 关闭。
  provider: ProviderView | null;
  onClose: () => void;
  /// 所有步骤成功时回调，调用方刷新列表（autofill / cache detect 可能改了 provider 字段）
  onSuccess?: () => void;
}

/// Test Connection 的"分步进度"对话框。
///
/// 不爽利改进：原来 handleTest 串行 3 个 API call + 弹 3 个 toast，用户看着
/// 像"为啥连发好几个通知"。改成单 dialog 显示 3 个 step 实时状态：
/// - Connectivity（必跑、必显）
/// - Capability autofill（一定 OK，因为是 name-pattern based，无网络）
/// - Cache support detect（仅 Anthropic-style，否则 skipped）
///
/// 全成功 1.5s 后自动关；失败保持打开让用户看清错误 + 提示语。
export function TestConnectionDialog({ provider, onClose, onSuccess }: Props) {
  const { t } = useI18n();
  const [connectivity, setConnectivity] = useState<StepState>({ status: "pending" });
  const [autofill, setAutofill] = useState<StepState>({ status: "pending" });
  const [cacheDetect, setCacheDetect] = useState<StepState>({ status: "pending" });

  useEffect(() => {
    if (!provider) return;
    let cancelled = false;

    // 重置
    setConnectivity({ status: "running" });
    setAutofill({ status: "pending" });
    setCacheDetect({ status: "pending" });

    (async () => {
      // 1. 连接性
      try {
        const r = await api.testProvider(provider.id);
        if (cancelled) return;
        if (!r.success) {
          setConnectivity({ status: "error", detail: r.message });
          return; // 失败链路终止，保留对话框让用户读错误
        }
        const ms = r.latency_ms ? `${r.latency_ms}ms` : "";
        setConnectivity({ status: "ok", detail: ms });
      } catch (err) {
        if (!cancelled) setConnectivity({ status: "error", detail: (err as api.AppError).message });
        return;
      }

      // 2. Capability autofill
      if (cancelled) return;
      setAutofill({ status: "running" });
      try {
        const filled = await api.autofillProviderCapabilities(provider.id);
        if (cancelled) return;
        setAutofill({
          status: "ok",
          detail: filled > 0
            ? t("providers.test.autofill_n").replace("{n}", String(filled))
            : t("providers.test.autofill_none"),
        });
      } catch (err) {
        if (!cancelled) setAutofill({ status: "error", detail: (err as api.AppError).message });
      }

      // 3. Cache detect
      if (cancelled) return;
      setCacheDetect({ status: "running" });
      try {
        const r = await api.detectProviderCache(provider.id);
        if (cancelled) return;
        setCacheDetect({
          status: r.success ? "ok" : "skipped",
          detail: r.message,
        });
      } catch (err) {
        if (!cancelled) setCacheDetect({ status: "error", detail: (err as api.AppError).message });
      }

      // 全成功（含 skipped）→ 自动关闭
      if (!cancelled) {
        setTimeout(() => {
          if (!cancelled) {
            onSuccess?.();
            onClose();
          }
        }, 1500);
      }
    })();

    return () => { cancelled = true; };
  }, [provider, onClose, onSuccess, t]);

  if (!provider) return null;

  const anyError = [connectivity, autofill, cacheDetect].some(s => s.status === "error");

  return (
    <div className="fixed inset-0 z-[120] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/40 backdrop-blur-sm" onClick={onClose} />
      <div
        className="animate-scale-in relative z-10 w-full max-w-md rounded-xl border border-border bg-card p-6"
        style={{ boxShadow: "var(--shadow-lg)" }}
      >
        <div className="mb-4 flex items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold text-text-primary">
              {t("providers.test.title")}
            </h3>
            <p className="mt-0.5 text-xs text-text-muted">{provider.name}</p>
          </div>
          <button
            onClick={onClose}
            className="rounded-md p-1 text-text-muted hover:bg-hover hover:text-text-primary"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3">
          <StepRow label={t("providers.test.step_connectivity")} state={connectivity} />
          <StepRow label={t("providers.test.step_autofill")} state={autofill} />
          <StepRow label={t("providers.test.step_cache")} state={cacheDetect} />
        </div>

        {anyError && (
          <div className="mt-4 rounded-md border border-error/30 bg-error-soft p-3">
            <p className="text-[11px] font-medium text-error">{t("providers.test.error_hint_title")}</p>
            <p className="mt-1 text-[11px] text-text-secondary">
              {t("providers.test.error_hint_desc")}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

function StepRow({ label, state }: { label: string; state: StepState }) {
  return (
    <div className="flex items-start gap-3">
      <div className="mt-0.5 h-4 w-4 shrink-0">
        {state.status === "pending" && <div className="h-4 w-4 rounded-full border-2 border-border" />}
        {state.status === "running" && <Loader2 className="h-4 w-4 animate-spin text-accent" />}
        {state.status === "ok" && <CheckCircle className="h-4 w-4 text-success" />}
        {state.status === "error" && <XCircle className="h-4 w-4 text-error" />}
        {state.status === "skipped" && <div className="h-4 w-4 rounded-full bg-card-secondary" />}
      </div>
      <div className="min-w-0 flex-1">
        <p className={cn(
          "text-sm",
          state.status === "pending" ? "text-text-muted" : "text-text-primary",
        )}>
          {label}
        </p>
        {state.detail && (
          <p className={cn(
            "mt-0.5 break-words text-[11px]",
            state.status === "error" ? "text-error" : "text-text-muted",
          )}>
            {state.detail}
          </p>
        )}
      </div>
    </div>
  );
}
