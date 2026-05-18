import { clsx, type ClassValue } from "clsx";

export function cn(...inputs: ClassValue[]) {
  return clsx(inputs);
}

export function formatTimestamp(iso: string, locale: string = "en-US"): string {
  const d = new Date(iso);
  const loc = locale === "zh" ? "zh-CN" : locale;
  const month = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  const time = d.toLocaleTimeString(loc, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
  return `${month}-${day} ${time}`;
}

export function formatDate(iso: string, locale: string = "en-US"): string {
  const d = new Date(iso);
  const loc = locale === "zh" ? "zh-CN" : locale;
  return d.toLocaleDateString(loc, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function formatLatency(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function formatUptime(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}
