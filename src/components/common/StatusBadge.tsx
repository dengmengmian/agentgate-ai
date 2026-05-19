import { cn } from "@/lib/utils";

type Variant = "success" | "error" | "warning" | "muted" | "accent";

const styles: Record<Variant, string> = {
  success: "bg-success-soft text-success",
  error: "bg-error-soft text-error",
  warning: "bg-warning-soft text-warning",
  muted: "bg-hover text-text-muted",
  accent: "bg-accent-soft text-accent",
};

interface StatusBadgeProps {
  variant: Variant;
  children: React.ReactNode;
  className?: string;
}

export function StatusBadge({ variant, children, className }: StatusBadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full px-2 py-0.5 text-[11px] font-medium",
        styles[variant],
        className
      )}
    >
      {children}
    </span>
  );
}
