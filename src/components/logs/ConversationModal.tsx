import { useEffect, useState } from "react";
import { Loader2, X, Copy, Terminal, FileText } from "lucide-react";
import { formatTimestamp } from "@/lib/utils";
import { toast } from "@/components/common/Toast";
import { useI18n } from "@/lib/i18n";
import { MarkdownContent } from "@/components/common/MarkdownContent";
import * as api from "@/lib/api";
import type { ConversationMessage } from "@/types/request-log";

/// 按会话来源生成恢复命令。网关请求没有「会话恢复」概念，返回 null。
export function resumeCommand(
  sessionId: string,
  source: string
): string | null {
  if (source === "codex_session") return `codex resume ${sessionId}`;
  if (source === "gateway") return null;
  return `claude --resume ${sessionId}`; // claude_session / mixed / 默认
}

type ConversationMessageKind = "chat" | "tool_call" | "tool_result";

export function getConversationMessageKind(
  text: string
): ConversationMessageKind {
  const trimmed = text.trim();
  if (/^\[Tool:\s*.+\]$/.test(trimmed)) return "tool_call";
  if (/^\[Tool result\]/.test(trimmed)) return "tool_result";
  return "chat";
}

/// 会话对话弹窗：读 Claude Code / Codex 本地日志，渲染对话气泡 + 恢复命令。
/// SessionGroupView 和 RequestDetailDrawer 共用。
export function ConversationModal({
  sessionId,
  source,
  onClose,
}: {
  sessionId: string;
  source: string;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [msgs, setMsgs] = useState<ConversationMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setErr(null);
    api
      .getSessionConversation(sessionId)
      .then((d) => {
        if (!cancelled) setMsgs(d);
      })
      .catch((e) => {
        if (!cancelled) setErr((e as api.AppError).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  const cmd = resumeCommand(sessionId, source);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={onClose}
    >
      <div
        className="flex max-h-[86vh] w-full max-w-4xl flex-col overflow-hidden rounded-xl border border-border bg-card shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-5 py-3">
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-text-primary">
              {t("logs.conversation_title")}
            </h3>
            <p
              className="truncate font-mono text-[11px] text-text-muted"
              title={sessionId}
            >
              {sessionId}
            </p>
          </div>
          <button
            onClick={onClose}
            className="shrink-0 text-text-muted hover:text-text-primary"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
        {cmd && (
          <div className="flex items-center justify-between gap-3 border-b border-border bg-card-secondary/50 px-5 py-2.5">
            <div className="flex min-w-0 items-center gap-2">
              <Terminal className="h-3.5 w-3.5 shrink-0 text-text-muted" />
              <code className="truncate font-mono text-[11px] text-text-secondary">
                {cmd}
              </code>
            </div>
            <button
              onClick={() => {
                navigator.clipboard.writeText(cmd);
                toast("success", t("logs.resume_cmd_copied"));
              }}
              className="shrink-0 rounded-md p-1.5 text-text-muted transition-colors hover:bg-card hover:text-accent"
              title={t("common.copy")}
            >
              <Copy className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
        <div className="flex-1 space-y-4 overflow-y-auto bg-background/30 p-5">
          {loading ? (
            <div className="flex items-center gap-2 text-xs text-text-muted">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t("common.loading")}
            </div>
          ) : err ? (
            <p className="text-xs text-error">{err}</p>
          ) : msgs.length === 0 ? (
            <p className="text-xs text-text-muted">
              {t("logs.conversation_empty")}
            </p>
          ) : (
            msgs.map((m, i) => <MessageBubble key={i} msg={m} />)
          )}
        </div>
      </div>
    </div>
  );
}

function MessageBubble({ msg }: { msg: ConversationMessage }) {
  const { t } = useI18n();
  const isUser = msg.role === "user";
  const kind = getConversationMessageKind(msg.text);
  const meta = (
    <div className="mb-1 text-[10px] text-text-muted">
      {isUser ? t("logs.msg_user") : t("logs.msg_ai")}
      {msg.timestamp ? ` · ${formatTimestamp(msg.timestamp)}` : ""}
    </div>
  );

  if (kind === "tool_call") {
    const toolName = msg.text
      .trim()
      .replace(/^\[Tool:\s*/, "")
      .replace(/\]$/, "");
    return (
      <div className="flex flex-col items-start">
        {meta}
        <div className="flex max-w-[72%] items-center gap-2 rounded-lg border border-border bg-card px-3 py-2 text-xs text-text-secondary">
          <Terminal className="h-3.5 w-3.5 shrink-0 text-text-muted" />
          <span>{t("logs.tool_call")}</span>
          <span className="rounded bg-card-secondary px-1.5 py-0.5 font-mono text-[10px] text-text-primary">
            {toolName}
          </span>
        </div>
      </div>
    );
  }

  if (kind === "tool_result") {
    return (
      <div className="flex flex-col items-start">
        {meta}
        <div className="w-full max-w-[92%] rounded-lg border border-border bg-card">
          <div className="flex items-center gap-2 border-b border-border px-3 py-2 text-[11px] font-medium text-text-muted">
            <FileText className="h-3.5 w-3.5" />
            {t("logs.tool_result")}
          </div>
          <pre className="max-h-[22rem] overflow-auto whitespace-pre-wrap break-words px-3 py-2.5 font-mono text-[11px] leading-relaxed text-text-secondary">
            {msg.text.replace(/^\[Tool result\]\s*/, "")}
          </pre>
        </div>
      </div>
    );
  }

  return (
    <div className={`flex flex-col ${isUser ? "items-end" : "items-start"}`}>
      {meta}
      <div
        className={`max-w-[76%] break-words rounded-xl px-3.5 py-2.5 text-sm leading-relaxed shadow-sm ${isUser ? "bg-accent/10 text-text-primary" : "bg-card text-text-primary"}`}
      >
        <MarkdownContent content={msg.text} />
      </div>
    </div>
  );
}
