import { AlertTriangle, Lightbulb } from "lucide-react";
import { useI18n } from "@/lib/i18n";

interface ErrorExplanationCardProps {
  statusCode: number;
  message: string;
}

/// 根据 status code + 错误文本启发式生成"试试这个"建议，返回 i18n key。仅在
/// backend enhance_error 没附建议时兜底——backend 知道 provider 具体定价/账单
/// 页面 URL，比 client-side 更精确。这里只覆盖最常见的通用情况。
function deriveSuggestionKey(statusCode: number, message: string): string | null {
  const text = message.toLowerCase();

  // 余额 / 配额（覆盖广，最优先）
  if (
    text.includes("insufficient_balance")
    || text.includes("insufficient balance")
    || text.includes("insufficient_quota")
    || text.includes("insufficient quota")
    || text.includes("balance not enough")
    || text.includes("余额不足")
  ) {
    return "logs.suggest_balance";
  }

  // 鉴权
  if (
    statusCode === 401
    || statusCode === 403
    || text.includes("invalid_api_key")
    || text.includes("invalid api key")
    || text.includes("unauthorized")
    || text.includes("authentication")
  ) {
    return "logs.suggest_auth";
  }

  // 限频
  if (
    statusCode === 429
    || text.includes("rate_limit")
    || text.includes("rate limit")
    || text.includes("too many requests")
  ) {
    return "logs.suggest_rate_limit";
  }

  // 上下文长度 / max tokens
  if (
    text.includes("context_length")
    || text.includes("context length")
    || text.includes("max_tokens")
    || text.includes("maximum context length")
    || text.includes("token limit")
  ) {
    return "logs.suggest_context_length";
  }

  // 上游 5xx
  if (statusCode === 502 || statusCode === 503 || statusCode === 504) {
    return "logs.suggest_upstream_5xx";
  }
  if (statusCode === 500 && text.includes("<html")) {
    return "logs.suggest_upstream_html";
  }
  if (statusCode === 500) {
    return "logs.suggest_upstream_500";
  }

  // 网络
  if (
    text.includes("connection refused")
    || text.includes("failed to connect")
    || text.includes("dns")
    || text.includes("timeout")
    || text.includes("超时")
  ) {
    return "logs.suggest_network";
  }

  // 协议 / 参数错误
  if (statusCode === 400) {
    return "logs.suggest_bad_request";
  }

  // 404
  if (statusCode === 404) {
    return "logs.suggest_not_found";
  }

  return null;
}

export function ErrorExplanationCard({
  statusCode,
  message,
}: ErrorExplanationCardProps) {
  const { t } = useI18n();
  // Backend appends "\n\n💡 <suggestion>" to error_message when a provider's
  // enhance_error hook fires. Split it out so the suggestion gets its own
  // visual treatment instead of being mashed into the raw error text.
  const [body, ...suggestionParts] = message.split("\n\n💡 ");
  const backendSuggestion = suggestionParts.join("\n\n💡 ").trim();
  // Backend 没附建议时用 client-side heuristic 兜底——对通用 401/429/5xx 等
  // 一定能给出可执行下一步。
  const suggestionKey = deriveSuggestionKey(statusCode, body);
  const suggestion = backendSuggestion || (suggestionKey ? t(suggestionKey) : null);

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
            <span className="text-xs font-semibold text-accent">{t("logs.suggestion")}</span>
          </div>
          <p className="whitespace-pre-wrap break-words text-xs leading-relaxed text-text-secondary">{suggestion}</p>
        </div>
      )}
    </div>
  );
}
