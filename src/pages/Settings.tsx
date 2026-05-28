import { useState, useEffect, useCallback } from "react";
import { Shield, FolderOpen, RefreshCcw, Download, Copy, DollarSign, Plus, Trash2, Settings2, Database, Info, PawPrint, ChevronDown, ChevronRight } from "lucide-react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { toast } from "@/components/common/Toast";
import { useI18n, type Locale } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { GatewaySettings as GatewaySettingsType } from "@/types/gateway";
import type { GatewayAuthSettings } from "@/types/config";
import type { ModelPricing } from "@/types/stats";
import type { PetType, PetSettings as PetSettingsType } from "@/types/pet";
import { RobotPet } from "@/pet/pets/RobotPet";
import { PixelCat } from "@/pet/pets/PixelCat";
import { SlimePet } from "@/pet/pets/SlimePet";
import { FoxPet } from "@/pet/pets/FoxPet";
import { OctopusPet } from "@/pet/pets/OctopusPet";
import { GhostPet } from "@/pet/pets/GhostPet";
import { OxPet } from "@/pet/pets/OxPet";
import { SuperSoldierPet } from "@/pet/pets/SuperSoldierPet";
import { CoderPet } from "@/pet/pets/CoderPet";

type Tab = "general" | "security" | "data" | "pet" | "about";

// Gateway 标签曾经在这里，但与顶级"服务"页面（src/pages/Gateway.tsx）
// 语义重叠（都是 host/port/protocol 配置），且这里是只读、那边可改——
// 用户混淆。删掉这里的 tab，统一去顶级"服务"页编辑。
const TABS: { id: Tab; icon: React.ComponentType<{ className?: string }> }[] = [
  { id: "general", icon: Settings2 },
  { id: "security", icon: Shield },
  { id: "data", icon: Database },
  { id: "pet", icon: PawPrint },
  { id: "about", icon: Info },
];

