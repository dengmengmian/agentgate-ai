import { cn } from "@/lib/utils";

interface SkeletonProps {
  className?: string;
}

export function Skeleton({ className }: SkeletonProps) {
  return <div className={cn("skeleton", className)} />;
}

export function SkeletonCard() {
  return (
    <div className="rounded-xl border border-border bg-card p-5">
      <div className="flex items-center gap-3 mb-4">
        <Skeleton className="h-9 w-9 rounded-lg" />
        <div className="flex-1">
          <Skeleton className="h-4 w-32 mb-2" />
          <Skeleton className="h-3 w-48" />
        </div>
      </div>
      <div className="space-y-2">
        <Skeleton className="h-3 w-full" />
        <Skeleton className="h-3 w-3/4" />
      </div>
    </div>
  );
}
