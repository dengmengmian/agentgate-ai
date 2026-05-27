import { useState, useEffect, useCallback } from "react";
import { Radio, Play, Square, RotateCcw, Settings, Save } from "lucide-react";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import { PROTOCOLS } from "@/types/provider";
import type { GatewayStatus } from "@/types/gateway";
import type { GatewaySettings } from "@/types/gateway";

export function Gateway() {
  const { t } = useI18n();
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [settings, setSettings] = useState<GatewaySettings | null>(null);

  // Editable fields
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  const [inputProtocol, setInputProtocol] = useState("");
  const [outputProtocol, setOutputProtocol] = useState("");
  const [autoStart, setAutoStart] = useState(false);
  const [logRetention, setLogRetention] = useState("");
  const [dirty, setDirty] = useState(false);

  const load = useCallback(async () => {
    try {
      const [s, g] = await Promise.all([
        api.getGatewayStatus(),
        api.getGatewaySettings(),
      ]);
      setStatus(s);
      setSettings(g);
      setHost(g.host);
      setPort(String(g.port));
      setInputProtocol(g.input_protocol);
      setOutputProtocol(g.output_protocol);
      setAutoStart(g.auto_start);
      setLogRetention(String(g.log_retention_days));
      setDirty(false);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const markDirty = () => setDirty(true);

  const handleSave = async () => {
    const portNum = parseInt(port, 10);
    const retentionNum = parseInt(logRetention, 10);
    if (isNaN(portNum) || portNum < 1 || portNum > 65535) {
      toast("error", t("gateway.invalid_port"));
      return;
    }
    if (isNaN(retentionNum) || retentionNum < 1) {
      toast("error", t("gateway.invalid_retention"));
      return;
    }
    try {
      await api.updateGatewaySettings({
        host,
        port: portNum,
        input_protocol: inputProtocol,
        output_protocol: outputProtocol,
        auto_start: autoStart,
        log_retention_days: retentionNum,
      });
      toast("success", t("gateway.settings_saved"));
      load();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleStart = async () => {
    try {
      const s = await api.startGateway();
      setStatus(s);
      toast("success", t("gateway.started"));
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleStop = async () => {
    try {
      const s = await api.stopGateway();
      setStatus(s);
      toast("success", t("gateway.stopped"));
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleRestart = async () => {
    try {
      const s = await api.restartGateway();
      setStatus(s);
      toast("success", t("gateway.restarted"));
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  if (!status || !settings) {
    return <p className="text-xs text-text-muted">{t("common.loading")}</p>;
  }

  const startedAt = status.started_at
    ? new Date(status.started_at).toLocaleTimeString()
    : null;
  const listenUrl = `http://${status.host}:${status.port}`;

  return (
    <div className="space-y-4">
      {/* ── 1. Status strip — endpoint, active provider, started_at, controls.
              Topbar already shows running/stopped + host:port. Status badge
              + decorative big icon dropped. ── */}
      <div className="rounded-xl border border-border bg-card px-5 py-4" style={{ boxShadow: "var(--shadow-sm)" }}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1.5 text-xs">
            <span className="flex items-center gap-2 text-text-primary">
              <Radio className="h-3.5 w-3.5 text-accent" />
              <span className="font-medium">{t("gateway.local_gateway")}</span>
            </span>
            <span className="text-text-muted/40">·</span>
            <a
              href={listenUrl}
              className="font-mono text-accent hover:underline"
              target="_blank"
              rel="noreferrer"
            >
              {listenUrl}
            </a>
            <span className="text-text-muted/40">·</span>
            <span className="text-text-secondary">
              <span className="text-text-muted">{t("gateway.active_provider")} </span>
              <span className="text-text-primary">{status.active_provider ?? t("common.none")}</span>
            </span>
            {startedAt && (
              <>
                <span className="text-text-muted/40">·</span>
                <span className="text-text-secondary">
                  <span className="text-text-muted">{t("gateway.started_at")} </span>
                  <span className="font-mono text-text-primary">{startedAt}</span>
                </span>
              </>
            )}
          </div>
          <div className="flex items-center gap-2">
            {status.running ? (
              <>
                <button
                  onClick={handleStop}
                  className="flex items-center gap-1.5 rounded-md bg-error-soft px-2.5 py-1 text-xs font-medium text-error transition-colors hover:bg-error/20"
                >
                  <Square className="h-3 w-3" />
                  {t("gateway.stop")}
                </button>
                <button
                  onClick={handleRestart}
                  className="flex items-center gap-1.5 rounded-md bg-warning-soft px-2.5 py-1 text-xs font-medium text-warning transition-colors hover:bg-warning/20"
                >
                  <RotateCcw className="h-3 w-3" />
                  {t("gateway.restart")}
                </button>
              </>
            ) : (
              <button
                onClick={handleStart}
                className="flex items-center gap-1.5 rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-accent/90"
              >
                <Play className="h-3 w-3" />
                {t("gateway.start")}
              </button>
            )}
          </div>
        </div>
        {dirty && status.running && (
          <p className="mt-2 text-[11px] text-warning">{t("gateway.settings_changed")}</p>
        )}
      </div>

      {/* ── 2. Configuration — the editable settings ── */}
      <div className="rounded-xl border border-border bg-card p-5" style={{ boxShadow: "var(--shadow-sm)" }}>
        <div className="mb-4 flex items-center justify-between">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-text-primary">
            <Settings className="h-4 w-4 text-text-muted" />
            {t("gateway.configuration")}
          </h3>
          <button
            onClick={handleSave}
            disabled={!dirty}
            className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-accent/90 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-accent"
          >
            <Save className="h-3.5 w-3.5" />
            {t("gateway.save")}
          </button>
        </div>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          <SettingsField label={t("gateway.listen_address")}>
            <input
              value={host}
              onChange={(e) => { setHost(e.target.value); markDirty(); }}
              className="form-input"
            />
          </SettingsField>
          <SettingsField label={t("gateway.port")}>
            <input
              type="number"
              value={port}
              onChange={(e) => { setPort(e.target.value); markDirty(); }}
              className="form-input"
            />
          </SettingsField>
          <SettingsField label={t("gateway.log_retention")}>
            <input
              type="number"
              value={logRetention}
              onChange={(e) => { setLogRetention(e.target.value); markDirty(); }}
              min={1}
              className="form-input"
            />
          </SettingsField>
          <SettingsField label={t("gateway.input_protocol")}>
            <select
              value={inputProtocol}
              onChange={(e) => { setInputProtocol(e.target.value); markDirty(); }}
              className="form-input"
            >
              {PROTOCOLS.map((p) => (
                <option key={p.value} value={p.value}>{p.label}</option>
              ))}
            </select>
          </SettingsField>
          <SettingsField label={t("gateway.output_protocol")}>
            <select
              value={outputProtocol}
              onChange={(e) => { setOutputProtocol(e.target.value); markDirty(); }}
              className="form-input"
            >
              {PROTOCOLS.map((p) => (
                <option key={p.value} value={p.value}>{p.label}</option>
              ))}
            </select>
          </SettingsField>
          <SettingsField label={t("gateway.auto_start")}>
            <label className="mt-1 flex cursor-pointer items-center gap-2">
              <input
                type="checkbox"
                checked={autoStart}
                onChange={(e) => { setAutoStart(e.target.checked); markDirty(); }}
                className="accent-accent"
              />
              <span className="text-xs text-text-secondary">
                {autoStart ? t("providers.enabled") : t("providers.disabled")}
              </span>
            </label>
          </SettingsField>
        </div>
      </div>

      {/* ── 3. Route reference — what the gateway exposes ── */}
      <div className="rounded-xl border border-border bg-card p-5" style={{ boxShadow: "var(--shadow-sm)" }}>
        <h3 className="mb-3 text-sm font-semibold text-text-primary">
          {t("gateway.route_modes")}
        </h3>
        <div className="grid grid-cols-1 gap-1.5 lg:grid-cols-2">
          {ROUTE_REFERENCE.map((r) => (
            <div
              key={r.path}
              className="flex items-center justify-between rounded-md border border-border/50 bg-card-secondary px-3 py-1.5 text-xs"
            >
              <div className="flex min-w-0 items-center gap-2">
                <span className="w-10 shrink-0 rounded bg-bg px-1.5 py-0.5 text-center font-mono text-[10px] text-text-muted">
                  {r.method}
                </span>
                <span className="truncate font-mono text-text-primary">{r.path}</span>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                {r.detail && (
                  <span className="hidden text-[10px] text-text-muted lg:inline">{r.detail}</span>
                )}
                <span className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${
                  r.mode === "pass-through"
                    ? "bg-accent-soft text-accent"
                    : r.mode === "transform"
                      ? "bg-warning-soft text-warning"
                      : "bg-hover text-text-muted"
                }`}>
                  {r.mode}
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

const ROUTE_REFERENCE: { method: string; path: string; mode: string; detail?: string }[] = [
  { method: "GET", path: "/health", mode: "internal" },
  { method: "GET", path: "/v1/models", mode: "internal" },
  { method: "POST", path: "/v1/responses", mode: "transform", detail: "Responses → Chat Completions" },
  { method: "POST", path: "/responses", mode: "transform", detail: "alias" },
  { method: "POST", path: "/v1/chat/completions", mode: "pass-through", detail: "Chat Completions → Chat Completions" },
  { method: "POST", path: "/chat/completions", mode: "pass-through", detail: "alias" },
  { method: "POST", path: "/v1/messages", mode: "transform", detail: "Anthropic Messages → Chat Completions" },
  { method: "POST", path: "/v1beta/models/{model}:generateContent", mode: "transform", detail: "Gemini → Chat Completions" },
];

function SettingsField({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="mb-1 block text-xs font-medium text-text-secondary">
        {label}
      </label>
      {children}
    </div>
  );
}
