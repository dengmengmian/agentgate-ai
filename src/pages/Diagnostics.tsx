import { useState } from "react";
import {
  Activity,
  Play,
  Download,
  FolderOpen,
  CheckCircle,
  XCircle,
  AlertTriangle,
  MinusCircle,
} from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type {
  FullSelfTestReport,
  CheckReport,
  CheckItem,
} from "@/types/diagnostics";

const statusIcon = (s: string) => {
  switch (s) {
    case "ok":
      return <CheckCircle className="h-3.5 w-3.5 text-success" />;
    case "warning":
      return <AlertTriangle className="h-3.5 w-3.5 text-warning" />;
    case "failed":
      return <XCircle className="h-3.5 w-3.5 text-error" />;
    default:
      return <MinusCircle className="h-3.5 w-3.5 text-text-muted" />;
  }
};

const statusVariant = (
  s: string
): "success" | "warning" | "error" | "muted" => {
  switch (s) {
    case "ok":
      return "success";
    case "warning":
      return "warning";
    case "failed":
      return "error";
    default:
      return "muted";
  }
};

export function Diagnostics() {
  const { t } = useI18n();
  const [report, setReport] = useState<FullSelfTestReport | null>(null);
  const [running, setRunning] = useState(false);
  const [exporting, setExporting] = useState(false);

  const handleRunTest = async () => {
    setRunning(true);
    try {
      const r = await api.runFullSelfTest();
      setReport(r);
      toast(
        r.overall_status === "ok"
          ? "success"
          : r.overall_status === "warning"
            ? "warning"
            : "error",
        r.summary
      );
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setRunning(false);
    }
  };

  const handleExport = async () => {
    setExporting(true);
    try {
      const result = await api.exportDiagnosticBundle(true, 50);
      if (result.success) {
        toast(
          "success",
          `${t("diag.exported")}: ${result.path.split("/").pop()}`
        );
      }
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* Actions */}
      <div
        className="relative overflow-hidden rounded-xl border border-accent/20 bg-card p-5"
        style={{
          boxShadow: "0 10px 30px rgba(194, 112, 43, 0.10)",
          background:
            "linear-gradient(135deg, var(--color-card) 0%, rgba(194,112,43,0.07) 100%)",
        }}
      >
        <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-accent/50 to-transparent" />
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-[0.16em] text-accent">
              {t("diag.diagnostic_console")}
            </p>
            <h3 className="mt-1 text-sm font-semibold text-text-primary">
              {t("diag.self_test")}
            </h3>
            <p className="mt-0.5 text-xs text-text-muted">
              {report?.summary ?? t("diag.run_prompt")}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <button
              onClick={handleRunTest}
              disabled={running}
              className="btn-primary"
            >
              {running ? (
                <Activity className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play className="h-3.5 w-3.5" />
              )}
              {t("diag.run_self_test")}
            </button>
            <button
              onClick={handleExport}
              disabled={exporting}
              className="btn-secondary"
            >
              <Download className="h-3.5 w-3.5" />
              {t("diag.export_bundle")}
            </button>
            <button
              onClick={() => api.openAppDataDir()}
              className="btn-secondary"
            >
              <FolderOpen className="h-3.5 w-3.5" />
              {t("diag.open_data_dir")}
            </button>
          </div>
        </div>
      </div>

      {/* Overall status */}
      {report && (
        <div
          className="rounded-xl border border-border bg-card p-5"
          style={{ boxShadow: "0 12px 30px rgba(17, 24, 39, 0.05)" }}
        >
          <div className="mb-4 flex items-center justify-between">
            <div className="flex items-center gap-3">
              {statusIcon(report.overall_status)}
              <div>
                <h3 className="text-sm font-semibold text-text-primary">
                  {t("diag.self_test")}
                </h3>
                <p className="text-xs text-text-muted">{report.summary}</p>
              </div>
            </div>
            <StatusBadge variant={statusVariant(report.overall_status)}>
              {report.overall_status}
            </StatusBadge>
          </div>
          <p className="text-[11px] text-text-muted">{report.created_at}</p>
        </div>
      )}

      {/* Individual reports */}
      {report?.reports.map((r) => (
        <ReportCard key={r.name} report={r} />
      ))}

      {!report && (
        <div className="py-16 text-center">
          <Activity className="mx-auto mb-4 h-10 w-10 text-text-muted" />
          <h3 className="mb-1 text-sm font-medium text-text-primary">
            {t("diag.no_report")}
          </h3>
          <p className="text-xs text-text-muted">{t("diag.run_prompt")}</p>
        </div>
      )}
    </div>
  );
}

function ReportCard({ report }: { report: CheckReport }) {
  const [expanded, setExpanded] = useState(report.status !== "ok");

  return (
    <div
      className="rounded-xl border border-border bg-card"
      style={{ boxShadow: "0 12px 30px rgba(17, 24, 39, 0.04)" }}
    >
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center justify-between px-5 py-3 text-left"
      >
        <div className="flex items-center gap-3">
          {statusIcon(report.status)}
          <span className="text-sm font-medium text-text-primary">
            {report.name}
          </span>
          <span className="text-xs text-text-muted">{report.summary}</span>
        </div>
        <StatusBadge variant={statusVariant(report.status)}>
          {report.status}
        </StatusBadge>
      </button>

      {expanded && (
        <div className="border-t border-border px-5 py-3">
          <div className="space-y-1.5">
            {report.checks.map((check, i) => (
              <CheckItemRow key={`${check.id}-${i}`} check={check} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function CheckItemRow({ check }: { check: CheckItem }) {
  return (
    <div className="flex items-start gap-2 py-1">
      <div className="mt-0.5 shrink-0">{statusIcon(check.status)}</div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-xs font-medium text-text-primary">
            {check.name}
          </span>
          <span className="text-[11px] text-text-secondary">
            {check.message}
          </span>
        </div>
        {check.detail && (
          <p className="mt-0.5 text-[11px] text-text-muted">{check.detail}</p>
        )}
        {check.suggestion && (
          <p className="mt-0.5 text-[11px] text-accent">{check.suggestion}</p>
        )}
      </div>
    </div>
  );
}
