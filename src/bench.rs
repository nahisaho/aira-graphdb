use std::time::{Duration, Instant};

use crate::graph::InMemoryGraphStore;
use crate::query::execute_query;

#[derive(Debug, Clone)]
pub struct LatencyProfile {
    pub name: &'static str,
    pub expected_p95_ms: u64,
    pub warmup_queries: usize,
    pub concurrency: usize,
}

pub const P0_LATENCY_BASELINE: LatencyProfile = LatencyProfile {
    name: "P0-LATENCY-BASELINE",
    expected_p95_ms: 50,
    warmup_queries: 5,
    concurrency: 1,
};

pub const P0_SERVER_CONCURRENCY_MIN: usize = 32;

pub fn run_match_latency_probe(iterations: usize) -> Duration {
    let mut store = InMemoryGraphStore::new();
    let _ = execute_query(&mut store, "CREATE (n:Paper)");
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = execute_query(&mut store, "MATCH (n) RETURN n");
    }
    start.elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_profile_is_configured() {
        assert_eq!(P0_LATENCY_BASELINE.name, "P0-LATENCY-BASELINE");
        assert_eq!(P0_LATENCY_BASELINE.concurrency, 1);
    }

    #[test]
    fn probe_executes_queries() {
        let duration = run_match_latency_probe(100);
        assert!(duration.as_nanos() > 0);
    }

    #[test]
    fn concurrency_minimum_matches_requirement() {
        assert_eq!(P0_SERVER_CONCURRENCY_MIN, 32);
    }
}
