export interface CheckItem {
  id: string;
  name: string;
  status: "ok" | "warning" | "failed" | "skipped";
  message: string;
  detail?: string;
  suggestion?: string;
}

export interface CheckReport {
  name: string;
  status: string;
  checks: CheckItem[];
  summary: string;
  created_at: string;
}

export interface FullSelfTestReport {
  overall_status: string;
  reports: CheckReport[];
  summary: string;
  created_at: string;
}

export interface ExportResult {
  success: boolean;
  path: string;
  files: string[];
  warnings: string[];
}
