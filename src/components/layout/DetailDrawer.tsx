import { X } from "lucide-react";
import { cn } from "@/lib/utils";

interface DetailDrawerProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: React.ReactNode;
}

export function DetailDrawer({
  open,
  onClose,
  title,
  children,
}: DetailDrawerProps) {
  return (
    <>
      {/* Backdrop */}
      {open && (
        <div
          className="fixed inset-0 z-40 bg-black/40 backdrop-blur-sm"
          onClick={onClose}
        />
      )}

      {/* Drawer */}
      <div
        className={cn(
          "fixed right-0 top-0 z-50 flex h-full w-[480px] flex-col border-l border-border bg-card transition-transform duration-200",
          open ? "translate-x-0" : "translate-x-full"
        )}
        style={{ boxShadow: open ? "var(--shadow-lg)" : "none" }}
      >
        {/* Header */}
        <div className="flex h-14 items-center justify-between border-b border-border px-5">
          <h2 className="text-sm font-semibold text-text-primary">
            {title ?? "Details"}
          </h2>
          <button
            onClick={onClose}
            className="rounded-lg p-1.5 text-text-muted transition-colors hover:bg-hover hover:text-text-primary"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-5">{children}</div>
      </div>
    </>
  );
}
