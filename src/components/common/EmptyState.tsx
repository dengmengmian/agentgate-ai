import type { LucideIcon } from "lucide-react";

interface EmptyStateProps {
  icon: LucideIcon;
  title: string;
  description: string;
  action?: React.ReactNode;
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
}: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-16 text-center">
      <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-card-secondary">
        <Icon className="h-6 w-6 text-text-muted" />
      </div>
      <h3 className="mb-1 text-sm font-medium text-text-primary">{title}</h3>
      <p className="mb-4 max-w-xs text-xs leading-relaxed text-text-muted">
        {description}
      </p>
      {action}
    </div>
  );
}
