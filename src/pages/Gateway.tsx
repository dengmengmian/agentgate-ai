import { useState, useEffect, useCallback } from "react";
import { Radio, Play, Square, RotateCcw, Settings, Save } from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
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

  return (
    <div className="space-y-6">
      {/* Status Card */}
      <div className="rounded-lg border border-border bg-card p-6">
        <div className="mb-6 flex items-center justify-between">
          <div className="flex items-center gap-4">
            <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-accent/10">
              <Radio className="h-6 w-6 text-accent" />
            </div>
            <div>
              <h2 className="text-lg font-semibold text-text-primary">
                {t("gateway.local_gateway")}
              </h2>
              <p className="text-xs text-text-muted">
                {t("gateway.protocol_conversion")}
              </p>
            </div>
          </div>
          <StatusBadge variant={status.running ? "success" : "muted"}>
            {status.running ? t("topbar.running") : t("topbar.stopped")}
          </StatusBadge>
        </div>

        {/* Listening status */}
        {status.running && (
          <p className="mb-4 text-xs text-success">
            {t("gateway.listening")} http://{status.host}:{status.port}
          </p>
        )}
        {!status.running && (
          <p className="mb-4 text-xs text-text-muted">{t("gateway.not_listening")}</p>
        )}
        {dirty && status.running && (
          <p className="mb-4 text-xs text-warning">
            {t("gateway.settings_changed")}
          </p>
        )}

        {/* Controls */}
        <div className="mb-6 flex gap-3">
          {status.running ? (
            <>
              <button
                onClick={handleStop}
                className="flex items-center gap-2 rounded-md bg-error/10 px-4 py-2 text-xs font-medium text-error transition-colors hover:bg-error/20"
              >
                <Square className="h-3.5 w-3.5" />
                {t("gateway.stop")}
              </button>
              <button
                onClick={handleRestart}
                className="flex items-center gap-2 rounded-md bg-warning/10 px-4 py-2 text-xs font-medium text-warning transition-colors hover:bg-warning/20"
              >
                <RotateCcw className="h-3.5 w-3.5" />
                {t("gateway.restart")}
              </button>
            </>
          ) : (
            <button
              onClick={handleStart}
              className="flex items-center gap-2 rounded-md bg-accent px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-accent/90"
            >
              <Play className="h-3.5 w-3.5" />
              {t("gateway.start")}
            </button>
          )}
        </div>

        {/* Status info */}
        <div className="grid grid-cols-2 gap-x-6 gap-y-3 text-xs">
          <div className="flex justify-between border-b border-border/50 pb-2">
            <span className="text-text-muted">{t("gateway.active_provider")}</span>
            <span className="text-text-primary">
              {status.active_provider ?? t("common.none")}
            </span>
          </div>
          <div className="flex justify-between border-b border-border/50 pb-2">
            <span className="text-text-muted">{t("gateway.started_at")}</span>
            <span className="font-mono text-text-primary">
              {status.started_at
                ? new Date(status.started_at).toLocaleTimeString()
                : "—"}
            </span>
          </div>
        </div>

        {/* Compatibility info */}
        <div className="mt-4 flex flex-wrap gap-3 text-[11px]">
          <span className="rounded-md bg-card-secondary px-2.5 py-1 text-text-secondary">
            {t("gateway.codex_compat")}: <span className="text-success">{t("gateway.enabled")}</span>
          </span>
          <span className="rounded-md bg-card-secondary px-2.5 py-1 text-text-secondary">
            {t("gateway.tool_call_streaming")}: <span className="text-success">{t("gateway.enabled")}</span>
          </span>
          <span className="rounded-md bg-card-secondary px-2.5 py-1 text-text-secondary">
            {t("gateway.deepseek_cleaner")}: <span className="text-success">{t("gateway.enabled")}</span>
          </span>
        </div>
      </div>

      {/* Route Modes */}
      <div className="rounded-lg border border-border bg-card p-6">
        <h3 className="mb-4 text-sm font-semibold text-text-primary">
          {t("gateway.route_modes")}
        </h3>
        <div className="space-y-2">
          {[
            { method: "GET", path: "/health", mode: "internal" },
            { method: "GET", path: "/v1/models", mode: "internal" },
            { method: "POST", path: "/v1/responses", mode: "transform", detail: "Responses → Chat Completions" },
            { method: "POST", path: "/responses", mode: "transform", detail: "alias" },
            { method: "POST", path: "/v1/chat/completions", mode: "pass-through", detail: "Chat Completions → Chat Completions" },
            { method: "POST", path: "/chat/completions", mode: "pass-through", detail: "alias" },
          ].map((r) => (
            <div
              key={r.path}
              className="flex items-center justify-between rounded-md border border-border/50 bg-card-secondary px-4 py-2 text-xs"
            >
              <div className="flex items-center gap-3">
                <span className="w-10 rounded bg-bg px-1.5 py-0.5 text-center font-mono text-[10px] text-text-muted">
                  {r.method}
                </span>
                <span className="font-mono text-text-primary">{r.path}</span>
              </div>
              <div className="flex items-center gap-2">
                {r.detail && (
                  <span className="text-text-muted">{r.detail}</span>
                )}
                <span className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${
                  r.mode === "pass-through"
                    ? "bg-accent/10 text-accent"
                    : r.mode === "transform"
                      ? "bg-warning/10 text-warning"
                      : "bg-text-muted/10 text-text-muted"
                }`}>
                  {r.mode}
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Settings */}
      <div className="rounded-lg border border-border bg-card p-6">
        <div className="mb-4 flex items-center justify-between">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-text-primary">
            <Settings className="h-4 w-4 text-text-muted" />
            {t("gateway.configuration")}
          </h3>
          {dirty && (
            <button
              onClick={handleSave}
              className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-accent/90"
            >
              <Save className="h-3.5 w-3.5" />
              {t("gateway.save")}
            </button>
          )}
        </div>

        <div className="grid grid-cols-2 gap-4">
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
          <SettingsField label={t("gateway.log_retention")}>
            <input
              type="number"
              value={logRetention}
              onChange={(e) => { setLogRetention(e.target.value); markDirty(); }}
              min={1}
              className="form-input"
            />
          </SettingsField>
        </div>
      </div>
    </div>
  );
}

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
