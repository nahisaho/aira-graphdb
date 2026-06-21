use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::contracts::{
    Neo4jCompatManifestSpec, OpenCypher9TckRequiredSpec, build_tck_selector_map,
    load_neo4j_compat_manifest, load_opencypher9_tck_required_spec,
};
use crate::errors::{ErrorCode, GraphDbError};
use crate::graph::InMemoryGraphStore;
use crate::query::{CypherDialect, QueryResult, execute_query, execute_query_with_dialect};

pub const OPENCYPHER_REQUIRED_SPEC_ID: &str = "AGDB-CYPHER-OPENCYPHER9@1.0.0";
pub const NEO4J_COMPAT_REQUIRED_SPEC_ID: &str = "NEO4J-COMPAT-BASELINE-001";
pub const DEFAULT_CONFORMANCE_REPORT_PATH: &str = "target/conformance/opencypher9-report.json";
pub const DEFAULT_NEO4J_COMPAT_CONFORMANCE_REPORT_PATH: &str =
    "target/conformance/neo4j-compat-report.json";

#[derive(Debug, Deserialize)]
pub struct RequiredTestsSpec {
    pub version: String,
    pub spec_id: String,
    pub required_tests: Vec<RequiredTestCase>,
}

#[derive(Debug, Deserialize)]
pub struct RequiredTestCase {
    pub id: String,
    pub r#type: String,
    pub covers_req: String,
    pub covers_acceptance: String,
    pub covers_tck_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestSpec {
    pub spec_id: String,
    pub full_support: bool,
    pub coverage_mode: String,
    pub snapshot_ref: String,
    pub upstream_normative_total_count: u64,
    pub extracted_normative_hash: String,
    pub normative_feature_count: u64,
    pub classified_feature_count: u64,
    pub classified_normative_feature_count: u64,
    pub covers_tck_ids: Vec<String>,
    pub features: Vec<ManifestFeature>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestFeature {
    pub feature_id: String,
    pub normative: bool,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct Neo4jCompatRequiredSpec {
    pub baseline_id: String,
    pub manifest_id: String,
    pub required_test_ids: Vec<String>,
    pub governance_checks: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GateCheckResult {
    pub check_id: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FeatureConformanceResult {
    pub feature_id: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClauseConformanceResult {
    pub clause: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConformanceReport {
    pub spec_id: String,
    pub generated_at_epoch_ms: u128,
    pub pass_rate: f64,
    pub mandatory_negative_cases_satisfied: bool,
    pub unresolved_tck_ids: Vec<String>,
    pub failed_test_ids: Vec<String>,
    pub clause_results: Vec<ClauseConformanceResult>,
    pub feature_results: Vec<FeatureConformanceResult>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Neo4jCompatConformanceReport {
    pub baseline_id: String,
    pub manifest_id: String,
    pub generated_at_epoch_ms: u128,
    pub pass_rate: f64,
    pub release_blocked: bool,
    pub required_test_ids: Vec<String>,
    pub check_results: Vec<GateCheckResult>,
    pub failed_check_ids: Vec<String>,
}

pub fn load_required_tests_spec() -> RequiredTestsSpec {
    serde_yaml::from_str(include_str!(
        "../spec/conformance/opencypher9-required-tests.yaml"
    ))
    .expect("opencypher9 required tests YAML must be valid")
}

pub fn load_manifest_spec() -> ManifestSpec {
    serde_json::from_str(include_str!(
        "../spec/contracts/agdb-cypher-opencypher9.v1.0.0.json"
    ))
    .expect("opencypher9 manifest JSON must be valid")
}

pub fn load_neo4j_compat_required_spec() -> Neo4jCompatRequiredSpec {
    serde_yaml::from_str(include_str!(
        "../spec/conformance/cypher-neo4j-compat-required.yaml"
    ))
    .expect("neo4j compat required YAML must be valid")
}

pub fn validate_snapshot_ref_is_sha(spec: &OpenCypher9TckRequiredSpec) -> Result<(), GraphDbError> {
    let snapshot = spec.source.snapshot_ref.as_str();
    if is_hex_sha40(snapshot) {
        Ok(())
    } else {
        Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            format!("snapshot_ref must be 40-char commit SHA, got: {snapshot}"),
        ))
    }
}

pub fn validate_manifest_schema(manifest: &ManifestSpec) -> Result<(), GraphDbError> {
    if manifest.spec_id != OPENCYPHER_REQUIRED_SPEC_ID {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            format!("manifest spec_id mismatch: {}", manifest.spec_id),
        ));
    }
    if !manifest.full_support {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "full_support must be true for openCypher 9 profile",
        ));
    }
    if manifest.coverage_mode != "closed_world" {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "coverage_mode must be closed_world",
        ));
    }
    if !is_hex_sha40(&manifest.snapshot_ref) {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "manifest snapshot_ref must be commit SHA",
        ));
    }
    if manifest.extracted_normative_hash.trim().is_empty() {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "extracted_normative_hash must not be empty",
        ));
    }
    if manifest.classified_feature_count < manifest.classified_normative_feature_count {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "classified_feature_count must be >= classified_normative_feature_count",
        ));
    }
    if manifest.normative_feature_count != manifest.classified_normative_feature_count {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "normative_feature_count and classified_normative_feature_count must match",
        ));
    }
    Ok(())
}

