import { cn } from "@/lib/utils";

type Variant = "success" | "error" | "warning" | "muted" | "accent";

const styles: Record<Variant, string> = {
  success: "bg-success/10 text-success",
  error: "bg-error/10 text-error",
  warning: "bg-warning/10 text-warning",
  muted: "bg-text-muted/10 text-text-muted",
  accent: "bg-accent/10 text-accent",
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
