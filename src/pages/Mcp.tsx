import { useEffect, useState } from "react";
import { Loader2, Plug, Code, Terminal, KeyRound } from "lucide-react";
import { EmptyState } from "@/components/common/EmptyState";
import { toast } from "@/components/common/Toast";
import * as api from "@/lib/api";
import type { McpServer } from "@/lib/api";

/// MCP 面板(第一步:只读展示)。以客户端文件为真相源,读 Codex / Claude Code 现有的
/// MCP server,按客户端分组展示。增删改 / 跨客户端复制是后续步骤。
export function Mcp() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    api.listMcpServers()
      .then((d) => { if (!cancelled) setServers(d); })
      .catch((err) => { if (!cancelled) toast("error", (err as api.AppError).message); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, []);

  const groups: { client: string; label: string; icon: typeof Code; items: McpServer[] }[] = [
    { client: "codex", label: "Codex", icon: Code, items: servers.filter((s) => s.client === "codex") },
    { client: "claude_code", label: "Claude Code", icon: Terminal, items: servers.filter((s) => s.client === "claude_code") },
  ];

  if (loading) {
    return (
      <div className="flex items-center gap-2 text-xs text-text-muted">
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
        加载中…
      </div>
    );
  }

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-sm font-semibold text-text-primary">MCP 服务器</h2>
        <p className="mt-0.5 text-xs text-text-muted">
          读自各客户端配置文件（Codex <code className="font-mono">config.toml</code> / Claude Code <code className="font-mono">.claude.json</code>）。
          当前为只读展示，增删改与跨客户端同步后续支持。env 仅显示 key、不显示值。
        </p>
      </div>

      {servers.length === 0 ? (
        <EmptyState
          icon={Plug}
          title="没有检测到 MCP 服务器"
          description="在 Codex 或 Claude Code 里配置过 MCP server 后，这里会自动列出。"
        />
      ) : (
        <div className="space-y-5">
          {groups.filter((g) => g.items.length > 0).map((g) => (
            <section key={g.client}>
              <div className="mb-2 flex items-center gap-2 text-xs font-medium text-text-secondary">
                <g.icon className="h-3.5 w-3.5" />
                {g.label}
                <span className="text-text-muted">· {g.items.length}</span>
              </div>
              <div className="space-y-2">
                {g.items.map((s) => (
                  <div key={`${s.client}:${s.name}`} className="rounded-xl border border-border bg-card p-4">
                    <div className="mb-1 font-mono text-xs font-semibold text-text-primary">{s.name}</div>
                    {s.command && (
                      <div className="truncate font-mono text-[11px] text-text-muted" title={s.command}>
                        {s.command} {s.args.join(" ")}
                      </div>
                    )}
                    {s.env_keys.length > 0 && (
                      <div className="mt-2 flex flex-wrap items-center gap-1.5">
                        <KeyRound className="h-3 w-3 text-text-muted" />
                        {s.env_keys.map((k) => (
                          <span key={k} className="rounded bg-card-secondary px-1.5 py-0.5 font-mono text-[10px] text-text-secondary">{k}</span>
                        ))}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
