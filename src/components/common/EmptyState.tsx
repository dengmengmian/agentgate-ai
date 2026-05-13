import type { LucideIcon } from "lucide-react";

interface EmptyStateProps {
  icon: LucideIcon;
  title: string;
  description: string;
}

export function EmptyState({ icon: Icon, title, description }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-16 text-center">
      <Icon className="mb-4 h-10 w-10 text-text-muted" />
      <h3 className="mb-1 text-sm font-medium text-text-primary">{title}</h3>
      <p className="text-xs text-text-muted">{description}</p>
    </div>
  );
}
