use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::{ErrorCode, GraphDbError};
use crate::graph::InMemoryGraphStore;
use crate::storage::{AuditEvent, StorageEngine};

#[derive(Debug)]
struct TxSession {
    graph: InMemoryGraphStore,
    base_version: u64,
}

#[derive(Debug)]
pub struct TransactionManager {
    storage: StorageEngine,
    base_graph: InMemoryGraphStore,
    sessions: HashMap<String, TxSession>,
    tx_seq: u64,
    graph_version: u64,
}

impl TransactionManager {
    pub fn new(storage: StorageEngine) -> Result<Self, GraphDbError> {
        let base_graph = storage.load_or_init()?;
        Ok(Self {
            storage,
            base_graph,
            sessions: HashMap::new(),
            tx_seq: 0,
            graph_version: 0,
        })
    }

    pub fn begin(&mut self) -> String {
        self.tx_seq += 1;
        let tx_id = format!("tx-{}", self.tx_seq);
        self.sessions.insert(
            tx_id.clone(),
            TxSession {
                graph: self.base_graph.clone(),
                base_version: self.graph_version,
            },
        );
        tx_id
    }

    pub fn graph_mut(&mut self, tx_id: &str) -> Result<&mut InMemoryGraphStore, GraphDbError> {
        self.sessions.get_mut(tx_id).map(|s| &mut s.graph).ok_or_else(|| {
            GraphDbError::new(
                ErrorCode::RetryableConflict,
                format!("unknown transaction: {tx_id}"),
            )
        })
    }

    pub fn commit(&mut self, tx_id: &str) -> Result<(), GraphDbError> {
        let session = self.sessions.remove(tx_id).ok_or_else(|| {
            let err = GraphDbError::new(
                ErrorCode::RetryableConflict,
                format!("unknown transaction: {tx_id}"),
            );
            let _ = self.record_audit_event(
                "DETERMINISTIC_CONFLICT",
                Some(err.code.as_str()),
                &err.message,
                Some(tx_id),
                HashMap::new(),
            );
            err
        })?;
        if session.base_version != self.graph_version {
            let err = GraphDbError::new(
                ErrorCode::RetryableConflict,
                format!("deterministic conflict for tx_id={tx_id}"),
            );
            let _ = self.record_audit_event(
                "DETERMINISTIC_CONFLICT",
                Some(err.code.as_str()),
                &err.message,
                Some(tx_id),
                HashMap::new(),
            );
            return Err(err);
        }
        let snapshot = session.graph;
        self.storage.persist_after_wal(tx_id, &snapshot)?;
        self.base_graph = snapshot;
        self.graph_version += 1;
        Ok(())
    }

    pub fn rollback(&mut self, tx_id: &str) -> Result<(), GraphDbError> {
        self.sessions.remove(tx_id).ok_or_else(|| {
            GraphDbError::new(
                ErrorCode::RetryableConflict,
                format!("unknown transaction: {tx_id}"),
            )
        })?;
        let _ = self.record_audit_event(
            "ROLLBACK_EXECUTED",
            None,
            "transaction rollback executed",
            Some(tx_id),
            HashMap::new(),
        );
        Ok(())
    }

    pub fn current_graph(&self) -> &InMemoryGraphStore {
        &self.base_graph
    }

    pub fn recover(&mut self) -> Result<(), GraphDbError> {
        self.base_graph = self.storage.recover()?;
        self.sessions.clear();
        self.graph_version += 1;
        Ok(())
    }

    pub fn record_audit_event(
        &self,
        event_type: &str,
        code: Option<&str>,
        message: &str,
        tx_id: Option<&str>,
        payload: HashMap<String, String>,
    ) -> Result<(), GraphDbError> {
        self.storage.append_audit_event(AuditEvent {
            event_type: event_type.to_string(),
            code: code.map(str::to_string),
            message: message.to_string(),
            tx_id: tx_id.map(str::to_string),
            payload,
            timestamp_epoch_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after epoch")
                .as_millis(),
        })
    }

    pub fn audit_events(&self) -> Result<Vec<AuditEvent>, GraphDbError> {
        self.storage.load_audit_events()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::graph::{Properties, Value};

    fn temp_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("agdb-tx-{nanos}.json"))
    }

    #[test]
    fn rollback_keeps_base_graph_clean() {
        let path = temp_path();
        let storage = StorageEngine::new(&path);
        let mut txm = TransactionManager::new(storage).expect("init");
        let tx_id = txm.begin();
        let mut props = Properties::new();
        props.insert("title".to_string(), Value::String("draft".to_string()));
        txm.graph_mut(&tx_id)
            .expect("tx graph")
            .create_node(vec!["Doc".to_string()], props);
        txm.rollback(&tx_id).expect("rollback");
        assert_eq!(txm.current_graph().node_count(), 0);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("wal"));
    }

    #[test]
    fn commit_persists_graph() {
        let path = temp_path();
        let storage = StorageEngine::new(&path);
        let mut txm = TransactionManager::new(storage).expect("init");
        let tx_id = txm.begin();
        txm.graph_mut(&tx_id)
            .expect("tx graph")
            .create_node(vec!["Doc".to_string()], Properties::new());
        txm.commit(&tx_id).expect("commit");
        assert_eq!(txm.current_graph().node_count(), 1);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("wal"));
    }

    #[test]
    fn conflicting_parallel_transactions_return_retryable_conflict() {
        let path = temp_path();
        let storage = StorageEngine::new(&path);
        let mut txm = TransactionManager::new(storage).expect("init");

        let tx1 = txm.begin();
        let tx2 = txm.begin();
        txm.graph_mut(&tx1)
            .expect("tx1 graph")
            .create_node(vec!["Doc".to_string()], Properties::new());
        txm.graph_mut(&tx2)
            .expect("tx2 graph")
            .create_node(vec!["Doc".to_string()], Properties::new());

        txm.commit(&tx1).expect("first commit");
        let err = txm.commit(&tx2).expect_err("second commit must conflict");
        assert_eq!(err.code, ErrorCode::RetryableConflict);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("wal"));
        let _ = fs::remove_file(path.with_extension("audit.log"));
    }

    #[test]
    fn rollback_emits_audit_event() {
        let path = temp_path();
        let storage = StorageEngine::new(&path);
        let mut txm = TransactionManager::new(storage).expect("init");
        let tx_id = txm.begin();
        txm.rollback(&tx_id).expect("rollback");
        let events = txm.audit_events().expect("audit");
        assert!(events.iter().any(|e| e.event_type == "ROLLBACK_EXECUTED"));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("wal"));
        let _ = fs::remove_file(path.with_extension("audit.log"));
    }
}