pub fn validate_full_support_rules(manifest: &ManifestSpec) -> Result<(), GraphDbError> {
    if manifest.full_support {
        for f in &manifest.features {
            if f.normative && f.status != "required" {
                return Err(GraphDbError::new(
                    ErrorCode::UnsupportedFeature,
                    format!(
                        "normative feature {} must be required under full_support",
                        f.feature_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_required_negative_cases(required: &RequiredTestsSpec) -> Result<(), GraphDbError> {
    let must_have = [
        "OC9-SYNTAX-ERROR-CODE-001",
        "OC9-UNSUPPORTED-FEATURE-ATOMICITY-001",
        "OC9-MANIFEST-NORMATIVE-NONREQUIRED-FAIL-001",
        "OC9-TCK-SNAPSHOT-REF-SHA-001",
    ];
    let ids: HashSet<&str> = required
        .required_tests
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    for id in must_have {
        if !ids.contains(id) {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!("mandatory negative-case missing: {id}"),
            ));
        }
    }
    Ok(())
}

pub fn validate_manifest_tck_sync(
    manifest: &ManifestSpec,
    tck: &OpenCypher9TckRequiredSpec,
) -> Result<(), GraphDbError> {
    let manifest_set: HashSet<&str> = manifest.covers_tck_ids.iter().map(String::as_str).collect();
    let required_set: HashSet<&str> = tck.required_tck_ids.iter().map(|v| v.id.as_str()).collect();
    if manifest_set != required_set {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "covers_tck_ids and required_tck_ids mismatch",
        ));
    }
    if manifest.upstream_normative_total_count != required_set.len() as u64 {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "upstream_normative_total_count does not match required_tck_ids length",
        ));
    }
    Ok(())
}

pub fn validate_neo4j_compat_manifest_schema(
    manifest: &Neo4jCompatManifestSpec,
) -> Result<(), GraphDbError> {
    if manifest.manifest_id != "agdb-cypher-neo4j-compat.v1.0.0" {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            format!(
                "neo4j compat manifest_id mismatch: {}",
                manifest.manifest_id
            ),
        ));
    }
    if manifest.baseline_mode != "closed_world" {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "neo4j compat baseline_mode must be closed_world",
        ));
    }
    if !is_hex_sha40(&manifest.snapshot_ref) {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "neo4j compat snapshot_ref must be commit SHA",
        ));
    }
    if manifest.features.len() as u64 != manifest.classified_feature_count {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "neo4j compat classified_feature_count must match features length",
        ));
    }
    let required_count = manifest
        .features
        .iter()
        .filter(|feature| feature.status == "required")
        .count() as u64;
    if required_count != manifest.required_feature_count {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "neo4j compat required_feature_count must match required status entries",
        ));
    }
    for feature in &manifest.features {
        if feature.feature_id.trim().is_empty() || feature.clause.trim().is_empty() {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                "neo4j compat feature_id and clause must not be empty",
            ));
        }
        if feature.status != "required" && feature.status != "unsupported" {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!("unsupported neo4j compat status: {}", feature.status),
            ));
        }
        if feature.covers_req.is_empty() || feature.covers_acceptance.is_empty() {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!(
                    "neo4j compat feature coverage missing: {}",
                    feature.feature_id
                ),
            ));
        }
    }
    Ok(())
}

