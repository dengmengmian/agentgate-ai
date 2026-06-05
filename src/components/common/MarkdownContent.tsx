import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";

interface MarkdownContentProps {
  content: string;
  className?: string;
}

export function MarkdownContent({ content, className }: MarkdownContentProps) {
  return (
    <div className={cn("space-y-2 break-words", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: ({ children }) => <h1 className="text-base font-semibold leading-snug text-text-primary">{children}</h1>,
          h2: ({ children }) => <h2 className="text-[15px] font-semibold leading-snug text-text-primary">{children}</h2>,
          h3: ({ children }) => <h3 className="text-sm font-semibold leading-snug text-text-primary">{children}</h3>,
          p: ({ children }) => <p className="leading-relaxed">{children}</p>,
          a: ({ children, href }) => (
            <a href={href} target="_blank" rel="noreferrer" className="text-accent underline underline-offset-2">
              {children}
            </a>
          ),
          ul: ({ children }) => <ul className="space-y-1 pl-4 list-disc">{children}</ul>,
          ol: ({ children }) => <ol className="space-y-1 pl-4 list-decimal">{children}</ol>,
          li: ({ children }) => <li className="pl-0.5">{children}</li>,
          blockquote: ({ children }) => (
            <blockquote className="border-l-2 border-accent/50 pl-3 text-text-secondary">
              {children}
            </blockquote>
          ),
          table: ({ children }) => (
            <div className="overflow-x-auto rounded-lg border border-border">
              <table className="w-full border-collapse text-left text-xs">{children}</table>
            </div>
          ),
          thead: ({ children }) => <thead className="bg-card-secondary text-text-muted">{children}</thead>,
          th: ({ children }) => <th className="border-b border-border px-3 py-2 font-medium">{children}</th>,
          td: ({ children }) => <td className="border-t border-border px-3 py-2 align-top">{children}</td>,
          code: ({ children, className }) => (
            <code className={cn("rounded bg-card-secondary px-1 py-0.5 font-mono text-[0.92em]", className)}>
              {children}
            </code>
          ),
          pre: ({ children }) => (
            <pre className="overflow-auto rounded-md border border-border bg-card-secondary px-3 py-2 font-mono text-[11px] leading-relaxed text-text-secondary">
              {children}
            </pre>
          ),
          input: ({ checked, type, node: _node, ...props }) => (
            <input
              {...props}
              type={type}
              checked={checked}
              readOnly
              className="mr-1.5 align-middle"
            />
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
