import { AlertTriangle, Lightbulb } from "lucide-react";

interface ErrorExplanationCardProps {
  statusCode: number;
  message: string;
}

/// 根据 status code + 错误文本启发式生成"试试这个"建议。仅在 backend
/// enhance_error 没附建议时兜底——backend 知道 provider 具体定价/账单
/// 页面 URL，比 client-side 更精确。这里只覆盖最常见的通用情况。
function deriveSuggestion(statusCode: number, message: string): string | null {
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
    return "账户余额不足或额度耗尽。去 provider 后台充值或提升配额。";
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
    return "API key 无效、过期或权限不足。回服务商页编辑该 provider，重新粘贴 key。";
  }

  // 限频
  if (
    statusCode === 429
    || text.includes("rate_limit")
    || text.includes("rate limit")
    || text.includes("too many requests")
  ) {
    return "上游限频。降低请求频率，或在路由策略里配多个 provider 做失败转移。";
  }

  // 上下文长度 / max tokens
  if (
    text.includes("context_length")
    || text.includes("context length")
    || text.includes("max_tokens")
    || text.includes("maximum context length")
    || text.includes("token limit")
  ) {
    return "请求超出 model 上下文 / max_tokens 限制。缩短 prompt、调高 max_output_tokens，或换更大窗口的模型。";
  }

  // 上游 5xx
  if (statusCode === 502 || statusCode === 503 || statusCode === 504) {
    return "上游服务临时故障（5xx）。立刻重试一次，或在路由策略里加 failover 自动切换。";
  }
  if (statusCode === 500 && text.includes("<html")) {
    return "上游网关返回了 HTML 错误页（通常是上游内部到 origin 的连接抖动）。立刻重试或切换 provider。";
  }
  if (statusCode === 500) {
    return "上游内部错误。先重试一次；持续失败检查 provider 状态页。";
  }

  // 网络
  if (
    text.includes("connection refused")
    || text.includes("failed to connect")
    || text.includes("dns")
    || text.includes("timeout")
    || text.includes("超时")
  ) {
    return "网络问题。检查本机网络 / VPN / 是否需要代理访问该服务商。";
  }

  // 协议 / 参数错误
  if (statusCode === 400) {
    return "请求被服务商拒绝（400）。看下面的「原始响应」找具体字段名，可能需要在 provider 配置里调整 model 或参数。";
  }

  // 404
  if (statusCode === 404) {
    return "上游路径不存在。检查 provider 的 base_url 是否正确、是否需要 /v1 后缀。";
  }

  return null;
}

export function ErrorExplanationCard({
  statusCode,
  message,
}: ErrorExplanationCardProps) {
  // Backend appends "\n\n💡 <suggestion>" to error_message when a provider's
  // enhance_error hook fires. Split it out so the suggestion gets its own
  // visual treatment instead of being mashed into the raw error text.
  const [body, ...suggestionParts] = message.split("\n\n💡 ");
  const backendSuggestion = suggestionParts.join("\n\n💡 ").trim();
  // Backend 没附建议时用 client-side heuristic 兜底——对通用 401/429/5xx 等
  // 一定能给出可执行下一步。
  const suggestion = backendSuggestion || deriveSuggestion(statusCode, body);

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