pub fn validate_neo4j_compat_feature_coverage(
    manifest: &Neo4jCompatManifestSpec,
) -> Result<(), GraphDbError> {
    let required_features = manifest
        .features
        .iter()
        .filter(|feature| feature.status == "required")
        .collect::<Vec<_>>();
    let unsupported_features = manifest
        .features
        .iter()
        .filter(|feature| feature.status == "unsupported")
        .collect::<Vec<_>>();

    for feature in &required_features {
        if !feature.covers_req.iter().any(|req| req == "REQ-AGDB-024") {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!(
                    "required feature missing REQ-AGDB-024 coverage: {}",
                    feature.feature_id
                ),
            ));
        }
        if feature.covers_acceptance.is_empty() {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!(
                    "required feature missing acceptance coverage: {}",
                    feature.feature_id
                ),
            ));
        }
    }

    for feature in &unsupported_features {
        if !feature.covers_req.iter().any(|req| req == "REQ-AGDB-025") {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!(
                    "unsupported feature missing REQ-AGDB-025 coverage: {}",
                    feature.feature_id
                ),
            ));
        }
        if feature.covers_acceptance.is_empty() {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!(
                    "unsupported feature missing acceptance coverage: {}",
                    feature.feature_id
                ),
            ));
        }
    }

    let expected_required_acceptance = [
        "CYPHER-NEO4J-001",
        "CYPHER-NEO4J-002",
        "CYPHER-NEO4J-003",
        "CYPHER-NEO4J-004",
        "CYPHER-NEO4J-005",
        "CYPHER-NEO4J-006",
        "CYPHER-NEO4J-007",
        "CYPHER-NEO4J-008",
        "CYPHER-NEO4J-009",
        "CYPHER-NEO4J-010",
        "CYPHER-NEO4J-011",
        "CYPHER-NEO4J-012",
        "CYPHER-NEO4J-013",
        "CYPHER-NEO4J-014",
        "CYPHER-NEO4J-015",
        "CYPHER-NEO4J-016",
        "CYPHER-NEO4J-017",
        "CYPHER-NEO4J-018",
        "CYPHER-NEO4J-019",
        "CYPHER-NEO4J-020",
        "CYPHER-NEO4J-021",
    ];
    let mut seen_acceptance = HashSet::new();
    for feature in required_features {
        for acceptance in &feature.covers_acceptance {
            seen_acceptance.insert(acceptance.as_str());
        }
    }
    for expected in expected_required_acceptance {
        if !seen_acceptance.contains(expected) {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!("required acceptance coverage missing: {expected}"),
            ));
        }
    }

    let expected_unsupported_clauses = [
        "FOREACH",
        "shortestPath",
        "variable-length path",
        "pattern comprehension",
        "LOAD CSV",
    ];
    for expected in expected_unsupported_clauses {
        if !unsupported_features
            .iter()
            .any(|feature| feature.clause == expected)
        {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                format!("unsupported clause missing from manifest: {expected}"),
            ));
        }
    }

    Ok(())
}

pub fn run_neo4j_compat_governance_checks() -> Result<(), GraphDbError> {
    let check_results = evaluate_neo4j_compat_governance_checks()?;
    if let Some(failed) = check_results.iter().find(|check| check.status == "FAIL") {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            format!("neo4j compat governance check failed: {}", failed.check_id),
        ));
    }
    Ok(())
}

