import { useEffect, useState } from "react";
import { Loader2, X, Copy } from "lucide-react";
import { formatTimestamp } from "@/lib/utils";
import { toast } from "@/components/common/Toast";
import * as api from "@/lib/api";
import type { ConversationMessage } from "@/types/request-log";

/// 按会话来源生成恢复命令。网关请求没有「会话恢复」概念，返回 null。
export function resumeCommand(sessionId: string, source: string): string | null {
  if (source === "codex_session") return `codex resume ${sessionId}`;
  if (source === "gateway") return null;
  return `claude --resume ${sessionId}`; // claude_session / mixed / 默认
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
  const [msgs, setMsgs] = useState<ConversationMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setErr(null);
    api.getSessionConversation(sessionId)
      .then((d) => { if (!cancelled) setMsgs(d); })
      .catch((e) => { if (!cancelled) setErr((e as api.AppError).message); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [sessionId]);

  const cmd = resumeCommand(sessionId, source);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div className="flex max-h-[82vh] w-full max-w-3xl flex-col rounded-xl border border-border bg-card" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-border px-5 py-3">
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-text-primary">会话对话</h3>
            <p className="truncate font-mono text-[11px] text-text-muted" title={sessionId}>{sessionId}</p>
          </div>
          <button onClick={onClose} className="shrink-0 text-text-muted hover:text-text-primary"><X className="h-4 w-4" /></button>
        </div>
        {cmd && (
          <div className="flex items-center justify-between gap-3 border-b border-border px-5 py-2">
            <code className="truncate font-mono text-[11px] text-text-secondary">{cmd}</code>
            <button
              onClick={() => { navigator.clipboard.writeText(cmd); toast("success", "已复制恢复命令"); }}
              className="shrink-0 text-text-muted transition-colors hover:text-accent"
              title="复制"
            >
              <Copy className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
        <div className="flex-1 space-y-3 overflow-y-auto p-5">
          {loading ? (
            <div className="flex items-center gap-2 text-xs text-text-muted"><Loader2 className="h-3.5 w-3.5 animate-spin" />加载中…</div>
          ) : err ? (
            <p className="text-xs text-error">{err}</p>
          ) : msgs.length === 0 ? (
            <p className="text-xs text-text-muted">没有可显示的对话内容（该来源的本地日志可能不含完整对话）。</p>
          ) : (
            msgs.map((m, i) => <MessageBubble key={i} msg={m} />)
          )}
        </div>
      </div>
    </div>
  );
}

function MessageBubble({ msg }: { msg: ConversationMessage }) {
  const isUser = msg.role === "user";
  return (
    <div className={`flex flex-col ${isUser ? "items-end" : "items-start"}`}>
      <div className="mb-1 text-[10px] text-text-muted">
        {isUser ? "用户" : "AI"}{msg.timestamp ? ` · ${formatTimestamp(msg.timestamp)}` : ""}
      </div>
      <div className={`max-w-[85%] whitespace-pre-wrap break-words rounded-lg px-3 py-2 text-xs ${isUser ? "bg-accent/10 text-text-primary" : "bg-card-secondary text-text-primary"}`}>
        {msg.text}
      </div>
    </div>
  );
}
