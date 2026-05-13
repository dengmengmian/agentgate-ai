import { CopyButton } from "./CopyButton";

interface JsonCodeBlockProps {
  title?: string;
  content: string;
  language?: string;
}

export function JsonCodeBlock({
  title,
  content,
  language = "json",
}: JsonCodeBlockProps) {
  return (
    <div className="overflow-hidden rounded-lg border border-border bg-bg">
      {title && (
        <div className="flex items-center justify-between border-b border-border px-4 py-2">
          <span className="text-xs font-medium text-text-secondary">
            {title}
          </span>
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-text-muted">{language}</span>
            <CopyButton text={content} />
          </div>
        </div>
      )}
      <pre className="overflow-x-auto p-4 text-xs leading-relaxed text-text-secondary">
        <code>{content}</code>
      </pre>
    </div>
  );
}