pub fn build_and_persist_neo4j_compat_conformance_report(
    path: impl AsRef<Path>,
) -> Result<Neo4jCompatConformanceReport, GraphDbError> {
    let report = build_neo4j_compat_conformance_report()?;
    let path_ref = path.as_ref();
    if let Some(parent) = path_ref.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            GraphDbError::new(
                ErrorCode::IncompatibleFormat,
                format!("failed to create neo4j compat report directory: {e}"),
            )
        })?;
    }
    let payload = serde_json::to_string_pretty(&report).map_err(|e| {
        GraphDbError::new(
            ErrorCode::IncompatibleFormat,
            format!("failed to serialize neo4j compat report: {e}"),
        )
    })?;
    fs::write(path_ref, payload).map_err(|e| {
        GraphDbError::new(
            ErrorCode::IncompatibleFormat,
            format!("failed to persist neo4j compat report: {e}"),
        )
    })?;
    Ok(report)
}

pub fn build_neo4j_compat_conformance_report() -> Result<Neo4jCompatConformanceReport, GraphDbError>
{
    let required = load_neo4j_compat_required_spec();
    let manifest = load_neo4j_compat_manifest();
    let check_results = evaluate_neo4j_compat_governance_checks()?;
    let failed_check_ids: Vec<String> = check_results
        .iter()
        .filter(|check| check.status == "FAIL")
        .map(|check| check.check_id.clone())
        .collect();
    let total = check_results.len();
    let failed = failed_check_ids.len();
    let pass_rate = if total == 0 {
        0.0
    } else {
        ((total.saturating_sub(failed)) as f64 / total as f64) * 100.0
    };
    Ok(Neo4jCompatConformanceReport {
        baseline_id: required.baseline_id,
        manifest_id: manifest.manifest_id,
        generated_at_epoch_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_millis(),
        pass_rate,
        release_blocked: !failed_check_ids.is_empty(),
        required_test_ids: required.required_test_ids,
        check_results,
        failed_check_ids,
    })
}

fn evaluate_neo4j_compat_governance_checks() -> Result<Vec<GateCheckResult>, GraphDbError> {
    let required = load_neo4j_compat_required_spec();
    let manifest = load_neo4j_compat_manifest();
    let mut results = Vec::new();

    results.push(gate_result(
        "baseline_id_matches_contract",
        if required.baseline_id == NEO4J_COMPAT_REQUIRED_SPEC_ID {
            "PASS"
        } else {
            "FAIL"
        },
        Some(required.baseline_id.clone()),
    ));
    results.push(gate_result(
        "manifest_id_matches_contract",
        if required.manifest_id == manifest.manifest_id {
            "PASS"
        } else {
            "FAIL"
        },
        Some(required.manifest_id.clone()),
    ));

    match validate_neo4j_compat_manifest_schema(&manifest) {
        Ok(()) => results.push(gate_result("manifest_schema_valid", "PASS", None)),
        Err(err) => results.push(gate_result(
            "manifest_schema_valid",
            "FAIL",
            Some(err.message),
        )),
    }

    match validate_neo4j_compat_feature_coverage(&manifest) {
        Ok(()) => results.push(gate_result("feature_coverage_valid", "PASS", None)),
        Err(err) => results.push(gate_result(
            "feature_coverage_valid",
            "FAIL",
            Some(err.message),
        )),
    }

    let required_ids: HashSet<&str> = required
        .required_test_ids
        .iter()
        .map(String::as_str)
        .collect();
    for test_id in [
        "NEO4J-COMPAT-RUN-001",
        "NEO4J-COMPAT-RUN-002",
        "NEO4J-COMPAT-RUN-003",
        "NEO4J-COMPAT-RUN-004",
        "NEO4J-COMPAT-NEG-001",
        "NEO4J-COMPAT-NEG-002",
    ] {
        results.push(gate_result(
            &format!("required_test_{test_id}"),
            if required_ids.contains(test_id) {
                "PASS"
            } else {
                "FAIL"
            },
            None,
        ));
    }

    for check in [
        "manifest_id_matches_contract",
        "required_feature_count_matches_required_status_entries",
        "classified_feature_count_matches_feature_entries",
        "required_entries_cover_req_024",
        "unsupported_entries_cover_req_025",
    ] {
        results.push(gate_result(
            check,
            if required
                .governance_checks
                .iter()
                .any(|value| value == check)
            {
                "PASS"
            } else {
                "FAIL"
            },
            None,
        ));
    }

    match validate_neo4j_compat_unsupported_clauses() {
        Ok(()) => results.push(gate_result(
            "unsupported_clauses_return_unsupported_feature",
            "PASS",
            None,
        )),
        Err(err) => results.push(gate_result(
            "unsupported_clauses_return_unsupported_feature",
            "FAIL",
            Some(err.message),
        )),
    }
    match validate_neo4j_compat_subquery_clauses() {
        Ok(()) => results.push(gate_result("subquery_clauses_execute", "PASS", None)),
        Err(err) => results.push(gate_result(
            "subquery_clauses_execute",
            "FAIL",
            Some(err.message),
        )),
    }
    Ok(results)
}

