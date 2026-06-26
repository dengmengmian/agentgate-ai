import { useState, useEffect } from "react";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import type { Locale } from "@/lib/i18n";
import type { GatewaySettings as GatewaySettingsType } from "@/types/gateway";
import { toast } from "@/components/common/Toast";

interface Props {
  settings: GatewaySettingsType;
  locale: Locale;
  setLocale: (l: Locale) => void;
  theme: string;
  setTheme: (t: string) => void;
  handleUpdateAutoStart: (val: boolean) => Promise<void>;
  handleUpdateRefinerGlobal: (
    key:
      | "body_filter_global"
      | "thinking_rectifier_global"
      | "error_mapper_global"
      | "health_probe_enabled",
    val: boolean
  ) => Promise<void>;
  handleUpdateCostAlert: (patch: {
    cost_alert_enabled?: boolean;
    cost_alert_threshold?: number;
  }) => Promise<void>;
  handleUpdateRequestBodyLimit: (mb: number) => Promise<void>;
  t: (key: string) => string;
  ToggleSwitch: React.ComponentType<{
    checked: boolean;
    onChange: (val: boolean) => void;
  }>;
  ThemePicker: React.ComponentType<{
    value: string;
    onChange: (id: string) => void;
  }>;
}

export function GeneralTab({
  settings,
  locale,
  setLocale,
  theme,
  setTheme,
  handleUpdateAutoStart,
  handleUpdateRefinerGlobal,
  handleUpdateCostAlert,
  handleUpdateRequestBodyLimit,
  t,
  ToggleSwitch,
  ThemePicker,
}: Props) {
  const [launchAtLogin, setLaunchAtLogin] = useState(false);

  useEffect(() => {
    isEnabled()
      .then(setLaunchAtLogin)
      .catch(() => {});
  }, []);

  const handleToggleLaunchAtLogin = async (val: boolean) => {
    try {
      if (val) await enable();
      else await disable();
      setLaunchAtLogin(val);
    } catch (e) {
      toast("error", String(e));
    }
  };

  return (
    <section className="rounded-xl border border-border bg-card p-5">
      <h3 className="mb-4 text-sm font-semibold text-text-primary">
        {t("settings.general")}
      </h3>
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-text-primary">
              {t("settings.auto_start_gateway")}
            </p>
            <p className="text-xs text-text-muted">
              {t("settings.auto_start_desc")}
            </p>
          </div>
          <ToggleSwitch
            checked={settings.auto_start}
            onChange={handleUpdateAutoStart}
          />
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-text-primary">
              {t("settings.launch_at_login")}
            </p>
            <p className="text-xs text-text-muted">
              {t("settings.launch_at_login_desc")}
            </p>
          </div>
          <ToggleSwitch
            checked={launchAtLogin}
            onChange={handleToggleLaunchAtLogin}
          />
        </div>
        <div className="flex items-center justify-between">
          <div className="flex-1 pr-4">
            <p className="text-sm text-text-primary">
              {t("settings.request_body_limit")}
            </p>
            <p className="text-xs text-text-muted">
              {t("settings.request_body_limit_desc")}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <input
              type="number"
              min="1"
              max="128"
              step="1"
              defaultValue={settings.request_body_limit_mb}
              onBlur={(e) => {
                const v = Math.floor(Number(e.target.value));
                if (
                  Number.isFinite(v) &&
                  v > 0 &&
                  v <= 128 &&
                  v !== settings.request_body_limit_mb
                ) {
                  handleUpdateRequestBodyLimit(v);
                }
              }}
              className="form-input w-24"
            />
            <span className="text-sm text-text-muted">MB</span>
          </div>
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-text-primary">
              {t("settings.language")}
            </p>
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
        <div>
          <p className="text-sm text-text-primary">{t("settings.theme")}</p>
          <p className="mb-3 text-xs text-text-muted">
            {t("settings.theme_desc")}
          </p>
          <ThemePicker value={theme} onChange={setTheme} />
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-text-primary">
              {t("settings.show_quick_setup")}
            </p>
            <p className="text-xs text-text-muted">
              {t("settings.show_quick_setup_desc")}
            </p>
          </div>
          <ToggleSwitch
            checked={localStorage.getItem("agentgate_show_quick_setup") === "1"}
            onChange={(val) => {
              if (val) {
                localStorage.setItem("agentgate_show_quick_setup", "1");
                localStorage.removeItem("agentgate_hide_quick_setup");
              } else {
                localStorage.removeItem("agentgate_show_quick_setup");
                localStorage.setItem("agentgate_hide_quick_setup", "1");
              }
              window.location.reload();
            }}
          />
        </div>

        {/* 网关精炼层全局总闸——默认全关 = 字节级透明 pass-through */}
        <div className="border-t border-border pt-4">
          <p className="text-sm font-medium text-text-primary">
            {t("settings.refiner")}
          </p>
          <p className="mb-3 text-xs text-text-muted">
            {t("settings.refiner_desc")}
          </p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">
                  {t("settings.body_filter")}
                </p>
                <p className="text-xs text-text-muted">
                  {t("settings.body_filter_desc")}
                </p>
              </div>
              <ToggleSwitch
                checked={settings.body_filter_global}
                onChange={(v) =>
                  handleUpdateRefinerGlobal("body_filter_global", v)
                }
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">
                  {t("settings.thinking_rectifier")}
                </p>
                <p className="text-xs text-text-muted">
                  {t("settings.thinking_rectifier_desc")}
                </p>
              </div>
              <ToggleSwitch
                checked={settings.thinking_rectifier_global}
                onChange={(v) =>
                  handleUpdateRefinerGlobal("thinking_rectifier_global", v)
                }
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">
                  {t("settings.error_mapper")}
                </p>
                <p className="text-xs text-text-muted">
                  {t("settings.error_mapper_desc")}
                </p>
              </div>
              <ToggleSwitch
                checked={settings.error_mapper_global}
                onChange={(v) =>
                  handleUpdateRefinerGlobal("error_mapper_global", v)
                }
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">
                  {t("settings.health_probe")}
                </p>
                <p className="text-xs text-text-muted">
                  {t("settings.health_probe_desc")}
                </p>
              </div>
              <ToggleSwitch
                checked={settings.health_probe_enabled}
                onChange={(v) =>
                  handleUpdateRefinerGlobal("health_probe_enabled", v)
                }
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">
                  {t("settings.cost_alert")}
                </p>
                <p className="text-xs text-text-muted">
                  {t("settings.cost_alert_desc")}
                </p>
              </div>
              <ToggleSwitch
                checked={settings.cost_alert_enabled}
                onChange={(v) =>
                  handleUpdateCostAlert({ cost_alert_enabled: v })
                }
              />
            </div>
            {settings.cost_alert_enabled && (
              <div className="flex items-center justify-between">
                <div className="flex-1 pr-4">
                  <p className="text-sm text-text-primary">
                    {t("settings.cost_alert_threshold")}
                  </p>
                  <p className="text-xs text-text-muted">
                    {t("settings.cost_alert_threshold_desc")}
                  </p>
                </div>
                <div className="flex items-center gap-1">
                  <span className="text-sm text-text-muted">$</span>
                  <input
                    type="number"
                    min="0"
                    step="0.5"
                    defaultValue={settings.cost_alert_threshold ?? ""}
                    onBlur={(e) => {
                      const v = parseFloat(e.target.value);
                      if (
                        !Number.isNaN(v) &&
                        v > 0 &&
                        v !== settings.cost_alert_threshold
                      ) {
                        handleUpdateCostAlert({ cost_alert_threshold: v });
                      }
                    }}
                    placeholder="10"
                    className="form-input w-24"
                  />
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
