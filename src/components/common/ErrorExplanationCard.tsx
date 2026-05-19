import { AlertTriangle } from "lucide-react";

interface ErrorExplanationCardProps {
  statusCode: number;
  message: string;
}

export function ErrorExplanationCard({
  statusCode,
  message,
}: ErrorExplanationCardProps) {
  return (
    <div className="rounded-xl border border-error/20 bg-error/5 p-4">
      <div className="mb-2 flex items-center gap-2">
        <AlertTriangle className="h-4 w-4 text-error" />
        <span className="text-xs font-semibold text-error">
          Error {statusCode}
        </span>
      </div>
      <p className="text-xs leading-relaxed text-text-secondary">{message}</p>
    </div>
  );
}