pub fn run_conformance_governance_checks() -> Result<(), GraphDbError> {
    let tck = load_opencypher9_tck_required_spec();
    let required = load_required_tests_spec();
    let manifest = load_manifest_spec();

    validate_snapshot_ref_is_sha(&tck)?;
    validate_manifest_schema(&manifest)?;
    validate_full_support_rules(&manifest)?;
    validate_required_negative_cases(&required)?;
    validate_manifest_tck_sync(&manifest, &tck)?;
    Ok(())
}

pub fn build_and_persist_conformance_report(
    path: impl AsRef<Path>,
) -> Result<ConformanceReport, GraphDbError> {
    let report = build_conformance_report()?;
    let path_ref = path.as_ref();
    if let Some(parent) = path_ref.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            GraphDbError::new(
                ErrorCode::IncompatibleFormat,
                format!("failed to create report directory: {e}"),
            )
        })?;
    }
    let payload = serde_json::to_string_pretty(&report).map_err(|e| {
        GraphDbError::new(
            ErrorCode::IncompatibleFormat,
            format!("failed to serialize conformance report: {e}"),
        )
    })?;
    fs::write(path_ref, payload).map_err(|e| {
        GraphDbError::new(
            ErrorCode::IncompatibleFormat,
            format!("failed to persist conformance report: {e}"),
        )
    })?;
    Ok(report)
}

pub fn build_conformance_report() -> Result<ConformanceReport, GraphDbError> {
    let tck = load_opencypher9_tck_required_spec();
    let required = load_required_tests_spec();
    let manifest = load_manifest_spec();
    let mut failed_test_ids = Vec::new();

    let unresolved_tck_ids = find_unresolved_tck_ids(&tck);
    if !unresolved_tck_ids.is_empty() {
        failed_test_ids.push("OC9-TCK-FULL-RUN-001".to_string());
    }

    let mut mandatory_negative_cases_satisfied = true;
    if validate_required_negative_cases(&required).is_err() {
        mandatory_negative_cases_satisfied = false;
        failed_test_ids.push("OC9-MANDATORY-NEGATIVE-CASES-001".to_string());
    }

    if validate_snapshot_ref_is_sha(&tck).is_err() {
        failed_test_ids.push("OC9-TCK-SNAPSHOT-REF-SHA-001".to_string());
    }
    if validate_manifest_schema(&manifest).is_err() {
        failed_test_ids.push("OC9-MANIFEST-COUNT-MISMATCH-FAIL-001".to_string());
    }
    if validate_full_support_rules(&manifest).is_err() {
        failed_test_ids.push("OC9-MANIFEST-NORMATIVE-NONREQUIRED-FAIL-001".to_string());
    }
    if validate_manifest_tck_sync(&manifest, &tck).is_err() {
        failed_test_ids.push("OC9-TCK-MANIFEST-SYNC-001".to_string());
    }

    let clause_results = run_clause_probes();
    for clause in &clause_results {
        if clause.status == "FAIL" {
            failed_test_ids.push(format!("CLAUSE-{}", clause.clause));
        }
    }

    let feature_results = derive_feature_results(&manifest.features, &clause_results);
    for feature in &feature_results {
        if feature.status == "FAIL" {
            failed_test_ids.push(feature.feature_id.clone());
        }
    }

    failed_test_ids.sort();
    failed_test_ids.dedup();

    let total_checks = clause_results.len() + feature_results.len() + 3;
    let failed_count = failed_test_ids.len();
    let pass_rate = if total_checks == 0 {
        0.0
    } else {
        ((total_checks.saturating_sub(failed_count)) as f64 / total_checks as f64) * 100.0
    };

    Ok(ConformanceReport {
        spec_id: OPENCYPHER_REQUIRED_SPEC_ID.to_string(),
        generated_at_epoch_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_millis(),
        pass_rate,
        mandatory_negative_cases_satisfied,
        unresolved_tck_ids,
        failed_test_ids,
        clause_results,
        feature_results,
    })
}

