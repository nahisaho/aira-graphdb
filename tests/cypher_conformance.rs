use aira_graphdb::conformance::{
    DEFAULT_CONFORMANCE_REPORT_PATH, build_and_persist_conformance_report,
    build_and_persist_neo4j_compat_conformance_report, load_neo4j_compat_required_spec,
    load_required_tests_spec, run_conformance_governance_checks,
    run_neo4j_compat_governance_checks,
};
use aira_graphdb::contracts::{load_opencypher9_tck_required_spec, resolve_tck_selector};

#[test]
fn required_tck_ids_are_all_resolvable() {
    let tck = load_opencypher9_tck_required_spec();
    for id in &tck.required_tck_ids {
        let selector =
            resolve_tck_selector(&tck, &id.id).expect("all required_tck_ids must resolve");
        assert_eq!(selector, id.id);
    }
}

#[test]
fn governance_profile_is_valid() {
    run_conformance_governance_checks().expect("governance checks must pass");
}

#[test]
fn neo4j_compat_governance_profile_is_valid() {
    run_neo4j_compat_governance_checks().expect("neo4j compat governance checks must pass");
}

#[test]
fn required_suite_contains_traceable_cases() {
    let required = load_required_tests_spec();
    let ids: Vec<&str> = required
        .required_tests
        .iter()
        .map(|v| v.id.as_str())
        .collect();
    assert!(ids.contains(&"OC9-TCK-FULL-RUN-001"));
    assert!(ids.contains(&"OC9-TCK-MANIFEST-SYNC-001"));
    assert!(ids.contains(&"OC9-SYNTAX-ERROR-CODE-001"));
}

#[test]
fn persists_compatibility_report_artifact() {
    let report = build_and_persist_conformance_report(DEFAULT_CONFORMANCE_REPORT_PATH)
        .expect("report artifact");
    assert!(report.failed_test_ids.is_empty());
    assert!(report.unresolved_tck_ids.is_empty());
    assert!(report.mandatory_negative_cases_satisfied);
    assert!(report.pass_rate >= 99.0);
    assert!(std::path::Path::new(DEFAULT_CONFORMANCE_REPORT_PATH).exists());
}

#[test]
fn neo4j_compat_required_spec_is_traceable() {
    let spec = load_neo4j_compat_required_spec();
    assert_eq!(spec.baseline_id, "NEO4J-COMPAT-BASELINE-001");
    assert!(
        spec.required_test_ids
            .iter()
            .any(|id| id == "NEO4J-COMPAT-RUN-001")
    );
    assert!(
        spec.governance_checks
            .iter()
            .any(|check| check == "unsupported_clauses_return_unsupported_feature")
    );
    assert!(
        spec.governance_checks
            .iter()
            .any(|check| check == "subquery_clauses_execute")
    );
}

#[test]
fn neo4j_compat_manifest_includes_subquery_support() {
    let manifest = aira_graphdb::contracts::load_neo4j_compat_manifest();
    assert!(
        manifest
            .features
            .iter()
            .any(|feature| feature.feature_id == "EXISTS" && feature.status == "required")
    );
    assert!(
        manifest
            .features
            .iter()
            .any(|feature| feature.feature_id == "CALL_SUBQUERY" && feature.status == "required")
    );
}

#[test]
fn persists_neo4j_compat_report_artifact() {
    let path = std::env::temp_dir().join("neo4j-compat-report.json");
    let report =
        build_and_persist_neo4j_compat_conformance_report(&path).expect("neo4j compat report");
    assert!(report.failed_check_ids.is_empty());
    assert!(!report.release_blocked);
    assert!(report.pass_rate >= 99.0);
    assert!(path.exists());
    let _ = std::fs::remove_file(path);
}
