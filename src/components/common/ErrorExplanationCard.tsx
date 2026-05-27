import { AlertTriangle, Lightbulb } from "lucide-react";

interface ErrorExplanationCardProps {
  statusCode: number;
  message: string;
}

export function ErrorExplanationCard({
  statusCode,
  message,
}: ErrorExplanationCardProps) {
  // Backend appends "\n\n💡 <suggestion>" to error_message when a provider's
  // enhance_error hook fires. Split it out so the suggestion gets its own
  // visual treatment instead of being mashed into the raw error text.
  const [body, ...suggestionParts] = message.split("\n\n💡 ");
  const suggestion = suggestionParts.join("\n\n💡 ").trim();

  return (
    <div className="space-y-2">
      <div className="rounded-xl border border-error/20 bg-error/5 p-4">
        <div className="mb-2 flex items-center gap-2">
          <AlertTriangle className="h-4 w-4 text-error" />
          <span className="text-xs font-semibold text-error">
            Error {statusCode}
          </span>
        </div>
        <p className="whitespace-pre-wrap break-words text-xs leading-relaxed text-text-secondary">{body}</p>
      </div>
      {suggestion && (
        <div className="rounded-xl border border-accent/20 bg-accent-soft p-4">
          <div className="mb-2 flex items-center gap-2">
            <Lightbulb className="h-4 w-4 text-accent" />
            <span className="text-xs font-semibold text-accent">建议</span>
          </div>
          <p className="whitespace-pre-wrap break-words text-xs leading-relaxed text-text-secondary">{suggestion}</p>
        </div>
      )}
    </div>
  );
}
