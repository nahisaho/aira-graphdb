use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const ALLOWED_FAILURE_CLASSES: [&str; 5] = [
    "INTERNAL_BUG",
    "IO_FAILURE",
    "OOM",
    "TIMEOUT",
    "CLIENT_INPUT",
];

pub const INTERNAL_FAILURE_CLASSES: [&str; 4] = ["INTERNAL_BUG", "IO_FAILURE", "OOM", "TIMEOUT"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeRequestAuditEvent {
    pub error_code: String,
    pub failure_class: String,
    pub request_id: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeCrashAuditEvent {
    pub error_code: String,
    pub timestamp: String,
    pub process_exit_code: Option<i32>,
    pub signal: Option<String>,
    pub last_request_id: Option<String>,
    pub uptime_sec: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

pub fn now_epoch_ms_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_millis()
        .to_string()
}

pub fn is_allowed_failure_class(failure_class: &str) -> bool {
    ALLOWED_FAILURE_CLASSES.contains(&failure_class)
}

pub fn is_internal_failure_class(failure_class: &str) -> bool {
    INTERNAL_FAILURE_CLASSES.contains(&failure_class)
}

pub fn validate_request_event_required_fields(event: &NativeRequestAuditEvent) -> Result<(), String> {
    if event.error_code.trim().is_empty() {
        return Err("errorCode is required".to_string());
    }
    if !is_allowed_failure_class(&event.failure_class) {
        return Err(format!("failureClass must be one of {:?}", ALLOWED_FAILURE_CLASSES));
    }
    if event.request_id.trim().is_empty() {
        return Err("requestId is required".to_string());
    }
    if event.timestamp.trim().is_empty() {
        return Err("timestamp is required".to_string());
    }
    Ok(())
}

pub fn validate_crash_event_required_fields(event: &NativeCrashAuditEvent) -> Result<(), String> {
    if event.error_code != "PROCESS_CRASH" {
        return Err("errorCode must be PROCESS_CRASH".to_string());
    }
    if event.timestamp.trim().is_empty() {
        return Err("timestamp is required".to_string());
    }
    if event.last_request_id.as_deref().unwrap_or_default().trim().is_empty() {
        return Err("lastRequestId is required".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_request_event_required_fields() {
        let event = NativeRequestAuditEvent {
            error_code: "REQUEST_EXECUTION_FAILED".to_string(),
            failure_class: "CLIENT_INPUT".to_string(),
            request_id: "r-1".to_string(),
            timestamp: now_epoch_ms_string(),
        };
        assert!(validate_request_event_required_fields(&event).is_ok());
    }

    #[test]
    fn validates_crash_event_required_fields() {
        let event = NativeCrashAuditEvent {
            error_code: "PROCESS_CRASH".to_string(),
            timestamp: now_epoch_ms_string(),
            process_exit_code: Some(101),
            signal: Some("SIGABRT".to_string()),
            last_request_id: Some("r-10".to_string()),
            uptime_sec: 120,
            cause: Some("panic message".to_string()),
        };
        assert!(validate_crash_event_required_fields(&event).is_ok());
    }
}
