use std::path::Path;

use aira_graphdb::native_bench::{run_native_bench_report, write_native_bench_report};

const REPORT_PATH: &str = "artifacts/native-bench-report.json";

#[test]
fn writes_native_bench_artifact_and_meets_gate() {
    let report = run_native_bench_report(3);
    write_native_bench_report(&report, REPORT_PATH).expect("native bench report should be written");
    assert!(Path::new(REPORT_PATH).exists());
    assert!(
        report.gate_pass,
        "native perf gate failed: metrics={:?} thresholds={:?}",
        report.metrics_ms,
        report.thresholds_ms
    );
}
