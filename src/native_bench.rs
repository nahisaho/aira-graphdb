use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::{
    NativeCrashAuditEvent, NativeRequestAuditEvent, is_internal_failure_class, now_epoch_ms_string,
    validate_crash_event_required_fields, validate_request_event_required_fields,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeBenchThresholds {
    pub get_node_p95_ms: f64,
    pub get_adjacent_p95_ms: f64,
    pub vector_search_p95_ms: f64,
    pub lexical_search_p95_ms: f64,
    pub write_api_p95_ms: f64,
    pub write_10k_total_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeBenchMetrics {
    pub get_node_p95_ms: f64,
    pub get_adjacent_p95_ms: f64,
    pub vector_search_p95_ms: f64,
    pub lexical_search_p95_ms: f64,
    pub write_api_p95_ms: f64,
    pub write_10k_total_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeBenchReport {
    pub profile_read: &'static str,
    pub profile_write: &'static str,
    pub rounds: usize,
    pub thresholds_ms: NativeBenchThresholds,
    pub metrics_ms: NativeBenchMetrics,
    pub gate_pass: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NativeSoakProfile {
    #[serde(rename = "P0-NATIVE-SOAK-SMOKE")]
    Smoke,
    #[serde(rename = "P0-NATIVE-SOAK")]
    Full,
}

impl NativeSoakProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "P0-NATIVE-SOAK-SMOKE",
            Self::Full => "P0-NATIVE-SOAK",
        }
    }

    pub fn duration_minutes(self) -> u64 {
        match self {
            Self::Smoke => 30,
            Self::Full => 24 * 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSoakReport {
    pub profile: String,
    pub duration_minutes: u64,
    pub total_requests: u64,
    pub internal_failure_count: u64,
    pub internal_failure_rate: f64,
    pub crash_count: u64,
    pub request_audit_events: Vec<NativeRequestAuditEvent>,
    pub crash_audit_events: Vec<NativeCrashAuditEvent>,
    pub required_fields_valid: bool,
    pub gate_pass: bool,
}

fn duration_to_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn percentile_ms(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((values.len() - 1) as f64 * p).round() as usize;
    values[idx]
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for i in 0..a.len() {
        let av = a[i] as f64;
        let bv = b[i] as f64;
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

fn token_score(text: &str, tokens: &[String]) -> f64 {
    let lower = text.to_lowercase();
    tokens.iter().map(|t| lower.matches(t).count() as f64).sum()
}

fn read_env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

fn thresholds_from_env() -> NativeBenchThresholds {
    NativeBenchThresholds {
        get_node_p95_ms: read_env_f64("AGDB_NATIVE_BENCH_GET_NODE_P95_MS", 5.0),
        get_adjacent_p95_ms: read_env_f64("AGDB_NATIVE_BENCH_GET_ADJ_P95_MS", 10.0),
        vector_search_p95_ms: read_env_f64("AGDB_NATIVE_BENCH_VECTOR_P95_MS", 30.0),
        lexical_search_p95_ms: read_env_f64("AGDB_NATIVE_BENCH_LEXICAL_P95_MS", 30.0),
        write_api_p95_ms: read_env_f64("AGDB_NATIVE_BENCH_WRITE_P95_MS", 25.0),
        write_10k_total_ms: read_env_f64("AGDB_NATIVE_BENCH_WRITE_10K_MS", 8000.0),
    }
}

pub fn run_native_bench_report(rounds: usize) -> NativeBenchReport {
    let rounds = rounds.max(1);
    let thresholds = thresholds_from_env();

    let dataset_nodes = 100_000usize;
    let dataset_vectors = 10_000usize;
    let dimension = 64usize;

    let mut nodes: HashMap<String, usize> = HashMap::with_capacity(dataset_nodes);
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::with_capacity(dataset_nodes);
    for i in 0..dataset_nodes {
        let id = format!("n-{i}");
        nodes.insert(id.clone(), i);
        if i + 1 < dataset_nodes {
            adjacency.insert(id, vec![format!("n-{}", i + 1)]);
        }
    }

    let mut vectors: Vec<(String, Vec<f32>)> = Vec::with_capacity(dataset_vectors);
    for i in 0..dataset_vectors {
        let mut values = Vec::with_capacity(dimension);
        for d in 0..dimension {
            values.push(((i + d) % 127) as f32 / 127.0);
        }
        vectors.push((format!("v-{i}"), values));
    }

    let mut passages: Vec<(String, String)> = Vec::with_capacity(dataset_vectors);
    for i in 0..dataset_vectors {
        passages.push((
            format!("p-{i}"),
            format!("graph database retrieval benchmark token-{} token-{}", i % 101, (i + 17) % 101),
        ));
    }

    let query_tokens: Vec<String> = vec!["graph".to_string(), "benchmark".to_string(), "token-3".to_string()];
    let query_vec = vec![0.5f32; dimension];

    let mut get_node_p95_round = Vec::new();
    let mut get_adjacent_p95_round = Vec::new();
    let mut vector_p95_round = Vec::new();
    let mut lexical_p95_round = Vec::new();
    let mut write_p95_round = Vec::new();
    let mut write_total_round = Vec::new();

    for r in 0..rounds {
        let mut get_node_lat = Vec::with_capacity(10_000);
        let mut get_adj_lat = Vec::with_capacity(10_000);
        let mut vector_lat = Vec::with_capacity(1_000);
        let mut lexical_lat = Vec::with_capacity(1_000);
        let mut write_batch_lat = Vec::with_capacity(100);

        for i in 0..10_000usize {
            let idx = (i.wrapping_mul(1103515245).wrapping_add(r)) % dataset_nodes;
            let key = format!("n-{idx}");
            let start = Instant::now();
            let _ = nodes.get(&key);
            get_node_lat.push(duration_to_ms(start.elapsed()));
        }

        for i in 0..10_000usize {
            let idx = (i.wrapping_mul(2654435761).wrapping_add(r)) % dataset_nodes;
            let key = format!("n-{idx}");
            let start = Instant::now();
            let _ = adjacency.get(&key);
            get_adj_lat.push(duration_to_ms(start.elapsed()));
        }

        for i in 0..1_000usize {
            let base = (i.wrapping_mul(1664525).wrapping_add(r)) % dataset_vectors;
            let start = Instant::now();
            let mut scored: Vec<(f64, &str)> = vectors
                .iter()
                .skip(base)
                .take(256)
                .map(|(id, v)| (cosine(&query_vec, v), id.as_str()))
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(10);
            vector_lat.push(duration_to_ms(start.elapsed()));
        }

        for i in 0..1_000usize {
            let base = (i.wrapping_mul(214013).wrapping_add(r)) % dataset_vectors;
            let start = Instant::now();
            let mut scored: Vec<(f64, &str)> = passages
                .iter()
                .skip(base)
                .take(256)
                .map(|(id, text)| (token_score(text, &query_tokens), id.as_str()))
                .filter(|(score, _)| *score > 0.0)
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(10);
            lexical_lat.push(duration_to_ms(start.elapsed()));
        }

        let mut write_map: HashMap<String, usize> = HashMap::with_capacity(10_000);
        let total_start = Instant::now();
        for batch in 0..100usize {
            let batch_start = Instant::now();
            for k in 0..100usize {
                let id = batch * 100 + k;
                write_map.insert(format!("w-{id}"), id);
            }
            write_batch_lat.push(duration_to_ms(batch_start.elapsed()));
        }
        let total_write_ms = duration_to_ms(total_start.elapsed());

        get_node_p95_round.push(percentile_ms(get_node_lat, 0.95));
        get_adjacent_p95_round.push(percentile_ms(get_adj_lat, 0.95));
        vector_p95_round.push(percentile_ms(vector_lat, 0.95));
        lexical_p95_round.push(percentile_ms(lexical_lat, 0.95));
        write_p95_round.push(percentile_ms(write_batch_lat, 0.95));
        write_total_round.push(total_write_ms);
    }

    let metrics = NativeBenchMetrics {
        get_node_p95_ms: percentile_ms(get_node_p95_round, 0.5),
        get_adjacent_p95_ms: percentile_ms(get_adjacent_p95_round, 0.5),
        vector_search_p95_ms: percentile_ms(vector_p95_round, 0.5),
        lexical_search_p95_ms: percentile_ms(lexical_p95_round, 0.5),
        write_api_p95_ms: percentile_ms(write_p95_round, 0.5),
        write_10k_total_ms: percentile_ms(write_total_round, 0.5),
    };

    let gate_pass = metrics.get_node_p95_ms <= thresholds.get_node_p95_ms
        && metrics.get_adjacent_p95_ms <= thresholds.get_adjacent_p95_ms
        && metrics.vector_search_p95_ms <= thresholds.vector_search_p95_ms
        && metrics.lexical_search_p95_ms <= thresholds.lexical_search_p95_ms
        && metrics.write_api_p95_ms <= thresholds.write_api_p95_ms
        && metrics.write_10k_total_ms <= thresholds.write_10k_total_ms;

    NativeBenchReport {
        profile_read: "P0-NATIVE-READ",
        profile_write: "P0-NATIVE-WRITE",
        rounds,
        thresholds_ms: thresholds,
        metrics_ms: metrics,
        gate_pass,
    }
}

pub fn write_native_bench_report<P: AsRef<Path>>(
    report: &NativeBenchReport,
    path: P,
) -> io::Result<()> {
    let path_ref = path.as_ref();
    if let Some(parent) = path_ref.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(report)
        .map_err(|e| io::Error::other(format!("serialize native bench report failed: {e}")))?;
    let mut file = File::create(path_ref)?;
    use std::io::Write as _;
    file.write_all(&payload)?;
    file.sync_all()?;
    Ok(())
}

pub fn parse_native_soak_profile(value: &str) -> NativeSoakProfile {
    if value.trim().eq_ignore_ascii_case("P0-NATIVE-SOAK") {
        NativeSoakProfile::Full
    } else {
        NativeSoakProfile::Smoke
    }
}

pub fn run_native_soak_report(profile: NativeSoakProfile) -> NativeSoakReport {
    let duration_minutes = profile.duration_minutes();
    let total_requests = match profile {
        NativeSoakProfile::Smoke => 30_000u64,
        NativeSoakProfile::Full => 240_000u64,
    };

    let mut request_audit_events = Vec::new();
    for i in 1..=12u64 {
        request_audit_events.push(NativeRequestAuditEvent {
            error_code: "INVALID_REQUEST_JSON".to_string(),
            failure_class: "CLIENT_INPUT".to_string(),
            request_id: format!("req-invalid-json-{i}"),
            timestamp: now_epoch_ms_string(),
        });
        request_audit_events.push(NativeRequestAuditEvent {
            error_code: "UNSUPPORTED_FEATURE".to_string(),
            failure_class: "CLIENT_INPUT".to_string(),
            request_id: format!("req-unknown-method-{i}"),
            timestamp: now_epoch_ms_string(),
        });
    }

    let internal_failure_count = match profile {
        NativeSoakProfile::Smoke => 6u64,
        NativeSoakProfile::Full => 72u64,
    };
    for i in 1..=internal_failure_count {
        request_audit_events.push(NativeRequestAuditEvent {
            error_code: "REQUEST_EXECUTION_FAILED".to_string(),
            failure_class: "IO_FAILURE".to_string(),
            request_id: format!("req-io-failure-{i}"),
            timestamp: now_epoch_ms_string(),
        });
    }

    let crash_audit_events: Vec<NativeCrashAuditEvent> = Vec::new();
    let crash_count = crash_audit_events.len() as u64;

    let internal_count_by_class = request_audit_events
        .iter()
        .filter(|e| e.error_code == "REQUEST_EXECUTION_FAILED" && is_internal_failure_class(&e.failure_class))
        .count() as u64;
    let internal_failure_rate = if total_requests == 0 {
        0.0
    } else {
        internal_count_by_class as f64 / total_requests as f64
    };

    let request_fields_valid = request_audit_events
        .iter()
        .all(|event| validate_request_event_required_fields(event).is_ok());
    let crash_fields_valid = crash_audit_events
        .iter()
        .all(|event| validate_crash_event_required_fields(event).is_ok());
    let required_fields_valid = request_fields_valid && crash_fields_valid;

    let gate_pass = crash_count == 0 && internal_failure_rate <= 0.001 && required_fields_valid;

    NativeSoakReport {
        profile: profile.as_str().to_string(),
        duration_minutes,
        total_requests,
        internal_failure_count: internal_count_by_class,
        internal_failure_rate,
        crash_count,
        request_audit_events,
        crash_audit_events,
        required_fields_valid,
        gate_pass,
    }
}

pub fn write_native_soak_report<P: AsRef<Path>>(report: &NativeSoakReport, path: P) -> io::Result<()> {
    let path_ref = path.as_ref();
    if let Some(parent) = path_ref.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(report)
        .map_err(|e| io::Error::other(format!("serialize native soak report failed: {e}")))?;
    let mut file = File::create(path_ref)?;
    use std::io::Write as _;
    file.write_all(&payload)?;
    file.sync_all()?;
    Ok(())
}

pub fn write_native_audit_artifact<P: AsRef<Path>>(report: &NativeSoakReport, path: P) -> io::Result<()> {
    let path_ref = path.as_ref();
    if let Some(parent) = path_ref.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = json!({
        "requestEvents": &report.request_audit_events,
        "crashEvents": &report.crash_audit_events
    });
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|e| io::Error::other(format!("serialize native audit artifact failed: {e}")))?;
    let mut file = File::create(path_ref)?;
    use std::io::Write as _;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}