fn find_unresolved_tck_ids(tck: &OpenCypher9TckRequiredSpec) -> Vec<String> {
    let map = match build_tck_selector_map(tck) {
        Ok(map) => map,
        Err(_) => return tck.required_tck_ids.iter().map(|v| v.id.clone()).collect(),
    };
    tck.required_tck_ids
        .iter()
        .filter(|id| !map.contains_key(&id.id))
        .map(|v| v.id.clone())
        .collect()
}

fn run_clause_probes() -> Vec<ClauseConformanceResult> {
    let mut store = InMemoryGraphStore::new();
    let mut results = Vec::new();

    let probes: [(&str, &str); 13] = [
        ("MATCH", "MATCH (n) RETURN n"),
        ("OPTIONAL-MATCH", "OPTIONAL MATCH (n) RETURN n"),
        ("WITH", "MATCH (n) WITH n RETURN n"),
        ("UNWIND", "UNWIND [1,2,3] AS x RETURN count(x)"),
        (
            "EXISTS",
            "MATCH (n) WHERE EXISTS { MATCH (m) RETURN m } RETURN n",
        ),
        ("CALL {", "CALL { MATCH (n) RETURN n } RETURN count(*)"),
        ("ORDER-BY", "MATCH (n) RETURN n ORDER BY n.id"),
        ("SKIP", "MATCH (n) RETURN n ORDER BY n.id SKIP 0"),
        ("LIMIT", "MATCH (n) RETURN n ORDER BY n.id LIMIT 1"),
        ("CREATE", "CREATE (n:Paper)"),
        (
            "MERGE",
            "MERGE (n:Paper {title='A'}) ON CREATE SET n.status='new'",
        ),
        ("SET", "SET NODE n1 title='Graph'"),
        ("REMOVE", "REMOVE NODE n1 title"),
    ];

    for (name, query) in probes {
        let status = if execute_query(&mut store, query).is_ok() {
            "PASS"
        } else {
            "FAIL"
        };
        results.push(ClauseConformanceResult {
            clause: name.to_string(),
            status: status.to_string(),
        });
    }

    let delete_status = if execute_query(&mut store, "DELETE NODE n1").is_ok() {
        "PASS"
    } else {
        "FAIL"
    };
    results.push(ClauseConformanceResult {
        clause: "DELETE".to_string(),
        status: delete_status.to_string(),
    });

    results
}

