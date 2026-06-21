use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use aira_graphdb::watchdog::{
    WatchdogCrashReport, observe_child_exit_as_crash, write_watchdog_crash_report,
};

const WATCHDOG_REPORT_PATH: &str = "artifacts/watchdog-crash-report.json";

#[test]
fn records_process_crash_for_kill_level_exit() {
    let mut child = Command::new("sleep")
        .arg("10")
        .spawn()
        .expect("sleep process should start");
    let pid = child.id();
    thread::sleep(Duration::from_millis(50));
    child.kill().expect("kill should succeed");

    let started = Instant::now();
    let event = observe_child_exit_as_crash(
        child,
        Some("req-kill-1".to_string()),
        started,
        Some("killed by watchdog test".to_string()),
    )
    .expect("watchdog should capture exit");
    assert_eq!(event.error_code, "PROCESS_CRASH");
    assert!(event.process_exit_code.is_some() || event.signal.is_some());
    assert_eq!(event.last_request_id.as_deref(), Some("req-kill-1"));

    let report = WatchdogCrashReport {
        events: vec![event],
    };
    write_watchdog_crash_report(&report, WATCHDOG_REPORT_PATH).expect("report should be written");
    assert!(Path::new(WATCHDOG_REPORT_PATH).exists());
    assert!(pid > 0);
}
