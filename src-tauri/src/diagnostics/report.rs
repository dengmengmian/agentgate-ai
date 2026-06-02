use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CheckItem {
    pub id: String,
    pub name: String,
    pub status: String, // "ok" | "warning" | "failed" | "skipped"
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl CheckItem {
    pub fn ok(id: &str, name: &str, msg: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            status: "ok".into(),
            message: msg.into(),
            detail: None,
            suggestion: None,
        }
    }
    pub fn warning(id: &str, name: &str, msg: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            status: "warning".into(),
            message: msg.into(),
            detail: None,
            suggestion: None,
        }
    }
    pub fn failed(id: &str, name: &str, msg: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            status: "failed".into(),
            message: msg.into(),
            detail: None,
            suggestion: None,
        }
    }
    pub fn with_suggestion(mut self, s: &str) -> Self {
        self.suggestion = Some(s.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub name: String,
    pub status: String,
    pub checks: Vec<CheckItem>,
    pub summary: String,
    pub created_at: String,
}

impl CheckReport {
    pub fn new(name: &str, checks: Vec<CheckItem>) -> Self {
        let has_failed = checks.iter().any(|c| c.status == "failed");
        let has_warning = checks.iter().any(|c| c.status == "warning");
        let status = if has_failed {
            "failed"
        } else if has_warning {
            "warning"
        } else {
            "ok"
        };
        let ok_count = checks.iter().filter(|c| c.status == "ok").count();
        let summary = format!("{}/{} passed", ok_count, checks.len());
        Self {
            name: name.into(),
            status: status.into(),
            checks,
            summary,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FullSelfTestReport {
    pub overall_status: String,
    pub reports: Vec<CheckReport>,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub success: bool,
    pub path: String,
    pub files: Vec<String>,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_item_ok() {
        let item = CheckItem::ok("id1", "Name", "All good");
        assert_eq!(item.id, "id1");
        assert_eq!(item.name, "Name");
        assert_eq!(item.status, "ok");
        assert_eq!(item.message, "All good");
        assert!(item.detail.is_none());
        assert!(item.suggestion.is_none());
    }

    #[test]
    fn check_item_warning() {
        let item = CheckItem::warning("id2", "Name", "Careful");
        assert_eq!(item.status, "warning");
    }

    #[test]
    fn check_item_failed() {
        let item = CheckItem::failed("id3", "Name", "Broken");
        assert_eq!(item.status, "failed");
    }

    #[test]
    fn check_item_with_suggestion() {
        let item = CheckItem::ok("id", "Name", "msg").with_suggestion("Try this");
        assert_eq!(item.suggestion, Some("Try this".to_string()));
    }

    #[test]
    fn check_report_all_ok() {
        let checks = vec![CheckItem::ok("a", "A", "ok"), CheckItem::ok("b", "B", "ok")];
        let report = CheckReport::new("Test", checks);
        assert_eq!(report.status, "ok");
        assert_eq!(report.summary, "2/2 passed");
    }

    #[test]
    fn check_report_with_warning() {
        let checks = vec![
            CheckItem::ok("a", "A", "ok"),
            CheckItem::warning("b", "B", "warn"),
        ];
        let report = CheckReport::new("Test", checks);
        assert_eq!(report.status, "warning");
        assert_eq!(report.summary, "1/2 passed");
    }

    #[test]
    fn check_report_with_failed() {
        let checks = vec![
            CheckItem::ok("a", "A", "ok"),
            CheckItem::failed("b", "B", "fail"),
        ];
        let report = CheckReport::new("Test", checks);
        assert_eq!(report.status, "failed");
        assert_eq!(report.summary, "1/2 passed");
    }

    #[test]
    fn check_report_failed_over_warning() {
        let checks = vec![
            CheckItem::warning("a", "A", "warn"),
            CheckItem::failed("b", "B", "fail"),
        ];
        let report = CheckReport::new("Test", checks);
        assert_eq!(report.status, "failed");
    }
}
