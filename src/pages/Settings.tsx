import { useState, useEffect, useCallback } from "react";
import { Shield, FolderOpen, RefreshCcw, Download } from "lucide-react";
import { check } from "@tauri-apps/plugin-updater";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { toast } from "@/components/common/Toast";
import { useI18n, type Locale } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { GatewaySettings as GatewaySettingsType } from "@/types/gateway";
import type { GatewayAuthSettings } from "@/types/config";

export function Settings() {
  const { t, locale, setLocale } = useI18n();
  const [settings, setSettings] = useState<GatewaySettingsType | null>(null);
  const [auth, setAuth] = useState<GatewayAuthSettings | null>(null);
  const [confirmRegen, setConfirmRegen] = useState(false);

  const load = useCallback(async () => {
    try {
      const [s, a] = await Promise.all([
        api.getGatewaySettings(),
        api.getGatewayAuthSettings(),
      ]);
      setSettings(s);
      setAuth(a);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleUpdateRetention = async (days: number) => {
    try {
      await api.updateGatewaySettings({ log_retention_days: days });
      toast("success", t("settings.updated"));
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleUpdateAutoStart = async (val: boolean) => {
    try {
      await api.updateGatewaySettings({ auto_start: val });
      toast("success", t("settings.updated"));
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleRegenToken = async () => {
    try {
      const a = await api.regenerateLocalAccessToken();
      setAuth(a);
      toast("success", t("settings.regen_done"));
      setConfirmRegen(false);
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  if (!settings) return <p className="text-xs text-text-muted">{t("common.loading")}</p>;

  return (
    <div className="space-y-6">
      {/* Gateway Security */}
      {auth && (
        <section className="rounded-lg border border-border bg-card p-5">
          <h3 className="mb-4 flex items-center gap-2 text-sm font-semibold text-text-primary">
            <Shield className="h-4 w-4 text-accent" />{t("settings.gateway_security")}
          </h3>
          <div className="space-y-3 text-xs">
            <div className="flex justify-between"><span className="text-text-muted">{t("settings.auth_mode")}</span><span className="text-text-primary">{auth.auth_mode}</span></div>
            <div className="flex justify-between"><span className="text-text-muted">{t("settings.token_path")}</span><span className="font-mono text-text-secondary text-[11px]">{auth.token_path}</span></div>
            <div className="flex justify-between"><span className="text-text-muted">{t("settings.local_token")}</span><span className="font-mono text-text-secondary">{auth.masked_token}</span></div>
            <div className="flex justify-between"><span className="text-text-muted">{t("settings.codex_auth")}</span><span className="text-text-primary">{auth.codex_auth_type}</span></div>
            <div className="flex justify-between"><span className="text-text-muted">{t("settings.claude_auth")}</span><span className="text-text-primary">{auth.claude_code_auth_type}</span></div>
          </div>
          <div className="mt-4 flex gap-2">
            <button onClick={() => setConfirmRegen(true)} className="btn-secondary"><RefreshCcw className="h-3 w-3" />{t("settings.regenerate_token")}</button>
            <button onClick={() => api.openTokenFolder()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("settings.open_token_folder")}</button>
          </div>
        </section>
      )}

      {/* General */}
      <section className="rounded-lg border border-border bg-card p-5">
        <h3 className="mb-4 text-sm font-semibold text-text-primary">{t("settings.general")}</h3>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm text-text-primary">{t("settings.auto_start_gateway")}</p>
              <p className="text-xs text-text-muted">{t("settings.auto_start_desc")}</p>
            </div>
            <ToggleSwitch checked={settings.auto_start} onChange={handleUpdateAutoStart} />
          </div>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm text-text-primary">{t("settings.language")}</p>
              <p className="text-xs text-text-muted">{t("settings.lang_desc")}</p>
            </div>
            <select
              value={locale}
              onChange={(e) => setLocale(e.target.value as Locale)}
              className="rounded-md border border-border bg-card-secondary px-3 py-1.5 text-xs text-text-primary outline-none focus:border-accent"
            >
              <option value="en">English</option>
              <option value="zh">中文</option>
            </select>
          </div>
        </div>
      </section>

      {/* Gateway */}
      <section className="rounded-lg border border-border bg-card p-5">
        <h3 className="mb-4 text-sm font-semibold text-text-primary">{t("settings.gateway")}</h3>
        <div className="space-y-4">
          <SettingsRow label={t("gateway.listen_address")} value={settings.host} />
          <SettingsRow label={t("gateway.port")} value={String(settings.port)} />
          <SettingsRow label={t("gateway.input_protocol")} value={settings.input_protocol} />
          <SettingsRow label={t("gateway.output_protocol")} value={settings.output_protocol} />
        </div>
      </section>

      {/* Data */}
      <section className="rounded-lg border border-border bg-card p-5">
        <h3 className="mb-4 text-sm font-semibold text-text-primary">{t("settings.data")}</h3>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-text-primary">{t("settings.log_retention")}</p>
            <p className="text-xs text-text-muted">{t("settings.log_retention_desc")}</p>
          </div>
          <select value={settings.log_retention_days} onChange={(e) => handleUpdateRetention(parseInt(e.target.value, 10))} className="rounded-md border border-border bg-card-secondary px-3 py-1.5 text-xs text-text-primary outline-none focus:border-accent">
            <option value={7}>7 {t("common.days")}</option>
            <option value={14}>14 {t("common.days")}</option>
            <option value={30}>30 {t("common.days")}</option>
            <option value={90}>90 {t("common.days")}</option>
          </select>
        </div>
      </section>

      {/* About */}
      <section className="rounded-lg border border-border bg-card p-5">
        <h3 className="mb-4 text-sm font-semibold text-text-primary">{t("settings.about")}</h3>
        <div className="space-y-2 text-xs">
          <div className="flex justify-between"><span className="text-text-muted">{t("settings.version")}</span><span className="text-text-primary">0.1.1</span></div>
          <div className="flex justify-between"><span className="text-text-muted">{t("settings.license")}</span><span className="text-text-primary">MIT</span></div>
        </div>
        <div className="mt-4 flex gap-2">
          <CheckUpdateButton t={t} />
          <a
            href="https://github.com/dengmengmian/AgentGate"
            target="_blank"
            rel="noopener noreferrer"
            className="btn-secondary"
          >
            <svg className="h-3 w-3" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/></svg>
            GitHub
          </a>
        </div>
      </section>

      <ConfirmDialog open={confirmRegen} title={t("settings.regen_title")} message={t("settings.regen_msg")} confirmLabel={t("settings.regenerate_token")} variant="danger" onConfirm={handleRegenToken} onCancel={() => setConfirmRegen(false)} />
    </div>
  );
}

function ToggleSwitch({ checked, onChange }: { checked: boolean; onChange: (val: boolean) => void }) {
  return (
    <label className="relative inline-flex cursor-pointer items-center">
      <input type="checkbox" className="peer sr-only" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <div className="h-5 w-9 rounded-full bg-border transition-colors after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-text-muted after:transition-all peer-checked:bg-accent peer-checked:after:translate-x-full peer-checked:after:bg-white" />
    </label>
  );
}

function SettingsRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-sm text-text-primary">{label}</span>
      <span className="font-mono text-xs text-text-secondary">{value}</span>
    </div>
  );
}

function CheckUpdateButton({ t }: { t: (key: string) => string }) {
  const [checking, setChecking] = useState(false);
  const [status, setStatus] = useState<"idle" | "latest" | "available">("idle");
  const [newVersion, setNewVersion] = useState("");

  const handleCheck = async () => {
    setChecking(true);
    setStatus("idle");
    try {
      const update = await check();
      if (update) {
        setStatus("available");
        setNewVersion(update.version);
      } else {
        setStatus("latest");
      }
    } catch {
      toast("error", t("update.check_failed"));
    } finally {
      setChecking(false);
    }
  };

  const handleInstall = async () => {
    setInstalling(true);
    try {
      const update = await check();
      if (!update) return;
      await update.downloadAndInstall();
      setStatus("latest");
    } catch {
      setInstalling(false);
      toast("error", t("update.install_failed"));
    }
  };

  const [installing, setInstalling] = useState(false);

  return (
    <div className="flex items-center gap-2">
      {status === "available" ? (
        <button onClick={handleInstall} disabled={installing} className="btn-primary">
          <Download className="h-3 w-3" />
          {installing ? t("update.installing") : `${t("update.now")} v${newVersion}`}
        </button>
      ) : (
        <button onClick={handleCheck} disabled={checking} className="btn-secondary">
          <RefreshCcw className={`h-3 w-3 ${checking ? "animate-spin" : ""}`} />
          {checking ? t("update.checking") : t("update.check")}
        </button>
      )}
      {status === "latest" && (
        <span className="text-xs text-green-400">{t("update.is_latest")}</span>
      )}
    </div>
  );
}
