import { useState, useEffect, useCallback } from "react";
import { Server, Plus, Trash2, ToggleLeft, ToggleRight, X } from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import * as api from "@/lib/api";
import type { McpOverview } from "@/types/mcp";

export function Mcp() {
  const { t } = useI18n();
  const [overview, setOverview] = useState<McpOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [showAdd, setShowAdd] = useState(false);

  const load = useCallback(async () => {
    try {
      const data = await api.getMcpOverview();
      setOverview(data);
    } catch (err) {
      toast("error", (err as api.AppError).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleToggle = async (client: string, name: string, enabled: boolean) => {
    try {
      await api.toggleMcpServer(client, name, enabled);
      toast("success", enabled ? t("mcp.enabled") : t("mcp.disabled"));
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  const handleRemove = async (client: string, name: string) => {
    try {
      await api.removeMcpServer(client, name);
      toast("success", t("common.deleted"));
      load();
    } catch (err) { toast("error", (err as api.AppError).message); }
  };

  if (loading) return <p className="text-xs text-text-muted">{t("common.loading")}</p>;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-sm font-semibold text-text-primary">{t("mcp.title")}</h2>
          <p className="text-xs text-text-muted">
            {overview ? `${overview.total_servers} ${t("mcp.servers_across")} ${overview.total_clients} ${t("mcp.clients")}` : ""}
          </p>
        </div>
        <button onClick={() => setShowAdd(true)} className="btn-primary"><Plus className="h-3 w-3" />{t("mcp.add_server")}</button>
      </div>

      {/* Sources */}
      {overview?.sources.length === 0 && (
        <div className="rounded-lg border border-border bg-card p-8 text-center">
          <Server className="mx-auto h-8 w-8 text-text-muted" />
          <p className="mt-2 text-sm text-text-muted">{t("mcp.no_servers")}</p>
          <p className="text-xs text-text-muted">{t("mcp.no_servers_hint")}</p>
        </div>
      )}

      {overview?.sources.map((source) => (
        <div key={source.config_path} className="rounded-lg border border-border bg-card p-5">
          <div className="mb-3 flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Server className="h-4 w-4 text-accent" />
              <h3 className="text-sm font-semibold text-text-primary">{source.client}</h3>
              <StatusBadge variant="muted">{source.servers.length} servers</StatusBadge>
            </div>
            <span className="font-mono text-[10px] text-text-muted">{source.config_path}</span>
          </div>

          <div className="space-y-2">
            {source.servers.map((server) => (
              <div key={server.name} className={`flex items-center justify-between rounded-md border px-4 py-2.5 ${server.enabled ? "border-border/50 bg-card-secondary" : "border-border/30 bg-card-secondary/50 opacity-60"}`}>
                <div className="flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-text-primary">{server.name}</span>
                    {!server.enabled && <StatusBadge variant="muted">{t("mcp.server_disabled")}</StatusBadge>}
                  </div>
                  <p className="mt-0.5 font-mono text-[10px] text-text-muted truncate max-w-md">
                    {server.command} {server.args.join(" ")}
                  </p>
                </div>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => handleToggle(source.client, server.name, !server.enabled)}
                    className="rounded p-1 text-text-muted hover:bg-border hover:text-text-primary"
                    title={server.enabled ? t("mcp.disable") : t("mcp.enable")}
                  >
                    {server.enabled ? <ToggleRight className="h-4 w-4 text-accent" /> : <ToggleLeft className="h-4 w-4" />}
                  </button>
                  <button
                    onClick={() => handleRemove(source.client, server.name)}
                    className="rounded p-1 text-text-muted hover:bg-error/20 hover:text-error"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}

      {/* Add dialog */}
      {showAdd && <AddMcpDialog onAdd={async (client, name, command, args) => {
        try {
          await api.addMcpServer(client, name, command, args);
          toast("success", t("mcp.server_added"));
          setShowAdd(false);
          load();
        } catch (err) { toast("error", (err as api.AppError).message); }
      }} onClose={() => setShowAdd(false)} />}
    </div>
  );
}

function AddMcpDialog({ onAdd, onClose }: { onAdd: (client: string, name: string, command: string, args: string[]) => void; onClose: () => void }) {
  const { t } = useI18n();
  const [client, setClient] = useState("Claude Code");
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative z-10 w-full max-w-md rounded-lg border border-border bg-card shadow-xl">
        <div className="flex items-center justify-between border-b border-border px-5 py-3">
          <h3 className="text-sm font-semibold text-text-primary">{t("mcp.add_server")}</h3>
          <button onClick={onClose} className="rounded p-1 text-text-muted hover:text-text-primary"><X className="h-4 w-4" /></button>
        </div>
        <div className="space-y-3 p-5">
          <div>
            <label className="mb-1 block text-[10px] text-text-muted">{t("mcp.target_client")}</label>
            <select value={client} onChange={(e) => setClient(e.target.value)} className="form-input w-full">
              <option value="Claude Code">Claude Code</option>
              <option value="Gemini CLI">Gemini CLI</option>
            </select>
          </div>
          <div>
            <label className="mb-1 block text-[10px] text-text-muted">{t("mcp.server_name")}</label>
            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-mcp-server" className="form-input w-full" />
          </div>
          <div>
            <label className="mb-1 block text-[10px] text-text-muted">{t("mcp.command")}</label>
            <input value={command} onChange={(e) => setCommand(e.target.value)} placeholder="/path/to/mcp-server" className="form-input w-full" />
          </div>
          <div>
            <label className="mb-1 block text-[10px] text-text-muted">{t("mcp.args")}</label>
            <input value={args} onChange={(e) => setArgs(e.target.value)} placeholder="--arg1 --arg2 (space-separated)" className="form-input w-full" />
          </div>
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <button onClick={onClose} className="rounded-md bg-card-secondary px-4 py-1.5 text-xs text-text-secondary hover:bg-border">{t("common.cancel")}</button>
          <button onClick={() => {
            if (!name.trim() || !command.trim()) return;
            onAdd(client, name.trim(), command.trim(), args.trim() ? args.trim().split(/\s+/) : []);
          }} className="rounded-md bg-accent px-4 py-1.5 text-xs font-medium text-white hover:bg-accent/90">{t("routes.add")}</button>
        </div>
      </div>
    </div>
  );
}
