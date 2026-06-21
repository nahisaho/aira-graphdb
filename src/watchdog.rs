use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::process::Child;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::audit::{NativeCrashAuditEvent, now_epoch_ms_string};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogCrashReport {
    pub events: Vec<NativeCrashAuditEvent>,
}

pub fn observe_child_exit_as_crash(
    mut child: Child,
    last_request_id: Option<String>,
    started: Instant,
    cause: Option<String>,
) -> io::Result<NativeCrashAuditEvent> {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    let status = child.wait()?;
    #[cfg(unix)]
    let signal = status.signal().map(|v| format!("SIG{v}"));
    #[cfg(not(unix))]
    let signal: Option<String> = None;

    Ok(NativeCrashAuditEvent {
        error_code: "PROCESS_CRASH".to_string(),
        timestamp: now_epoch_ms_string(),
        process_exit_code: status.code(),
        signal,
        last_request_id,
        uptime_sec: started.elapsed().as_secs(),
        cause,
    })
}

pub fn write_watchdog_crash_report<P: AsRef<Path>>(
    report: &WatchdogCrashReport,
    path: P,
) -> io::Result<()> {
    let path_ref = path.as_ref();
    if let Some(parent) = path_ref.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(report)
        .map_err(|e| io::Error::other(format!("serialize watchdog crash report failed: {e}")))?;
    let mut file = File::create(path_ref)?;
    file.write_all(&payload)?;
    file.sync_all()?;
    Ok(())
}
