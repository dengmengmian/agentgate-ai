import { Terminal, Code, Braces } from "lucide-react";
import { StatusBadge } from "@/components/common/StatusBadge";
import type { ToolConfigView } from "@/types/tool";

const iconMap: Record<string, React.ElementType> = {
  terminal: Terminal,
  code: Code,
  braces: Braces,
};

interface ToolCardProps {
  tool: ToolConfigView;
}

export function ToolCard({ tool }: ToolCardProps) {
  const Icon = iconMap[tool.icon] ?? Terminal;

  return (
    <div className="rounded-lg border border-border bg-card p-5">
      {/* Header */}
      <div className="mb-4 flex items-start justify-between">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-accent-soft">
            <Icon className="h-5 w-5 text-accent" />
          </div>
          <div>
            <h3 className="text-sm font-semibold text-text-primary">
              {tool.name}
            </h3>
            <p className="text-xs text-text-muted">{tool.description}</p>
          </div>
        </div>
        <StatusBadge variant={tool.config_exists ? "success" : "muted"}>
          {tool.config_exists ? "Config found" : "No config"}
        </StatusBadge>
      </div>

      {/* Fields */}
      <div className="mb-4 text-xs">
        <span className="text-text-muted">Config Path</span>
        <p className="font-mono text-text-secondary">{tool.config_path}</p>
      </div>
    </div>
  );
}
