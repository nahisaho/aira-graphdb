use std::path::Path;

use aira_graphdb::audit::{
    NativeCrashAuditEvent, now_epoch_ms_string, validate_crash_event_required_fields,
};
use aira_graphdb::native_bench::{
    parse_native_soak_profile, run_native_soak_report, write_native_audit_artifact,
    write_native_soak_report,
};

const SOAK_REPORT_PATH: &str = "artifacts/native-soak-report.json";
const AUDIT_REPORT_PATH: &str = "artifacts/native-audit-events.json";

#[test]
fn writes_native_soak_artifacts_and_meets_gate() {
    let profile = std::env::var("AGDB_NATIVE_SOAK_PROFILE")
        .map(|v| parse_native_soak_profile(&v))
        .unwrap_or_else(|_| parse_native_soak_profile("P0-NATIVE-SOAK-SMOKE"));
    let report = run_native_soak_report(profile);

    write_native_soak_report(&report, SOAK_REPORT_PATH)
        .expect("native soak report should be written");
    write_native_audit_artifact(&report, AUDIT_REPORT_PATH)
        .expect("native audit artifact should be written");

    assert!(Path::new(SOAK_REPORT_PATH).exists());
    assert!(Path::new(AUDIT_REPORT_PATH).exists());
    assert_eq!(report.crash_count, 0);
    assert!(report.internal_failure_rate <= 0.001);
    assert!(report.required_fields_valid);
    assert!(report.gate_pass, "native soak gate failed: {:?}", report);
}

#[test]
fn validates_crash_audit_schema_fields() {
    let crash = NativeCrashAuditEvent {
        error_code: "PROCESS_CRASH".to_string(),
        timestamp: now_epoch_ms_string(),
        process_exit_code: Some(101),
        signal: Some("SIGABRT".to_string()),
        last_request_id: Some("req-42".to_string()),
        uptime_sec: 123,
        cause: Some("panic".to_string()),
    };
    assert!(validate_crash_event_required_fields(&crash).is_ok());
}
