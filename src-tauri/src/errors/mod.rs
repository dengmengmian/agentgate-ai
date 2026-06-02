use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
    pub suggestion: Option<String>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
            suggestion: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn not_found(entity: &str, id: &str) -> Self {
        Self::new("NOT_FOUND", format!("{entity} '{id}' not found"))
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new("VALIDATION_ERROR", message)
    }

    pub fn database(err: rusqlite::Error) -> Self {
        Self::new("DATABASE_ERROR", "Database operation failed").with_detail(err.to_string())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("INTERNAL_ERROR", message)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        Self::database(err)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        Self::new("NETWORK_ERROR", "Network request failed").with_detail(err.to_string())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_new() {
        let err = AppError::new("TEST_CODE", "test message");
        assert_eq!(err.code, "TEST_CODE");
        assert_eq!(err.message, "test message");
        assert!(err.detail.is_none());
        assert!(err.suggestion.is_none());
    }

    #[test]
    fn test_app_error_with_detail() {
        let err = AppError::new("TEST", "msg").with_detail("extra info");
        assert_eq!(err.detail, Some("extra info".to_string()));
    }

    #[test]
    fn test_app_error_with_suggestion() {
        let err = AppError::new("TEST", "msg").with_suggestion("try this");
        assert_eq!(err.suggestion, Some("try this".to_string()));
    }

    #[test]
    fn test_app_error_not_found() {
        let err = AppError::not_found("Provider", "123");
        assert_eq!(err.code, "NOT_FOUND");
        assert_eq!(err.message, "Provider '123' not found");
    }

    #[test]
    fn test_app_error_validation() {
        let err = AppError::validation("invalid input");
        assert_eq!(err.code, "VALIDATION_ERROR");
        assert_eq!(err.message, "invalid input");
    }

    #[test]
    fn test_app_error_internal() {
        let err = AppError::internal("something broke");
        assert_eq!(err.code, "INTERNAL_ERROR");
        assert_eq!(err.message, "something broke");
    }

    #[test]
    fn test_display_format() {
        let err = AppError::new("CODE", "message");
        assert_eq!(format!("{}", err), "[CODE] message");
    }

    #[test]
    fn test_from_rusqlite_error() {
        let sqlite_err = rusqlite::Error::InvalidQuery;
        let err: AppError = sqlite_err.into();
        assert_eq!(err.code, "DATABASE_ERROR");
    }
}
