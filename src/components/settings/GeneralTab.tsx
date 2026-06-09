import type { Locale } from "@/lib/i18n";
import type { GatewaySettings as GatewaySettingsType } from "@/types/gateway";

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
    val: boolean,
  ) => Promise<void>;
  t: (key: string) => string;
  ToggleSwitch: React.ComponentType<{ checked: boolean; onChange: (val: boolean) => void }>;
  ThemePicker: React.ComponentType<{ value: string; onChange: (id: string) => void }>;
}

export function GeneralTab({
  settings, locale, setLocale, theme, setTheme,
  handleUpdateAutoStart, handleUpdateRefinerGlobal, t,
  ToggleSwitch, ThemePicker,
}: Props) {
  return (
    <section className="rounded-xl border border-border bg-card p-5">
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
        <div>
          <p className="text-sm text-text-primary">{t("settings.theme")}</p>
          <p className="mb-3 text-xs text-text-muted">{t("settings.theme_desc")}</p>
          <ThemePicker value={theme} onChange={setTheme} />
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-text-primary">{t("settings.show_quick_setup")}</p>
            <p className="text-xs text-text-muted">{t("settings.show_quick_setup_desc")}</p>
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
          <p className="text-sm font-medium text-text-primary">网关精炼层 (Refiner)</p>
          <p className="mb-3 text-xs text-text-muted">
            默认全部关闭，网关原样转发请求。打开后会按每个 provider 的 quirks 配置改写请求 / 响应。每个 provider 还能单独覆写为强制关。
          </p>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">请求字段过滤</p>
                <p className="text-xs text-text-muted">按 Quirks 剥不支持的请求字段，避免 400 错误</p>
              </div>
              <ToggleSwitch
                checked={settings.body_filter_global}
                onChange={(v) => handleUpdateRefinerGlobal("body_filter_global", v)}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">推理参数校正</p>
                <p className="text-xs text-text-muted">thinking.budget_tokens / reasoning.effort 自动归一</p>
              </div>
              <ToggleSwitch
                checked={settings.thinking_rectifier_global}
                onChange={(v) => handleUpdateRefinerGlobal("thinking_rectifier_global", v)}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">错误响应归一</p>
                <p className="text-xs text-text-muted">provider 错误结构改写成客户端协议期望的形态</p>
              </div>
              <ToggleSwitch
                checked={settings.error_mapper_global}
                onChange={(v) => handleUpdateRefinerGlobal("error_mapper_global", v)}
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="flex-1 pr-4">
                <p className="text-sm text-text-primary">主动健康探测</p>
                <p className="text-xs text-text-muted">每 10 分钟对启用的供应商发 1-token 探测，结果显示在供应商卡片（仅展示，不影响路由）。会消耗少量额度。</p>
              </div>
              <ToggleSwitch
                checked={settings.health_probe_enabled}
                onChange={(v) => handleUpdateRefinerGlobal("health_probe_enabled", v)}
              />
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
