// 从 bindings re-export。CheckItem.status 在 Rust 端是 String,bindings 给 string;
// 前端历史 union 在边界 cast,这里给一个 narrow 版本。
import type {
  CheckItem as WideItem,
  CheckReport as WideReport,
  FullSelfTestReport as WideFull,
  ExportResult,
} from "@/lib/bindings";

export type { ExportResult };

export type CheckItem = Omit<WideItem, "status"> & {
  status: "ok" | "warning" | "failed" | "skipped";
};

export type CheckReport = Omit<WideReport, "checks"> & {
  checks: CheckItem[];
};

export type FullSelfTestReport = Omit<WideFull, "reports"> & {
  reports: CheckReport[];
};
