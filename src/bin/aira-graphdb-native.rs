use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use std::panic;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GraphNode {
    node_id: String,
    corpus_id: String,
    layer: String,
    r#ref: Value,
    label: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GraphEdge {
    edge_id: String,
    corpus_id: String,
    source_node_id: String,
    target_node_id: String,
    relation: String,
    weight: f64,
    bridge_kind: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VectorRecord {
    id: String,
    corpus_id: String,
    namespace: String,
    values: Vec<f64>,
    metadata: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Passage {
    passage_id: String,
    corpus_id: String,
    document_id: String,
    text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct State {
    nodes: HashMap<String, GraphNode>,
    edges: HashMap<String, GraphEdge>,
    vectors: HashMap<String, VectorRecord>,
    passages: HashMap<String, Passage>,
    snapshots: HashMap<String, Value>,
    checkpoints: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RpcRequest {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct RpcResponse {
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcError {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_class: Option<String>,
}

#[derive(Debug)]
struct AppError {
    code: String,
    message: String,
    failure_class: Option<String>,
}

#[derive(Clone)]
struct CrashTracker {
    audit_log_path: PathBuf,
    started_epoch_sec: u64,
    last_request_id: Arc<Mutex<Option<String>>>,
}

struct Server {
    db_path: PathBuf,
    audit_log_path: PathBuf,
    state: State,
    cache_dirty: bool,
    node_keys_by_corpus: HashMap<String, Vec<String>>,
    edge_keys_by_corpus: HashMap<String, Vec<String>>,
    adjacent_edge_keys_by_node: HashMap<String, Vec<String>>,
    vector_keys_by_corpus_namespace: HashMap<String, Vec<String>>,
    passage_keys_by_corpus: HashMap<String, Vec<String>>,
}

impl CrashTracker {
    fn new(audit_log_path: PathBuf) -> Self {
        let started_epoch_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_secs();
        Self {
            audit_log_path,
            started_epoch_sec,
            last_request_id: Arc::new(Mutex::new(None)),
        }
    }

    fn set_last_request_id(&self, request_id: String) {
        if let Ok(mut guard) = self.last_request_id.lock() {
            *guard = Some(request_id);
        }
    }

    fn last_request_id(&self) -> Option<String> {
        self.last_request_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    fn uptime_sec(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_secs();
        now.saturating_sub(self.started_epoch_sec)
    }

    fn append_crash_event(
        &self,
        process_exit_code: Option<i32>,
        signal: Option<&str>,
        cause: Option<String>,
    ) {
        let payload = json!({
            "errorCode": "PROCESS_CRASH",
            "timestamp": Server::now_epoch_ms_string(),
            "processExitCode": process_exit_code,
            "signal": signal,
            "lastRequestId": self.last_request_id(),
            "uptimeSec": self.uptime_sec(),
            "cause": cause
        });
        let _ = Server::append_json_line_for_path(&self.audit_log_path, &payload);
    }
}

impl Server {
    fn open(db_path: PathBuf) -> io::Result<Self> {
        let state = if db_path.exists() {
            let raw = fs::read_to_string(&db_path)?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            State::default()
        };
        Ok(Self {
            audit_log_path: db_path.with_extension("native-audit.log"),
            db_path,
            state,
            cache_dirty: true,
            node_keys_by_corpus: HashMap::new(),
            edge_keys_by_corpus: HashMap::new(),
            adjacent_edge_keys_by_node: HashMap::new(),
            vector_keys_by_corpus_namespace: HashMap::new(),
            passage_keys_by_corpus: HashMap::new(),
        })
    }

    fn now_epoch_ms_string() -> String {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_millis()
            .to_string()
    }

    fn append_request_audit_event_for_path(
        audit_log_path: &PathBuf,
        error_code: &str,
        failure_class: &str,
        request_id: &str,
    ) -> io::Result<()> {
        let payload = json!({
            "errorCode": error_code,
            "failureClass": failure_class,
            "requestId": request_id,
            "timestamp": Self::now_epoch_ms_string()
        });
        Self::append_json_line_for_path(audit_log_path, &payload)
    }

    fn append_json_line_for_path(audit_log_path: &PathBuf, payload: &Value) -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(audit_log_path)?;
        file.write_all(payload.to_string().as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()
    }

    fn append_request_audit_event(
        &self,
        error_code: &str,
        failure_class: &str,
        request_id: &str,
    ) {
        let _ = Self::append_request_audit_event_for_path(
            &self.audit_log_path,
            error_code,
            failure_class,
            request_id,
        );
    }

    fn persist(&self) -> io::Result<()> {
        let raw = serde_json::to_vec(&self.state)
            .map_err(|err| io::Error::other(format!("serialize failed: {err}")))?;
        let parent = self
            .db_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&parent)?;

        let file_name = self
            .db_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("aira-graphdb-native.json");
        let tmp_path = parent.join(format!(".{file_name}.tmp"));

        {
            let mut tmp_file = fs::File::create(&tmp_path)?;
            tmp_file.write_all(&raw)?;
            tmp_file.sync_all()?;
        }

        fs::rename(&tmp_path, &self.db_path)?;

        let dir_file = fs::File::open(&parent)?;
        dir_file.sync_all()?;
        Ok(())
    }

    fn key(corpus_id: &str, id: &str) -> String {
        format!("{corpus_id}:{id}")
    }

    fn node_key(corpus_id: &str, node_id: &str) -> String {
        format!("{corpus_id}:{node_id}")
    }

    fn corpus_namespace_key(corpus_id: &str, namespace: &str) -> String {
        format!("{corpus_id}:{namespace}")
    }

    fn mark_cache_dirty(&mut self) {
        self.cache_dirty = true;
    }

    fn ensure_cache(&mut self) {
        if !self.cache_dirty {
            return;
        }

        self.node_keys_by_corpus.clear();
        for (key, node) in &self.state.nodes {
            self.node_keys_by_corpus
                .entry(node.corpus_id.clone())
                .or_default()
                .push(key.clone());
        }

        self.edge_keys_by_corpus.clear();
        self.adjacent_edge_keys_by_node.clear();
        for (key, edge) in &self.state.edges {
            self.edge_keys_by_corpus
                .entry(edge.corpus_id.clone())
                .or_default()
                .push(key.clone());

            let source_key = Self::node_key(&edge.corpus_id, &edge.source_node_id);
            self.adjacent_edge_keys_by_node
                .entry(source_key)
                .or_default()
                .push(key.clone());

            if edge.source_node_id != edge.target_node_id {
                let target_key = Self::node_key(&edge.corpus_id, &edge.target_node_id);
                self.adjacent_edge_keys_by_node
                    .entry(target_key)
                    .or_default()
                    .push(key.clone());
            }
        }

        self.vector_keys_by_corpus_namespace.clear();
        for (key, vector) in &self.state.vectors {
            let corpus_namespace_key =
                Self::corpus_namespace_key(&vector.corpus_id, &vector.namespace);
            self.vector_keys_by_corpus_namespace
                .entry(corpus_namespace_key)
                .or_default()
                .push(key.clone());
        }

        self.passage_keys_by_corpus.clear();
        for (key, passage) in &self.state.passages {
            self.passage_keys_by_corpus
                .entry(passage.corpus_id.clone())
                .or_default()
                .push(key.clone());
        }

        self.cache_dirty = false;
    }

    fn doc_ids_from_ref(value: &Value) -> Vec<String> {
        if let Some(ids) = value.get("sourceDocumentIds").and_then(|v| v.as_array()) {
            return ids
                .iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect();
        }
        if let Some(document_id) = value
            .get("metadata")
            .and_then(|v| v.get("documentId"))
            .and_then(|v| v.as_str())
        {
            return vec![document_id.to_string()];
        }
        Vec::new()
    }

    fn cosine(a: &[f64], b: &[f64]) -> f64 {
        if a.is_empty() || b.is_empty() || a.len() != b.len() {
            return 0.0;
        }
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        for i in 0..a.len() {
            dot += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
        }
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }

    fn token_score(text: &str, tokens: &[String]) -> f64 {
        let lower = text.to_lowercase();
        tokens
            .iter()
            .map(|token| lower.matches(token).count() as f64)
            .sum()
    }

    fn execution_client_error(message: String) -> AppError {
        AppError {
            code: "REQUEST_EXECUTION_FAILED".to_string(),
            message,
            failure_class: Some("CLIENT_INPUT".to_string()),
        }
    }

    fn execution_io_error(message: String) -> AppError {
        AppError {
            code: "REQUEST_EXECUTION_FAILED".to_string(),
            message,
            failure_class: Some("IO_FAILURE".to_string()),
        }
    }

    fn unsupported_method_error(method: &str) -> AppError {
        AppError {
            code: "UNSUPPORTED_FEATURE".to_string(),
            message: format!("unsupported_method:{method}"),
            failure_class: Some("CLIENT_INPUT".to_string()),
        }
    }

    fn handle(&mut self, req: RpcRequest) -> RpcResponse {
        let result: Result<Value, AppError> = (|| {
            match req.method.as_str() {
            "ping" => Ok(json!({"pong": true})),
            "upsert_nodes" => {
                let nodes = req.params.get("nodes").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                for node in nodes {
                    let parsed = serde_json::from_value::<GraphNode>(node)
                        .map_err(|err| Self::execution_client_error(format!("invalid node: {err}")))?;
                    self.state
                        .nodes
                        .insert(Self::key(&parsed.corpus_id, &parsed.node_id), parsed);
                }
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(null))
            }
            "upsert_edges" => {
                let edges = req.params.get("edges").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                for edge in edges {
                    let parsed = serde_json::from_value::<GraphEdge>(edge)
                        .map_err(|err| Self::execution_client_error(format!("invalid edge: {err}")))?;
                    self.state
                        .edges
                        .insert(Self::key(&parsed.corpus_id, &parsed.edge_id), parsed);
                }
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(null))
            }
            "get_node" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let node_id = req.params.get("nodeId").and_then(Value::as_str).unwrap_or_default();
                let node = self.state.nodes.get(&Self::key(corpus_id, node_id)).cloned();
                Ok(serde_json::to_value(node).unwrap_or(Value::Null))
            }
            "get_nodes" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let layer = req.params.get("layer").and_then(Value::as_str);
                self.ensure_cache();
                let mut out: Vec<GraphNode> = self
                    .node_keys_by_corpus
                    .get(corpus_id)
                    .into_iter()
                    .flat_map(|keys| keys.iter())
                    .filter_map(|key| self.state.nodes.get(key))
                    .filter(|n| layer.is_none_or(|l| n.layer == l))
                    .cloned()
                    .collect();
                out.sort_by(|a, b| a.node_id.cmp(&b.node_id));
                Ok(serde_json::to_value(out).unwrap_or(Value::Null))
            }
            "get_edges" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let source_node_id = req.params.get("sourceNodeId").and_then(Value::as_str);
                self.ensure_cache();
                let mut out: Vec<GraphEdge> = self
                    .edge_keys_by_corpus
                    .get(corpus_id)
                    .into_iter()
                    .flat_map(|keys| keys.iter())
                    .filter_map(|key| self.state.edges.get(key))
                    .filter(|e| source_node_id.is_none_or(|s| e.source_node_id == s))
                    .cloned()
                    .collect();
                out.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));
                Ok(serde_json::to_value(out).unwrap_or(Value::Null))
            }
            "get_adjacent" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let node_id = req.params.get("nodeId").and_then(Value::as_str).unwrap_or_default();
                self.ensure_cache();
                let node_key = Self::node_key(corpus_id, node_id);
                let mut out: Vec<GraphEdge> = self
                    .adjacent_edge_keys_by_node
                    .get(&node_key)
                    .into_iter()
                    .flat_map(|keys| keys.iter())
                    .filter_map(|key| self.state.edges.get(key))
                    .cloned()
                    .collect();
                out.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));
                Ok(serde_json::to_value(out).unwrap_or(Value::Null))
            }
            "delete_nodes" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let node_ids = req.params.get("nodeIds").and_then(Value::as_array).cloned().unwrap_or_default();
                let mut deleted = 0;
                for node_id in node_ids.iter().filter_map(Value::as_str) {
                    if self.state.nodes.remove(&Self::key(corpus_id, node_id)).is_some() {
                        deleted += 1;
                    }
                    self.state.edges.retain(|_, edge| {
                        !(edge.corpus_id == corpus_id
                            && (edge.source_node_id == node_id || edge.target_node_id == node_id))
                    });
                }
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(deleted))
            }
            "delete_edges" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let edge_ids = req.params.get("edgeIds").and_then(Value::as_array).cloned().unwrap_or_default();
                let mut deleted = 0;
                for edge_id in edge_ids.iter().filter_map(Value::as_str) {
                    if self.state.edges.remove(&Self::key(corpus_id, edge_id)).is_some() {
                        deleted += 1;
                    }
                }
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(deleted))
            }
            "delete_by_document" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let document_id = req.params.get("documentId").and_then(Value::as_str).unwrap_or_default();
                let before_nodes = self.state.nodes.len();
                let before_edges = self.state.edges.len();
                let removable: Vec<String> = self
                    .state
                    .nodes
                    .iter()
                    .filter_map(|(k, node)| {
                        if node.corpus_id != corpus_id {
                            return None;
                        }
                        let docs = Self::doc_ids_from_ref(&node.r#ref);
                        docs.iter().any(|id| id == document_id).then_some(k.clone())
                    })
                    .collect();
                let mut removed_node_ids = Vec::new();
                for key in removable {
                    if let Some(node) = self.state.nodes.remove(&key) {
                        removed_node_ids.push(node.node_id);
                    }
                }
                self.state.edges.retain(|_, edge| {
                    !(edge.corpus_id == corpus_id
                        && (removed_node_ids.iter().any(|id| id == &edge.source_node_id)
                            || removed_node_ids.iter().any(|id| id == &edge.target_node_id)))
                });
                self.state.vectors.retain(|_, v| {
                    if v.corpus_id != corpus_id {
                        return true;
                    }
                    let doc = v.metadata.get("documentId").and_then(Value::as_str).unwrap_or_default();
                    doc != document_id
                });
                self.state.passages.retain(|_, p| !(p.corpus_id == corpus_id && p.document_id == document_id));
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!({
                    "deletedNodes": before_nodes.saturating_sub(self.state.nodes.len()),
                    "deletedEdges": before_edges.saturating_sub(self.state.edges.len())
                }))
            }
            "delete_by_corpus" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let before_nodes = self.state.nodes.len();
                let before_edges = self.state.edges.len();
                self.state.nodes.retain(|_, n| n.corpus_id != corpus_id);
                self.state.edges.retain(|_, e| e.corpus_id != corpus_id);
                self.state.vectors.retain(|_, v| v.corpus_id != corpus_id);
                self.state.passages.retain(|_, p| p.corpus_id != corpus_id);
                self.state.snapshots.remove(corpus_id);
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!({
                    "deletedNodes": before_nodes.saturating_sub(self.state.nodes.len()),
                    "deletedEdges": before_edges.saturating_sub(self.state.edges.len())
                }))
            }
            "vector_upsert" => {
                let records = req.params.get("records").and_then(Value::as_array).cloned().unwrap_or_default();
                for record in records {
                    let parsed = serde_json::from_value::<VectorRecord>(record)
                        .map_err(|err| Self::execution_client_error(format!("invalid vector record: {err}")))?;
                    self.state
                        .vectors
                        .insert(Self::key(&parsed.corpus_id, &parsed.id), parsed);
                }
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(null))
            }
            "vector_search" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let namespace = req.params.get("namespace").and_then(Value::as_str).unwrap_or_default();
                let top_k = req.params.get("topK").and_then(Value::as_u64).unwrap_or(10) as usize;
                let threshold = req.params.get("threshold").and_then(Value::as_f64);
                let query_vec = req
                    .params
                    .get("queryVector")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(Value::as_f64)
                    .collect::<Vec<_>>();
                self.ensure_cache();
                let corpus_namespace_key = Self::corpus_namespace_key(corpus_id, namespace);
                let mut out: Vec<Value> = self
                    .vector_keys_by_corpus_namespace
                    .get(&corpus_namespace_key)
                    .into_iter()
                    .flat_map(|keys| keys.iter())
                    .filter_map(|key| self.state.vectors.get(key))
                    .map(|v| {
                        let score = Self::cosine(&query_vec, &v.values);
                        (v, score)
                    })
                    .filter(|(_, score)| threshold.is_none_or(|th| *score >= th))
                    .map(|(v, score)| {
                        json!({
                            "id": v.id,
                            "score": score,
                            "metadata": v.metadata
                        })
                    })
                    .collect();
                out.sort_by(|a, b| {
                    let sa = a.get("score").and_then(Value::as_f64).unwrap_or(0.0);
                    let sb = b.get("score").and_then(Value::as_f64).unwrap_or(0.0);
                    sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
                });
                out.truncate(top_k);
                Ok(json!(out))
            }
            "vector_delete_by_document" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let document_id = req.params.get("documentId").and_then(Value::as_str).unwrap_or_default();
                self.state.vectors.retain(|_, v| {
                    if v.corpus_id != corpus_id {
                        return true;
                    }
                    let doc = v.metadata.get("documentId").and_then(Value::as_str).unwrap_or_default();
                    doc != document_id
                });
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(null))
            }
            "memory_save" => {
                let snapshot = req.params.get("snapshot").cloned().unwrap_or_else(|| json!({}));
                let corpus_id = snapshot
                    .get("corpusId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Self::execution_client_error("missing snapshot.corpusId".to_string()))?
                    .to_string();
                self.state.snapshots.insert(corpus_id, snapshot);
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(null))
            }
            "memory_load" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let snapshot = self.state.snapshots.get(corpus_id).cloned().unwrap_or_else(|| {
                    json!({
                        "corpusId": corpus_id,
                        "exportedAt": "",
                        "schemas": [],
                        "facts": [],
                        "passages": [],
                        "schemaVersion": 1
                    })
                });
                Ok(snapshot)
            }
            "memory_save_checkpoint" => {
                let checkpoint = req.params.get("checkpoint").cloned().unwrap_or_else(|| json!({}));
                let job_id = checkpoint
                    .get("jobId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Self::execution_client_error("missing checkpoint.jobId".to_string()))?
                    .to_string();
                self.state.checkpoints.insert(job_id, checkpoint);
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(null))
            }
            "memory_load_checkpoint" => {
                let job_id = req.params.get("jobId").and_then(Value::as_str).unwrap_or_default();
                let checkpoint = self.state.checkpoints.get(job_id).cloned().unwrap_or(Value::Null);
                Ok(checkpoint)
            }
            "memory_validate_integrity" => Ok(json!([])),
            "projection_get_transitions" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                self.ensure_cache();
                let mut out: Vec<Value> = self
                    .edge_keys_by_corpus
                    .get(corpus_id)
                    .into_iter()
                    .flat_map(|keys| keys.iter())
                    .filter_map(|key| self.state.edges.get(key))
                    .map(|e| {
                        json!({
                            "sourceNodeId": e.source_node_id,
                            "targetNodeId": e.target_node_id,
                            "weight": e.weight
                        })
                    })
                    .collect();
                out.sort_by(|a, b| {
                    let ak = a.get("sourceNodeId").and_then(Value::as_str).unwrap_or_default();
                    let bk = b.get("sourceNodeId").and_then(Value::as_str).unwrap_or_default();
                    ak.cmp(bk)
                });
                Ok(json!(out))
            }
            "projection_get_dangling_nodes" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                self.ensure_cache();
                let mut outgoing: HashMap<String, usize> = HashMap::new();
                for edge in self
                    .edge_keys_by_corpus
                    .get(corpus_id)
                    .into_iter()
                    .flat_map(|keys| keys.iter())
                    .filter_map(|key| self.state.edges.get(key))
                {
                    *outgoing.entry(edge.source_node_id.clone()).or_default() += 1;
                }
                let mut dangling: Vec<String> = self
                    .node_keys_by_corpus
                    .get(corpus_id)
                    .into_iter()
                    .flat_map(|keys| keys.iter())
                    .filter_map(|key| self.state.nodes.get(key))
                    .filter(|n| !outgoing.contains_key(&n.node_id))
                    .map(|n| n.node_id.clone())
                    .collect();
                dangling.sort();
                Ok(json!(dangling))
            }
            "projection_get_node_count" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                self.ensure_cache();
                let count = self
                    .node_keys_by_corpus
                    .get(corpus_id)
                    .map(|keys| keys.len())
                    .unwrap_or(0);
                Ok(json!(count))
            }
            "lexical_index_passages" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let passages = req.params.get("passages").and_then(Value::as_array).cloned().unwrap_or_default();
                for passage in passages {
                    let passage_id = passage
                        .get("passageId")
                        .and_then(Value::as_str)
                        .ok_or_else(|| Self::execution_client_error("missing passageId".to_string()))?;
                    let document_id = passage
                        .get("metadata")
                        .and_then(|m| m.get("documentId"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| Self::execution_client_error("missing metadata.documentId".to_string()))?;
                    let text = passage.get("text").and_then(Value::as_str).unwrap_or_default();
                    let item = Passage {
                        passage_id: passage_id.to_string(),
                        corpus_id: corpus_id.to_string(),
                        document_id: document_id.to_string(),
                        text: text.to_string(),
                    };
                    self.state
                        .passages
                        .insert(Self::key(corpus_id, passage_id), item);
                }
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(null))
            }
            "lexical_search" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let query = req.params.get("query").and_then(Value::as_str).unwrap_or_default();
                let top_k = req.params.get("topK").and_then(Value::as_u64).unwrap_or(10) as usize;
                let tokens: Vec<String> = query
                    .to_lowercase()
                    .split_whitespace()
                    .map(ToString::to_string)
                    .collect();
                self.ensure_cache();
                let mut out: Vec<Value> = self
                    .passage_keys_by_corpus
                    .get(corpus_id)
                    .into_iter()
                    .flat_map(|keys| keys.iter())
                    .filter_map(|key| self.state.passages.get(key))
                    .map(|p| {
                        json!({
                            "passageId": p.passage_id,
                            "score": Self::token_score(&p.text, &tokens)
                        })
                    })
                    .filter(|v| v.get("score").and_then(Value::as_f64).unwrap_or(0.0) > 0.0)
                    .collect();
                out.sort_by(|a, b| {
                    let sa = a.get("score").and_then(Value::as_f64).unwrap_or(0.0);
                    let sb = b.get("score").and_then(Value::as_f64).unwrap_or(0.0);
                    match sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal) {
                        std::cmp::Ordering::Equal => {
                            let aid = a.get("passageId").and_then(Value::as_str).unwrap_or_default();
                            let bid = b.get("passageId").and_then(Value::as_str).unwrap_or_default();
                            aid.cmp(bid)
                        }
                        other => other,
                    }
                });
                out.truncate(top_k);
                Ok(json!(out))
            }
            "lexical_delete_by_document" => {
                let corpus_id = req.params.get("corpusId").and_then(Value::as_str).unwrap_or_default();
                let document_id = req.params.get("documentId").and_then(Value::as_str).unwrap_or_default();
                self.state.passages.retain(|_, p| !(p.corpus_id == corpus_id && p.document_id == document_id));
                self.mark_cache_dirty();
                self.persist()
                    .map_err(|err| Self::execution_io_error(format!("persist failed: {err}")))?;
                Ok(json!(null))
            }
            "__debug_force_panic__" => {
                if std::env::var("AGDB_ENABLE_TEST_CRASH").ok().as_deref() == Some("1") {
                    panic!("forced panic for crash audit");
                }
                Err(Self::unsupported_method_error(&req.method))
            }
                _ => Err(Self::unsupported_method_error(&req.method)),
            }
        })();

        match result {
            Ok(value) => RpcResponse {
                id: req.id,
                ok: true,
                result: Some(value),
                error: None,
            },
            Err(err) => {
                let code = err.code.clone();
                let failure_class = err
                    .failure_class
                    .clone()
                    .unwrap_or_else(|| "INTERNAL_BUG".to_string());
                self.append_request_audit_event(&code, &failure_class, &req.id.to_string());
                RpcResponse {
                id: req.id,
                ok: false,
                result: None,
                error: Some(RpcError {
                    code: err.code,
                    message: err.message,
                    failure_class: err.failure_class,
                }),
                }
            }
        }
    }
}

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut db_path = PathBuf::from("aira-graphdb-native.json");
    while let Some(arg) = args.next() {
        if arg == "--db" {
            if let Some(v) = args.next() {
                db_path = PathBuf::from(v);
            }
        }
    }

    let crash_tracker = CrashTracker::new(db_path.with_extension("native-audit.log"));
    let tracker_for_hook = crash_tracker.clone();
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        tracker_for_hook.append_crash_event(Some(101), None, Some(panic_info.to_string()));
        previous_hook(panic_info);
    }));

    let mut server = Server::open(db_path)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line_result in stdin.lock().lines() {
        let line = match line_result {
            Ok(line) => line,
            Err(err) => {
                crash_tracker.append_crash_event(Some(1), None, Some(format!("stdin read failed: {err}")));
                return Err(err);
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let req = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(req) => req,
            Err(err) => {
                let _ = Server::append_request_audit_event_for_path(
                    &server.audit_log_path,
                    "INVALID_REQUEST_JSON",
                    "CLIENT_INPUT",
                    "0",
                );
                let payload = serde_json::to_string(&RpcResponse {
                    id: 0,
                    ok: false,
                    result: None,
                    error: Some(RpcError {
                        code: "INVALID_REQUEST_JSON".to_string(),
                        message: format!("invalid request: {err}"),
                        failure_class: Some("CLIENT_INPUT".to_string()),
                    }),
                })
                .unwrap_or_else(|_| "{\"id\":0,\"ok\":false}".to_string());
                if let Err(write_err) = stdout
                    .write_all(payload.as_bytes())
                    .and_then(|_| stdout.write_all(b"\n"))
                    .and_then(|_| stdout.flush())
                {
                    crash_tracker.append_crash_event(
                        Some(1),
                        None,
                        Some(format!("stdout write failed after invalid request: {write_err}")),
                    );
                    return Err(write_err);
                }
                continue;
            }
        };
        crash_tracker.set_last_request_id(req.id.to_string());
        let resp = server.handle(req);
        let payload = serde_json::to_string(&resp)
            .map_err(|err| io::Error::other(format!("serialize response failed: {err}")))?;
        if let Err(write_err) = stdout
            .write_all(payload.as_bytes())
            .and_then(|_| stdout.write_all(b"\n"))
            .and_then(|_| stdout.flush())
        {
            crash_tracker.append_crash_event(
                Some(1),
                None,
                Some(format!("stdout write failed: {write_err}")),
            );
            return Err(write_err);
        }
    }
    Ok(())
}