export function Settings() {
  const { t, locale, setLocale } = useI18n();
  const [tab, setTab] = useState<Tab>("general");
  const [theme, setThemeState] = useState(() => localStorage.getItem("agentgate_theme") || "dark");
  const [settings, setSettings] = useState<GatewaySettingsType | null>(null);
  const [auth, setAuth] = useState<GatewayAuthSettings | null>(null);
  const [confirmRegen, setConfirmRegen] = useState(false);
  const [pricing, setPricing] = useState<ModelPricing[]>([]);
  const [petSettings, setPetSettings] = useState<PetSettingsType | null>(null);
  const [appVersion, setAppVersion] = useState("");
  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);

  const load = useCallback(async () => {
    try {
      const [s, a, p, pet] = await Promise.all([
        api.getGatewaySettings(),
        api.getGatewayAuthSettings(),
        api.listModelPricing(),
        api.getPetSettings(),
      ]);
      setSettings(s);
      setAuth(a);
      setPricing(p);
      setPetSettings(pet);
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

  const handleCopyToken = async () => {
    try {
      const token = await api.getLocalAccessToken();
      await navigator.clipboard.writeText(token);
      toast("success", t("settings.token_copied"));
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

  const handlePetTypeChange = async (type: PetType) => {
    try {
      const updated = await api.updatePetSettings({ pet_type: type });
      setPetSettings(updated);
      toast("success", t("settings.updated"));
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handlePetVisibleChange = async (visible: boolean) => {
    try {
      const updated = await api.setPetVisible(visible);
      setPetSettings(updated);
      toast("success", t("settings.updated"));
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const setTheme = (t: string) => {
    setThemeState(t);
    if (t === "dark") {
      document.documentElement.removeAttribute("data-theme");
      localStorage.removeItem("agentgate_theme");
    } else {
      document.documentElement.setAttribute("data-theme", t);
      localStorage.setItem("agentgate_theme", t);
    }
  };

  if (!settings) return (
    <div className="flex gap-6">
      <div className="w-44 shrink-0 space-y-2">
        {Array.from({ length: 6 }).map((_, i) => <div key={i} className="skeleton h-9 rounded-lg" />)}
      </div>
      <div className="flex-1 space-y-4">
        <div className="skeleton h-48 rounded-xl" />
      </div>
    </div>
  );

  return (
    <div className="flex gap-6 min-h-0">
      {/* Left Tab Navigation */}
      <nav className="w-44 shrink-0">
        <div className="space-y-1">
          {TABS.map(({ id, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setTab(id)}
              className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm transition-colors ${
                tab === id
                  ? "bg-accent-soft text-accent font-medium"
                  : "text-text-secondary hover:bg-hover hover:text-text-primary"
              }`}
            >
              <Icon className="h-4 w-4 shrink-0" />
              {t(`settings.tab.${id}`)}
            </button>
          ))}
        </div>
      </nav>

      {/* Right Content */}
      <div className="flex-1 min-w-0 space-y-6">
        {tab === "general" && (
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
            </div>
          </section>
        )}

        {tab === "security" && auth && (
          <section className="rounded-xl border border-border bg-card p-5">
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
              <button onClick={handleCopyToken} className="btn-secondary"><Copy className="h-3 w-3" />{t("settings.copy_token")}</button>
              <button onClick={() => setConfirmRegen(true)} className="btn-secondary"><RefreshCcw className="h-3 w-3" />{t("settings.regenerate_token")}</button>
              <button onClick={() => api.openTokenFolder()} className="btn-secondary"><FolderOpen className="h-3 w-3" />{t("settings.open_token_folder")}</button>
            </div>
          </section>
        )}

        {tab === "data" && (
          <>
            <section className="rounded-xl border border-border bg-card p-5">
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

            <ConfigBackupSection />


            {/* 模型定价表默认折叠——用户配 1 次就再也不动，平铺时挤压日常常看的
                日志保留 / 配置导入导出。点击 header 展开。 */}
            <CollapsibleSection
              icon={DollarSign}
              title={t("settings.model_pricing")}
              hint={t("settings.model_pricing_desc")}
              badge={`${pricing.length}`}
            >
              <div className="overflow-hidden rounded-md border border-border">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-border bg-card-secondary">
                      <th className="px-3 py-2 text-left font-medium text-text-muted">Provider</th>
                      <th className="px-3 py-2 text-left font-medium text-text-muted">Model</th>
                      <th className="px-3 py-2 text-right font-medium text-text-muted">Input ($/1M)</th>
                      <th className="px-3 py-2 text-right font-medium text-text-muted">Output ($/1M)</th>
                      <th className="px-3 py-2 text-center font-medium text-text-muted">{t("settings.source")}</th>
                      <th className="px-3 py-2 w-8"></th>
                    </tr>
                  </thead>
                  <tbody>
                    {pricing.map((p) => (
                      <PricingRow key={p.id} item={p} onUpdate={async (inputPrice, outputPrice) => {
                        try {
                          const updated = await api.upsertModelPricing(p.provider, p.model_pattern, inputPrice, outputPrice);
                          setPricing(pricing.map(x => x.id === p.id || x.id === updated.id ? updated : x).sort((a, b) => `${a.provider}${a.model_pattern}`.localeCompare(`${b.provider}${b.model_pattern}`)));
                          toast("success", t("settings.pricing_saved"));
                        } catch (err) { toast("error", (err as api.AppError).message); }
                      }} onDelete={async () => {
                        await api.deleteModelPricing(p.id);
                        setPricing(pricing.filter(x => x.id !== p.id));
                        toast("success", t("common.deleted"));
                      }} />
                    ))}
                  </tbody>
                </table>
              </div>

              <PricingAddForm onAdd={async (provider, model, inputPrice, outputPrice) => {
                try {
                  const p = await api.upsertModelPricing(provider, model, inputPrice, outputPrice);
                  setPricing([...pricing.filter(x => x.id !== p.id), p].sort((a, b) => `${a.provider}${a.model_pattern}`.localeCompare(`${b.provider}${b.model_pattern}`)));
                  toast("success", t("settings.pricing_saved"));
                } catch (err) { toast("error", (err as api.AppError).message); }
              }} />
            </CollapsibleSection>
          </>
        )}

        {tab === "pet" && petSettings && (
          <section className="rounded-xl border border-border bg-card p-5">
            <h3 className="mb-1 flex items-center gap-2 text-sm font-semibold text-text-primary">
              <PawPrint className="h-4 w-4 text-accent" />{t("settings.pet.title")}
            </h3>
            <p className="mb-5 text-xs text-text-muted">{t("settings.pet.desc")}</p>

            {/* Visibility toggle */}
            <div className="mb-6 flex items-center justify-between">
              <div>
                <p className="text-sm text-text-primary">{t("settings.pet.visible")}</p>
                <p className="text-xs text-text-muted">{t("settings.pet.visible_desc")}</p>
              </div>
              <ToggleSwitch checked={petSettings.visible} onChange={handlePetVisibleChange} />
            </div>

            {/* Pet type selection */}
            <div>
              <p className="mb-3 text-sm text-text-primary">{t("settings.pet.type")}</p>
              <p className="mb-4 text-xs text-text-muted">{t("settings.pet.type_desc")}</p>
              <div className="grid grid-cols-3 gap-3">
                {(["robot", "pixel-cat", "slime", "fox", "octopus", "ghost", "ox", "soldier", "coder"] as PetType[]).map((type) => (
                  <PetTypeCard
                    key={type}
                    type={type}
                    selected={petSettings.pet_type === type}
                    name={t(`settings.pet.${type}`)}
                    desc={t(`settings.pet.${type}_desc`)}
                    onClick={() => handlePetTypeChange(type)}
                  />
                ))}
              </div>
            </div>
          </section>
        )}

        {tab === "about" && (
          <section className="rounded-xl border border-border bg-card p-5">
            <h3 className="mb-4 text-sm font-semibold text-text-primary">{t("settings.about")}</h3>
            <div className="space-y-2 text-xs">
              <div className="flex justify-between"><span className="text-text-muted">{t("settings.version")}</span><span className="text-text-primary">{appVersion}</span></div>
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
        )}
      </div>

      <ConfirmDialog open={confirmRegen} title={t("settings.regen_title")} message={t("settings.regen_msg")} confirmLabel={t("settings.regenerate_token")} variant="danger" onConfirm={handleRegenToken} onCancel={() => setConfirmRegen(false)} />
    </div>
  );
}

// ── Pet Type Card ──

const PET_PREVIEWS: Record<PetType, React.ComponentType<{ state: "idle" }>> = {
  robot: RobotPet,
  "pixel-cat": PixelCat,
  slime: SlimePet,
  fox: FoxPet,
  octopus: OctopusPet,
  ghost: GhostPet,
  ox: OxPet,
  soldier: SuperSoldierPet,
  coder: CoderPet,
};

/// 配置导入/导出 section。语义是 replace（导入 = 覆盖），所以导入前用
/// ConfirmDialog 弹窗确认。API key 默认**不导出**——把含密钥的 JSON 文件
/// 随手发到群里/截图/丢仓库是常见泄密路径，所以默认安全；用户明确勾选
/// "含密钥"才会写入，仅用于本机迁移。
function ConfigBackupSection() {
  const { t: _t } = useI18n();
  const [includeSecrets, setIncludeSecrets] = useState(false);
  const [pendingImportJson, setPendingImportJson] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  const handleExport = async () => {
    try {
      const json = await api.exportConfigJson(includeSecrets);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const ts = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
      const a = document.createElement("a");
      a.href = url;
      a.download = `agentgate-config-${ts}${includeSecrets ? "-with-secrets" : ""}.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast("success", includeSecrets ? "已导出（含密钥）" : "已导出（不含密钥）");
    } catch (err) {
      toast("error", (err as api.AppError).message);
    }
  };

  const handleFilePicked = async (file: File) => {
    try {
      const text = await file.text();
      // 触发确认弹窗前先校验是 AgentGate 格式，避免误导入随机 JSON。
      JSON.parse(text);
      setPendingImportJson(text);
    } catch {
      toast("error", "文件不是合法 JSON");
    }
  };

  const handleConfirmImport = async () => {
    if (!pendingImportJson) return;
    setImporting(true);
    try {
      const summary = await api.importConfigJson(pendingImportJson);
      toast(
        "success",
        `已导入 ${summary.providers_imported} 个 provider、${summary.route_profiles_imported} 个 route profile${
          summary.secrets_applied ? "（含密钥）" : "（密钥需重新填写）"
        }`
      );
      setPendingImportJson(null);
      // 让上层数据刷新——最稳的是 reload 一次。
      window.location.reload();
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setImporting(false);
    }
  };

  return (
    <section className="rounded-xl border border-border bg-card p-5">
      <h3 className="mb-1 text-sm font-semibold text-text-primary">配置导入导出</h3>
      <p className="mb-4 text-xs text-text-muted">
        导出 providers + route profiles 为 JSON 文件，方便迁移机器或备份后再恢复。导入会
        <span className="text-warning">替换</span>
        当前所有配置（请求日志、定价等历史数据不受影响）。
      </p>

      <div className="space-y-3">
        <label className="flex items-center gap-2 text-xs text-text-secondary">
          <input
            type="checkbox"
            checked={includeSecrets}
            onChange={(e) => setIncludeSecrets(e.target.checked)}
            className="h-3.5 w-3.5 rounded border-border bg-card-secondary accent-accent"
          />
          导出时包含 API key（仅用于本机迁移；导出文件请勿分享）
        </label>

        <div className="flex flex-wrap items-center gap-2">
          <button onClick={handleExport} className="btn-primary inline-flex items-center gap-1.5 text-xs">
            <Download className="h-3.5 w-3.5" />
            导出配置
          </button>
          <label className="btn-secondary inline-flex cursor-pointer items-center gap-1.5 text-xs">
            <FolderOpen className="h-3.5 w-3.5" />
            选择文件导入
            <input
              type="file"
              accept="application/json,.json"
              className="hidden"
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) handleFilePicked(f);
                e.target.value = "";
              }}
            />
          </label>
        </div>
      </div>

      <ConfirmDialog
        open={!!pendingImportJson}
        variant="danger"
        title="确认导入配置？"
        message="导入会清空当前所有 providers 和 route profiles，并从文件还原。操作不可撤销——建议先导出当前配置作为备份。"
        confirmLabel={importing ? "导入中..." : "确认导入"}
        onConfirm={handleConfirmImport}
        onCancel={() => setPendingImportJson(null)}
      />
    </section>
  );
}

/// 可折叠 section：header 永远显示，body 默认折叠、点击展开。用于低频
/// 但占空间的设置（如模型定价表）——避免它把日常常用的"日志保留/配置
/// 导入导出"挤到下面要滚屏才能看见。
function CollapsibleSection({
  icon: Icon,
  title,
  hint,
  badge,
  children,
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  hint?: string;
  badge?: string;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <section className="rounded-xl border border-border bg-card p-5">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="flex w-full items-center justify-between gap-3 text-left"
      >
        <div className="flex items-center gap-2 text-sm font-semibold text-text-primary">
          <Icon className="h-4 w-4 text-accent" />
          {title}
          {badge && <span className="rounded-full bg-card-secondary px-1.5 py-0.5 text-[10px] font-normal text-text-muted">{badge}</span>}
        </div>
        {open ? <ChevronDown className="h-4 w-4 text-text-muted" /> : <ChevronRight className="h-4 w-4 text-text-muted" />}
      </button>
      {hint && open && <p className="mt-3 text-[11px] text-text-muted">{hint}</p>}
      {open && <div className="mt-4">{children}</div>}
    </section>
  );
}

function PetTypeCard({ type, selected, name, desc, onClick }: {
  type: PetType; selected: boolean; name: string; desc: string; onClick: () => void;
}) {
  const Preview = PET_PREVIEWS[type];
  return (
    <button
      onClick={onClick}
      className={`flex flex-col items-center gap-2 rounded-lg border p-4 transition-all duration-150 hover:scale-[1.02] ${
        selected
          ? "border-accent bg-accent/5"
          : "border-border bg-card-secondary hover:border-text-muted"
      }`}
    >
      <div className="h-16 w-16 flex items-center justify-center">
        <div className="scale-[0.55] origin-center">
          <Preview state="idle" />
        </div>
      </div>
      <div className="text-center">
        <p className={`text-xs font-medium ${selected ? "text-accent" : "text-text-primary"}`}>{name}</p>
        <p className="mt-0.5 text-[10px] text-text-muted leading-tight">{desc}</p>
      </div>
      {selected && (
        <span className="text-[10px] text-accent font-medium">● Active</span>
      )}
    </button>
  );
}

// ── Shared Components ──

function ToggleSwitch({ checked, onChange }: { checked: boolean; onChange: (val: boolean) => void }) {
  return (
    <label className="relative inline-flex cursor-pointer items-center">
      <input type="checkbox" className="peer sr-only" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <div className="h-5 w-9 rounded-full bg-border transition-colors after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-text-muted after:transition-all peer-checked:bg-accent peer-checked:after:translate-x-full peer-checked:after:bg-white" />
    </label>
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
      setStatus("latest");
    } finally {
      setChecking(false);
    }
  };

  const [installing, setInstalling] = useState(false);

  const handleInstall = async () => {
    setInstalling(true);
    try {
      const update = await check();
      if (!update) { setInstalling(false); return; }
      await update.downloadAndInstall();
      // Auto-relaunch into the freshly installed version.
      toast("success", t("update.relaunching"));
      await new Promise((r) => setTimeout(r, 800));
      await relaunch();
    } catch {
      toast("error", t("update.install_failed"));
      setInstalling(false);
    }
  };

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

function PricingRow({ item, onUpdate, onDelete }: { item: ModelPricing; onUpdate: (inputPrice: number, outputPrice: number) => void; onDelete: () => void }) {
  const { t } = useI18n();
  const [editInput, setEditInput] = useState(String(item.input_price));
  const [editOutput, setEditOutput] = useState(String(item.output_price));
  const [editing, setEditing] = useState(false);

  const save = () => {
    const inp = parseFloat(editInput);
    const outp = parseFloat(editOutput);
    if (!isNaN(inp) && !isNaN(outp) && (inp !== item.input_price || outp !== item.output_price)) {
      onUpdate(inp, outp);
    }
    setEditing(false);
  };

  return (
    <tr className="border-b border-border/50 last:border-0">
      <td className="px-3 py-1.5 text-text-primary">{item.provider}</td>
      <td className="px-3 py-1.5 font-mono text-text-secondary">{item.model_pattern}</td>
      <td className="px-3 py-1.5 text-right">
        {editing ? (
          <input type="number" step="0.01" value={editInput} onChange={(e) => setEditInput(e.target.value)} onBlur={save} onKeyDown={(e) => e.key === "Enter" && save()} className="w-20 rounded border border-accent/50 bg-bg px-1.5 py-0.5 text-right text-xs text-text-primary outline-none" autoFocus />
        ) : (
          <span className="cursor-pointer text-text-primary hover:text-accent" onClick={() => setEditing(true)}>{item.input_price.toFixed(2)}</span>
        )}
      </td>
      <td className="px-3 py-1.5 text-right">
        {editing ? (
          <input type="number" step="0.01" value={editOutput} onChange={(e) => setEditOutput(e.target.value)} onBlur={save} onKeyDown={(e) => e.key === "Enter" && save()} className="w-20 rounded border border-accent/50 bg-bg px-1.5 py-0.5 text-right text-xs text-text-primary outline-none" />
        ) : (
          <span className="cursor-pointer text-text-primary hover:text-accent" onClick={() => setEditing(true)}>{item.output_price.toFixed(2)}</span>
        )}
      </td>
      <td className="px-3 py-1.5 text-center">
        <span className={`rounded px-1.5 py-0.5 text-[10px] ${item.is_custom ? "bg-accent-soft text-accent" : "bg-card-secondary text-text-muted"}`}>
          {item.is_custom ? t("settings.custom") : t("settings.builtin")}
        </span>
      </td>
      <td className="px-3 py-1.5 text-center">
        {item.is_custom && (
          <button onClick={onDelete} className="text-text-muted hover:text-error"><Trash2 className="h-3 w-3" /></button>
        )}
      </td>
    </tr>
  );
}

function PricingAddForm({ onAdd }: { onAdd: (provider: string, model: string, inputPrice: number, outputPrice: number) => void }) {
  const { t } = useI18n();
  const [provider, setProvider] = useState("");
  const [model, setModel] = useState("");
  const [inputPrice, setInputPrice] = useState("");
  const [outputPrice, setOutputPrice] = useState("");

  const handleSubmit = () => {
    if (!provider.trim() || !model.trim()) return;
    const inp = parseFloat(inputPrice);
    const outp = parseFloat(outputPrice);
    if (isNaN(inp) || isNaN(outp)) return;
    onAdd(provider.trim(), model.trim(), inp, outp);
    setProvider("");
    setModel("");
    setInputPrice("");
    setOutputPrice("");
  };

  return (
    <div className="mt-3 flex items-end gap-2">
      <div className="flex-1">
        <label className="mb-1 block text-[10px] text-text-muted">Provider</label>
        <input value={provider} onChange={(e) => setProvider(e.target.value)} placeholder="deepseek" className="form-input w-full" />
      </div>
      <div className="flex-1">
        <label className="mb-1 block text-[10px] text-text-muted">Model</label>
        <input value={model} onChange={(e) => setModel(e.target.value)} placeholder="model-name or *" className="form-input w-full" />
      </div>
      <div className="w-24">
        <label className="mb-1 block text-[10px] text-text-muted">Input $/1M</label>
        <input type="number" step="0.01" value={inputPrice} onChange={(e) => setInputPrice(e.target.value)} placeholder="0.00" className="form-input w-full" />
      </div>
      <div className="w-24">
        <label className="mb-1 block text-[10px] text-text-muted">Output $/1M</label>
        <input type="number" step="0.01" value={outputPrice} onChange={(e) => setOutputPrice(e.target.value)} placeholder="0.00" className="form-input w-full" />
      </div>
      <button onClick={handleSubmit} className="btn-primary mb-0.5"><Plus className="h-3 w-3" />{t("routes.add")}</button>
    </div>
  );
}

// ── Theme picker — swatch grid ──────────────────────────────────────
// Each card previews the theme's surface + accent + text triplet so the
// user can pick by sight, not by name guessing. The check overlay marks
// the active theme.

interface ThemeSwatch {
  id: string;              // localStorage value + data-theme attribute
  labelEn: string;
  labelZh: string;
  bg: string;
  card: string;
  accent: string;
  textPrimary: string;
  border: string;
}

const THEME_SWATCHES: ThemeSwatch[] = [
  { id: "dark",    labelEn: "Warm Amber",    labelZh: "暖琥珀", bg: "#121110", card: "#1C1A18", accent: "#E89850", textPrimary: "#EDE8E2", border: "#38342F" },
  { id: "slate",   labelEn: "Slate Steel",   labelZh: "钢蓝",   bg: "#0F141B", card: "#1A2230", accent: "#38BDF8", textPrimary: "#E1E7EF", border: "#3A4454" },
  { id: "forest",  labelEn: "Forest Pine",   labelZh: "松林",   bg: "#0F1612", card: "#16201A", accent: "#84B062", textPrimary: "#E2E8E0", border: "#34453B" },
  { id: "violet",  labelEn: "Midnight Violet", labelZh: "紫夜", bg: "#14101C", card: "#1E1828", accent: "#A78BFA", textPrimary: "#ECE6F2", border: "#443854" },
  { id: "light",   labelEn: "Daylight",      labelZh: "晴日",   bg: "#F4F5F7", card: "#FFFFFF", accent: "#C07830", textPrimary: "#1A1C20", border: "#D5D8DC" },
  { id: "linen",   labelEn: "Linen Cream",   labelZh: "米麻",   bg: "#FAF6EE", card: "#FFFFFF", accent: "#B66821", textPrimary: "#2C2620", border: "#D9CFB8" },
  { id: "mist",    labelEn: "Mist Blue",     labelZh: "雾蓝",   bg: "#F4F7FB", card: "#FFFFFF", accent: "#2563EB", textPrimary: "#1B2735", border: "#CFD8E3" },
  { id: "sakura",  labelEn: "Sakura",        labelZh: "樱粉",   bg: "#FBF4F4", card: "#FFFFFF", accent: "#C44569", textPrimary: "#2C1F22", border: "#E0CCCC" },
];

function ThemePicker({ value, onChange }: { value: string; onChange: (id: string) => void }) {
  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
      {THEME_SWATCHES.map((s) => {
        const selected = value === s.id;
        return (
          <button
            key={s.id}
            type="button"
            onClick={() => onChange(s.id)}
            className={`group flex flex-col gap-2 rounded-lg border p-2.5 text-left transition-all ${
              selected ? "border-accent ring-2 ring-accent/40" : "border-border hover:border-text-muted"
            }`}
            style={{ backgroundColor: s.bg }}
            aria-pressed={selected}
          >
            {/* Mini preview: a faux card with accent dot + text bars */}
            <div
              className="flex items-center gap-2 rounded-md border px-2 py-2"
              style={{ backgroundColor: s.card, borderColor: s.border }}
            >
              <span className="h-3 w-3 shrink-0 rounded-full" style={{ backgroundColor: s.accent }} />
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <span className="h-1 w-3/4 rounded-full" style={{ backgroundColor: s.textPrimary, opacity: 0.85 }} />
                <span className="h-1 w-1/2 rounded-full" style={{ backgroundColor: s.textPrimary, opacity: 0.45 }} />
              </div>
            </div>
            <div className="flex items-baseline justify-between gap-2">
              <span className="text-xs font-medium" style={{ color: s.textPrimary }}>
                {s.labelZh}
              </span>
              <span className="text-[10px]" style={{ color: s.textPrimary, opacity: 0.6 }}>
                {s.labelEn}
              </span>
            </div>
          </button>
        );
      })}
    </div>
  );
}
