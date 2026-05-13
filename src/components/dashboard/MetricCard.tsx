import type { LucideIcon } from "lucide-react";

interface MetricCardProps {
  label: string;
  value: string | number;
  icon: LucideIcon;
  trend?: string;
}

export function MetricCard({ label, value, icon: Icon, trend }: MetricCardProps) {
  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-xs text-text-muted">{label}</span>
        <Icon className="h-4 w-4 text-text-muted" />
      </div>
      <p className="text-xl font-semibold text-text-primary">{value}</p>
      {trend && (
        <p className="mt-1 text-[11px] text-text-muted">{trend}</p>
      )}
    </div>
  );
}