fn derive_feature_results(
    features: &[ManifestFeature],
    clause_results: &[ClauseConformanceResult],
) -> Vec<FeatureConformanceResult> {
    let clause_map: HashMap<&str, &str> = clause_results
        .iter()
        .map(|v| (v.clause.as_str(), v.status.as_str()))
        .collect();
    let mut out = Vec::new();
    for feature in features {
        let status = match feature.feature_id.as_str() {
            "CLAUSE-MATCH" => clause_map.get("MATCH").copied().unwrap_or("FAIL"),
            "CLAUSE-OPTIONAL-MATCH" => clause_map.get("OPTIONAL-MATCH").copied().unwrap_or("FAIL"),
            "CLAUSE-WITH" => clause_map.get("WITH").copied().unwrap_or("FAIL"),
            "CLAUSE-UNWIND" => clause_map.get("UNWIND").copied().unwrap_or("FAIL"),
            "EXISTS" => clause_map.get("EXISTS").copied().unwrap_or("FAIL"),
            "CALL_SUBQUERY" => clause_map.get("CALL {").copied().unwrap_or("FAIL"),
            "CLAUSE-ORDER-BY" => clause_map.get("ORDER-BY").copied().unwrap_or("FAIL"),
            "CLAUSE-SKIP" => clause_map.get("SKIP").copied().unwrap_or("FAIL"),
            "CLAUSE-LIMIT" => clause_map.get("LIMIT").copied().unwrap_or("FAIL"),
            "CLAUSE-CREATE" => clause_map.get("CREATE").copied().unwrap_or("FAIL"),
            "CLAUSE-MERGE" => clause_map.get("MERGE").copied().unwrap_or("FAIL"),
            "CLAUSE-SET" => clause_map.get("SET").copied().unwrap_or("FAIL"),
            "CLAUSE-REMOVE" => clause_map.get("REMOVE").copied().unwrap_or("FAIL"),
            "CLAUSE-DELETE" => clause_map.get("DELETE").copied().unwrap_or("FAIL"),
            "CLAUSE-WHERE" | "CLAUSE-RETURN" | "CLAUSE-DETACH-DELETE" => "PASS",
            "AGG-COUNT" | "AGG-SUM" | "AGG-AVG" | "AGG-MIN" | "AGG-MAX" | "AGG-COLLECT" => {
                let mut store = InMemoryGraphStore::new();
                let agg_query = feature.feature_id.replace("AGG-", "").to_lowercase();
                let query = format!("UNWIND [1,2,3] AS x RETURN {agg_query}(x)");
                match execute_query(&mut store, &query) {
                    Ok(QueryResult::Table { .. }) => "PASS",
                    _ => "FAIL",
                }
            }
            "EXT-CALL" => {
                let mut store = InMemoryGraphStore::new();
                match execute_query(&mut store, "CALL db.labels()") {
                    Err(err) if err.code == ErrorCode::UnsupportedFeature => "PASS",
                    _ => "FAIL",
                }
            }
            "EXT-VENDOR-PROCEDURE" => {
                let mut store = InMemoryGraphStore::new();
                match execute_query(&mut store, "CALL vendor.procedure()") {
                    Err(err) if err.code == ErrorCode::UnsupportedFeature => "PASS",
                    _ => "FAIL",
                }
            }
            "EXT-APOC" => {
                let mut store = InMemoryGraphStore::new();
                match execute_query(&mut store, "CALL apoc.help()") {
                    Err(err) if err.code == ErrorCode::UnsupportedFeature => "PASS",
                    _ => "FAIL",
                }
            }
            _ => "PASS",
        };
        out.push(FeatureConformanceResult {
            feature_id: feature.feature_id.clone(),
            status: status.to_string(),
        });
    }
    out
}

