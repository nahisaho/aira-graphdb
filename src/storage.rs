use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::{ErrorCode, GraphDbError};
use crate::graph::InMemoryGraphStore;

pub const CURRENT_CATALOG_VERSION: u64 = 1;

#[derive(Debug, Clone)]
pub struct StorageEngine {
    db_file: PathBuf,
    wal_file: PathBuf,
    audit_file: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct ClusterMetadataCatalog {
    pub catalog_schema_version: String,
    pub partitions: Vec<PartitionMetadata>,
    pub replicas: Vec<ReplicaMetadata>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PartitionMetadata {
    pub id: String,
    pub range: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ReplicaMetadata {
    pub partition_id: String,
    pub replicas: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PersistedSnapshot {
    catalog_version: u64,
    #[serde(default)]
    cluster_metadata: ClusterMetadataCatalog,
    graph: InMemoryGraphStore,
}

#[derive(Debug, Serialize, Deserialize)]
struct WalCommitRecord {
    tx_id: String,
    snapshot: PersistedSnapshot,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub event_type: String,
    pub code: Option<String>,
    pub message: String,
    pub tx_id: Option<String>,
    pub payload: HashMap<String, String>,
    pub timestamp_epoch_ms: u128,
}

impl StorageEngine {
    pub fn new(db_file: impl Into<PathBuf>) -> Self {
        let db_file = db_file.into();
        let wal_file = db_file.with_extension("wal");
        let audit_file = db_file.with_extension("audit.log");
        Self {
            db_file,
            wal_file,
            audit_file,
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_file
    }

    pub fn load_or_init(&self) -> Result<InMemoryGraphStore, GraphDbError> {
        if !self.db_file.exists() {
            self.persist_snapshot(&InMemoryGraphStore::new())?;
        }
        self.load_graph()
    }

    pub fn load_graph(&self) -> Result<InMemoryGraphStore, GraphDbError> {
        let raw = fs::read_to_string(&self.db_file).map_err(|e| {
            GraphDbError::new(ErrorCode::IncompatibleFormat, format!("cannot read db file: {e}"))
        })?;
        let snapshot: PersistedSnapshot = serde_json::from_str(&raw).map_err(|e| {
            GraphDbError::new(
                ErrorCode::IncompatibleFormat,
                format!("cannot parse db snapshot: {e}"),
            )
        })?;
        if snapshot.catalog_version != CURRENT_CATALOG_VERSION {
            return Err(GraphDbError::new(
                ErrorCode::IncompatibleFormat,
                format!(
                    "catalog version mismatch: expected {}, got {}",
                    CURRENT_CATALOG_VERSION, snapshot.catalog_version
                ),
            ));
        }
        Ok(snapshot.graph)
    }

    pub fn load_cluster_metadata(&self) -> Result<ClusterMetadataCatalog, GraphDbError> {
        let raw = fs::read_to_string(&self.db_file).map_err(|e| {
            GraphDbError::new(ErrorCode::IncompatibleFormat, format!("cannot read db file: {e}"))
        })?;
        let snapshot: PersistedSnapshot = serde_json::from_str(&raw).map_err(|e| {
            GraphDbError::new(
                ErrorCode::IncompatibleFormat,
                format!("cannot parse db snapshot: {e}"),
            )
        })?;
        Ok(snapshot.cluster_metadata)
    }

    pub fn persist_cluster_metadata(&self, metadata: ClusterMetadataCatalog) -> Result<(), GraphDbError> {
        let mut graph = InMemoryGraphStore::new();
        if self.db_file.exists() {
            graph = self.load_graph()?;
        }
        let snapshot = PersistedSnapshot {
            catalog_version: CURRENT_CATALOG_VERSION,
            cluster_metadata: metadata,
            graph,
        };
        self.persist_snapshot_from_struct(&snapshot)
    }

    pub fn persist_after_wal(&self, tx_id: &str, graph: &InMemoryGraphStore) -> Result<(), GraphDbError> {
        let snapshot = PersistedSnapshot {
            catalog_version: CURRENT_CATALOG_VERSION,
            cluster_metadata: ClusterMetadataCatalog {
                catalog_schema_version: "v1".to_string(),
                partitions: Vec::new(),
                replicas: Vec::new(),
            },
            graph: graph.clone(),
        };
        self.append_wal(tx_id, &snapshot)?;
        self.persist_snapshot_from_struct(&snapshot)?;
        Ok(())
    }

    pub fn recover(&self) -> Result<InMemoryGraphStore, GraphDbError> {
        if !self.wal_file.exists() {
            return self.load_or_init();
        }

        let raw = fs::read_to_string(&self.wal_file).map_err(|e| {
            GraphDbError::new(ErrorCode::IncompatibleFormat, format!("cannot read wal file: {e}"))
        })?;
        let mut latest: Option<PersistedSnapshot> = None;
        for line in raw.lines().filter(|line| !line.trim().is_empty()) {
            let record: WalCommitRecord = serde_json::from_str(line).map_err(|e| {
                GraphDbError::new(ErrorCode::IncompatibleFormat, format!("invalid wal entry: {e}"))
            })?;
            latest = Some(record.snapshot);
        }

        if let Some(snapshot) = latest {
            self.persist_snapshot_from_struct(&snapshot)?;
            return Ok(snapshot.graph);
        }
        self.load_or_init()
    }

    pub fn append_audit_event(&self, event: AuditEvent) -> Result<(), GraphDbError> {
        let mut audit = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_file)
            .map_err(|e| GraphDbError::new(ErrorCode::IncompatibleFormat, format!("open audit failed: {e}")))?;
        let payload = serde_json::to_string(&event)
            .map_err(|e| GraphDbError::new(ErrorCode::IncompatibleFormat, format!("serialize audit failed: {e}")))?;
        audit
            .write_all(payload.as_bytes())
            .and_then(|_| audit.write_all(b"\n"))
            .and_then(|_| audit.flush())
            .map_err(|e| GraphDbError::new(ErrorCode::IncompatibleFormat, format!("append audit failed: {e}")))
    }

    pub fn load_audit_events(&self) -> Result<Vec<AuditEvent>, GraphDbError> {
        if !self.audit_file.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&self.audit_file).map_err(|e| {
            GraphDbError::new(ErrorCode::IncompatibleFormat, format!("cannot read audit file: {e}"))
        })?;
        let mut events = Vec::new();
        for line in raw.lines().filter(|line| !line.trim().is_empty()) {
            let event: AuditEvent = serde_json::from_str(line).map_err(|e| {
                GraphDbError::new(ErrorCode::IncompatibleFormat, format!("invalid audit entry: {e}"))
            })?;
            events.push(event);
        }
        Ok(events)
    }

    fn persist_snapshot(&self, graph: &InMemoryGraphStore) -> Result<(), GraphDbError> {
        let snapshot = PersistedSnapshot {
            catalog_version: CURRENT_CATALOG_VERSION,
            cluster_metadata: ClusterMetadataCatalog {
                catalog_schema_version: "v1".to_string(),
                partitions: Vec::new(),
                replicas: Vec::new(),
            },
            graph: graph.clone(),
        };
        self.persist_snapshot_from_struct(&snapshot)
    }

    fn persist_snapshot_from_struct(&self, snapshot: &PersistedSnapshot) -> Result<(), GraphDbError> {
        let serialized = serde_json::to_string_pretty(snapshot).map_err(|e| {
            GraphDbError::new(ErrorCode::IncompatibleFormat, format!("serialize snapshot failed: {e}"))
        })?;
        fs::write(&self.db_file, serialized).map_err(|e| {
            GraphDbError::new(ErrorCode::IncompatibleFormat, format!("write snapshot failed: {e}"))
        })
    }

    fn append_wal(&self, tx_id: &str, snapshot: &PersistedSnapshot) -> Result<(), GraphDbError> {
        let mut wal = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.wal_file)
            .map_err(|e| GraphDbError::new(ErrorCode::IncompatibleFormat, format!("open wal failed: {e}")))?;

        let record = WalCommitRecord {
            tx_id: tx_id.to_string(),
            snapshot: snapshot.clone(),
        };
        let payload = serde_json::to_string(&record)
            .map_err(|e| GraphDbError::new(ErrorCode::IncompatibleFormat, format!("serialize wal failed: {e}")))?;
        wal.write_all(payload.as_bytes())
            .and_then(|_| wal.write_all(b"\n"))
            .and_then(|_| wal.flush())
            .map_err(|e| GraphDbError::new(ErrorCode::IncompatibleFormat, format!("append wal failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::graph::{Properties, Value};

    fn temp_file(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time ok")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}.json"))
    }

    #[test]
    fn persists_and_loads_graph() {
        let path = temp_file("agdb-storage");
        let engine = StorageEngine::new(&path);
        let mut graph = InMemoryGraphStore::new();
        let mut props = Properties::new();
        props.insert("k".to_string(), Value::String("v".to_string()));
        graph.create_node(vec!["L".to_string()], props);
        engine
            .persist_after_wal("tx-1", &graph)
            .expect("persist must succeed");

        let loaded = engine.load_graph().expect("load succeeds");
        assert_eq!(loaded.node_count(), 1);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("wal"));
    }

    #[test]
    fn rejects_incompatible_catalog() {
        let path = temp_file("agdb-storage-incompat");
        let snapshot = serde_json::json!({
            "catalog_version": 999,
            "graph": InMemoryGraphStore::new()
        });
        fs::write(&path, serde_json::to_string_pretty(&snapshot).expect("json"))
            .expect("write");
        let engine = StorageEngine::new(&path);
        let err = engine.load_graph().expect_err("must fail");
        assert_eq!(err.code, ErrorCode::IncompatibleFormat);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn persists_and_loads_cluster_metadata() {
        let path = temp_file("agdb-cluster-meta");
        let engine = StorageEngine::new(&path);
        engine
            .persist_cluster_metadata(ClusterMetadataCatalog {
                catalog_schema_version: "v1".to_string(),
                partitions: vec![PartitionMetadata {
                    id: "p1".to_string(),
                    range: "0-99".to_string(),
                }],
                replicas: vec![ReplicaMetadata {
                    partition_id: "p1".to_string(),
                    replicas: vec!["r1".to_string(), "r2".to_string()],
                }],
            })
            .expect("persist metadata");

        let loaded = engine.load_cluster_metadata().expect("load metadata");
        assert_eq!(loaded.catalog_schema_version, "v1");
        assert_eq!(loaded.partitions.len(), 1);
        assert_eq!(loaded.replicas.len(), 1);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn appends_and_loads_audit_events() {
        let path = temp_file("agdb-audit");
        let engine = StorageEngine::new(&path);
        let event = AuditEvent {
            event_type: "AUTH_REQUIRED_REJECTED".to_string(),
            code: Some("AUTH_REQUIRED".to_string()),
            message: "APP_READY required before begin_tx".to_string(),
            tx_id: None,
            payload: HashMap::from([("request".to_string(), "begin_tx".to_string())]),
            timestamp_epoch_ms: 1,
        };
        engine.append_audit_event(event.clone()).expect("append");
        let loaded = engine.load_audit_events().expect("load");
        assert_eq!(loaded, vec![event]);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("wal"));
        let _ = fs::remove_file(path.with_extension("audit.log"));
    }
}
