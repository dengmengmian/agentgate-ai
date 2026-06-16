import { useState } from "react";
import { Copy, Check } from "lucide-react";
import { cn } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";

interface CopyButtonProps {
  text: string;
  className?: string;
  /// 显示文字标签——默认只显示图标，复制后短暂显示"已复制"。
  /// 设为 false 强制只图标模式（适合密集 UI）。默认 true。
  showLabel?: boolean;
}

export function CopyButton({
  text,
  className,
  showLabel = true,
}: CopyButtonProps) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      // 桌面 webview 极少数情况权限拒绝——静默失败比误报"已复制"好
    }
  };

  return (
    <button
      onClick={handleCopy}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] font-medium transition-all",
        copied
          ? "bg-success-soft text-success"
          : "text-text-muted hover:bg-hover hover:text-text-primary",
        className
      )}
      title={t("common.copy")}
    >
      {copied ? (
        <Check className="h-3.5 w-3.5" />
      ) : (
        <Copy className="h-3.5 w-3.5" />
      )}
      {showLabel && (
        <span>{copied ? t("common.copied") : t("common.copy")}</span>
      )}
    </button>
  );
}