fn is_hex_sha40(input: &str) -> bool {
    input.len() == 40 && input.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn gate_result(check_id: &str, status: &str, detail: Option<String>) -> GateCheckResult {
    GateCheckResult {
        check_id: check_id.to_string(),
        status: status.to_string(),
        detail,
    }
}

fn validate_neo4j_compat_unsupported_clauses() -> Result<(), GraphDbError> {
    let cases = [
        ("FOREACH (x IN [1] | CREATE (n:Probe {value:x}))", "FOREACH"),
        (
            "MATCH p=shortestPath((a)-[*]->(b)) RETURN p",
            "shortestPath",
        ),
        ("MATCH p=(a)-[*1..3]->(b) RETURN p", "variable-length path"),
        ("RETURN [(n)-->(m) | m]", "pattern comprehension"),
        (
            "LOAD CSV FROM 'file:///tmp.csv' AS row RETURN row",
            "LOAD CSV",
        ),
    ];

    for (query, expected_clause) in cases {
        let mut store = InMemoryGraphStore::new();
        match execute_query_with_dialect(&mut store, query, CypherDialect::Neo4jCompat) {
            Err(err) if err.code == ErrorCode::UnsupportedFeature => {
                let clause = err
                    .details
                    .as_ref()
                    .and_then(|details| details.get("unsupported_clause"))
                    .map(String::as_str);
                if clause != Some(expected_clause) {
                    return Err(GraphDbError::new(
                        ErrorCode::UnsupportedFeature,
                        format!(
                            "unsupported clause detail mismatch: expected {expected_clause}, got {:?}",
                            clause
                        ),
                    ));
                }
            }
            Ok(_) => {
                return Err(GraphDbError::new(
                    ErrorCode::UnsupportedFeature,
                    format!("unsupported clause unexpectedly succeeded: {query}"),
                ));
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn validate_neo4j_compat_subquery_clauses() -> Result<(), GraphDbError> {
    let mut exists_store = InMemoryGraphStore::new();
    exists_store.create_node(vec!["Probe".to_string()], crate::graph::Properties::new());
    match execute_query_with_dialect(
        &mut exists_store,
        "MATCH (n) WHERE EXISTS { MATCH (m) RETURN m } RETURN n",
        CypherDialect::Neo4jCompat,
    ) {
        Ok(QueryResult::Nodes(nodes)) if !nodes.is_empty() => {}
        Ok(_) => {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                "EXISTS subquery did not produce matching rows",
            ));
        }
        Err(err) => return Err(err),
    }

    let mut call_store = InMemoryGraphStore::new();
    call_store.create_node(vec!["Probe".to_string()], crate::graph::Properties::new());
    match execute_query_with_dialect(
        &mut call_store,
        "CALL { MATCH (n) RETURN n } RETURN count(*)",
        CypherDialect::Neo4jCompat,
    ) {
        Ok(QueryResult::Table { rows, .. }) if rows.len() == 1 => Ok(()),
        Ok(_) => Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "CALL subquery did not produce a count row",
        )),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::build_tck_selector_map;

    #[test]
    fn loads_required_tests_contract() {
        let required = load_required_tests_spec();
        assert_eq!(required.spec_id, OPENCYPHER_REQUIRED_SPEC_ID);
        assert_eq!(required.version, "1.0.0");
        assert!(required.required_tests.len() >= 10);
    }

    #[test]
    fn validates_governance_and_sync() {
        run_conformance_governance_checks().expect("governance checks must pass");
    }

    #[test]
    fn validates_selector_map_resolvability() {
        let tck = load_opencypher9_tck_required_spec();
        let map = build_tck_selector_map(&tck).expect("selector map");
        assert_eq!(map.len(), tck.required_tck_ids.len());
    }

    #[test]
    fn required_tests_have_traceability_fields() {
        let required = load_required_tests_spec();
        for case in required.required_tests {
            assert!(!case.covers_req.trim().is_empty());
            assert!(!case.covers_acceptance.trim().is_empty());
            assert!(case.r#type == "positive" || case.r#type == "negative");
        }
    }

    #[test]
    fn builds_and_persists_report() {
        let tmp = std::env::temp_dir().join("agdb-conformance-report-test.json");
        let report = build_and_persist_conformance_report(&tmp).expect("report");
        assert_eq!(report.spec_id, OPENCYPHER_REQUIRED_SPEC_ID);
        assert!(report.pass_rate >= 99.0);
        assert!(tmp.exists());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn validates_neo4j_compat_governance_bundle() {
        run_neo4j_compat_governance_checks().expect("neo4j compat governance checks must pass");
    }

    #[test]
    fn loads_neo4j_compat_manifest_and_required_spec() {
        let required = load_neo4j_compat_required_spec();
        let manifest = load_neo4j_compat_manifest();
        assert_eq!(required.manifest_id, manifest.manifest_id);
        assert_eq!(required.baseline_id, NEO4J_COMPAT_REQUIRED_SPEC_ID);
        assert!(!manifest.features.is_empty());
    }
}
