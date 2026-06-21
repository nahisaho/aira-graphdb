# Extension and Customization Guide (English)

## 1. Adding custom APOC procedures

The APOC procedure set is defined in the manifest:

```text
spec/contracts/apoc-procedure-manifest.v1.0.0.yaml
```

To add a custom procedure:

### 1.1 Update manifest

```yaml
procedures:
  apoc.custom.myProc:
    name: "apoc.custom.myProc"
    namespace: "custom"
    deprecated: false
    mode: "READ"  # or "WRITE"
    signature: "(input :: STRING) :: (output :: ANY)"
    description: "Custom procedure for demo"
    failureCodes:
      - "INVALID_INPUT"
      - "CUSTOM_ERROR"
    sideEffects: false
```

### 1.2 Implement in Rust

In `src/query.rs`, add handler:

```rust
fn call_custom_myproc(args: Vec<Value>) -> Result<Vec<Value>, GraphDbError> {
    let input = args.get(0)
        .and_then(Value::as_str)
        .ok_or_else(|| GraphDbError::client_error("INVALID_INPUT"))?;
    
    // Custom logic
    let output = format!("Processed: {}", input);
    Ok(vec![json!(output)])
}
```

Register in dispatcher:

```rust
"apoc.custom.myProc" => call_custom_myproc(args),
```

### 1.3 Test

```rust
#[test]
fn test_custom_apoc() {
    let result = call_custom_myproc(vec![json!("hello")]);
    assert_eq!(result, Ok(vec![json!("Processed: hello")]));
}
```

## 2. Adding memory storage types

The memory storage system supports snapshots and checkpoints. To add a new storage type:

### 2.1 Define type in contracts

In `spec/contracts/agdb-typemap-p0.v1.0.0.json`:

```json
{
  "memoryTypes": {
    "custom": {
      "fields": {
        "id": "STRING",
        "data": "JSON"
      },
      "indexed": ["id"]
    }
  }
}
```

### 2.2 Implement storage

In `src/storage.rs`:

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct CustomMemoryRecord {
    pub id: String,
    pub data: serde_json::Value,
}

impl CustomMemoryRecord {
    pub fn new(id: String, data: Value) -> Self {
        Self { id, data }
    }
}
```

Add to snapshot:

```rust
pub struct DbSnapshot {
    pub nodes: HashMap<String, GraphNode>,
    pub edges: HashMap<String, GraphEdge>,
    pub vectors: HashMap<String, VectorRecord>,
    pub passages: HashMap<String, Passage>,
    pub custom_records: HashMap<String, CustomMemoryRecord>,  // NEW
    pub snapshots: HashMap<String, Value>,
    pub checkpoints: HashMap<String, Value>,
}
```

### 2.3 Add RPC methods

In `src/bin/aira-graphdb-native.rs`:

```rust
"custom_upsert" => {
    let records = req.params.get("records")
        .and_then(Value::as_array).cloned().unwrap_or_default();
    for record in records {
        let parsed = serde_json::from_value::<CustomMemoryRecord>(record)?;
        self.state.custom_records.insert(parsed.id.clone(), parsed);
    }
    Ok(json!(null))
}

"custom_search" => {
    let results = self.state.custom_records.iter()
        .filter(|(id, _)| id.starts_with("prefix"))
        .map(|(_, r)| json!(r))
        .collect::<Vec<_>>();
    Ok(json!(results))
}
```

## 3. Adding new query language support

To support additional query languages (not Cypher):

Neo4j-compatible Cypher is handled as a guarded dialect inside `src/query.rs`; unsupported extensions are rejected with `UNSUPPORTED_FEATURE` and an `unsupported_clause` detail.

In the current baseline, `FOREACH`, `EXISTS {}`, `CALL {}`, `shortestPath(...)`, variable-length paths, pattern comprehension, and schema/index mutation remain rejected rather than partially executed.

### 3.1 Define grammar

Create new contract:

```text
spec/contracts/agdb-custom-ql-grammar.v1.0.0.json
```

### 3.2 Implement parser

In new module `src/query_lang_custom.rs`:

```rust
pub struct CustomQLParser;

impl CustomQLParser {
    pub fn parse(input: &str) -> Result<QueryPlan, ParseError> {
        // Custom parsing logic
        Ok(QueryPlan::default())
    }
}
```

### 3.3 Register in executor

In `src/query.rs`:

```rust
pub fn execute_query(store: &mut impl IGraphStore, query: &str) -> Result<Vec<Row>, GraphDbError> {
    if query.starts_with("CUSTOM::") {
        let plan = CustomQLParser::parse(&query[8..])?;
        execute_plan(store, plan)
    } else {
        // Existing Cypher path
        execute_cypher(store, query)
    }
}
```

## 4. Adding vector embedding backends

The current vector search uses cosine similarity with in-memory storage. To add external backends:

### 4.1 Define backend interface

Create `src/vector_backends.rs`:

```rust
pub trait VectorBackend {
    fn upsert(&mut self, record: VectorRecord) -> Result<(), GraphDbError>;
    fn search(&self, query: &[f64], top_k: usize) -> Result<Vec<String>, GraphDbError>;
    fn delete(&mut self, id: &str) -> Result<(), GraphDbError>;
}

pub struct LocalBackend {
    records: HashMap<String, VectorRecord>,
}

pub struct PineconeBackend {
    client: PineconeClient,
    namespace: String,
}
```

### 4.2 Implement backend

```rust
impl VectorBackend for PineconeBackend {
    fn search(&self, query: &[f64], top_k: usize) -> Result<Vec<String>, GraphDbError> {
        let results = self.client.query(
            &self.namespace,
            query,
            top_k,
            None
        ).map_err(|e| GraphDbError::io_error(e.to_string()))?;
        Ok(results.iter().map(|r| r.id.clone()).collect())
    }
}
```

### 4.3 Register in runtime

In `src/bin/aira-graphdb-native.rs`:

```rust
let backend: Box<dyn VectorBackend> = match env::var("AGDB_VECTOR_BACKEND") {
    Ok(b) if b == "pinecone" => Box::new(PineconeBackend::new()?),
    _ => Box::new(LocalBackend::new()),
};
```

## 5. Custom transaction isolation levels

To add custom transaction isolation:

### 5.1 Define isolation levels

In `src/tx.rs`:

```rust
pub enum IsolationLevel {
    READ_UNCOMMITTED,
    READ_COMMITTED,
    REPEATABLE_READ,
    SERIALIZABLE,
    CUSTOM_SNAPSHOT_ISOLATION,  // NEW
}
```

### 5.2 Implement isolation logic

```rust
impl Transaction {
    pub fn with_isolation(mut self, level: IsolationLevel) -> Self {
        self.isolation_level = level;
        self
    }
    
    pub fn validate_isolation(&self) -> Result<(), GraphDbError> {
        match self.isolation_level {
            IsolationLevel::CUSTOM_SNAPSHOT_ISOLATION => {
                self.check_snapshot_consistency()
            }
            _ => Ok(()),
        }
    }
}
```

### 5.3 Test

```rust
#[test]
fn test_snapshot_isolation() {
    let tx1 = Transaction::new().with_isolation(IsolationLevel::CUSTOM_SNAPSHOT_ISOLATION);
    let tx2 = Transaction::new().with_isolation(IsolationLevel::CUSTOM_SNAPSHOT_ISOLATION);
    
    tx1.write(...).unwrap();
    assert!(tx2.read(...).is_ok()); // Should see pre-snapshot state
    assert!(tx1.commit().is_ok());
    assert!(tx2.read(...).is_ok()); // Now sees committed state
}
```

## 6. Adding custom audit event types

To track domain-specific events:

### 6.1 Define event type

In `spec/contracts/agdb-error-codes.v1.0.0.json`, add:

```json
{
  "auditEventTypes": {
    "CUSTOM_BUSINESS_EVENT": {
      "level": "INFO",
      "category": "BUSINESS_LOGIC",
      "required_fields": ["userId", "action", "entityId"]
    }
  }
}
```

### 6.2 Log custom events

In `src/audit.rs`:

```rust
pub fn log_custom_event(
    event_type: &str,
    details: &HashMap<String, String>
) -> Result<(), GraphDbError> {
    let event = json!({
        "type": event_type,
        "timestamp": Utc::now().to_rfc3339(),
        "details": details
    });
    append_audit_log(&event)
}
```

### 6.3 Use in application

```rust
log_custom_event("CUSTOM_BUSINESS_EVENT", &hashmap![
    "userId".to_string() => user_id,
    "action".to_string() => "CREATE_DOCUMENT",
    "entityId".to_string() => doc_id,
])?;
```

## 7. Extension testing

### 7.1 Unit test template

```rust
#[cfg(test)]
mod custom_extension_tests {
    use super::*;

    #[test]
    fn test_extension_basic() {
        // Setup
        let mut store = InMemoryGraphStore::new();
        
        // Execute
        let result = execute_custom_operation(&mut store, &input);
        
        // Verify
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_output);
    }
    
    #[test]
    fn test_extension_error_handling() {
        let result = execute_custom_operation(&mut store, &invalid_input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "CUSTOM_ERROR");
    }
}
```

### 7.2 Integration test

```rust
#[test]
fn test_extension_with_full_stack() {
    let mut client = GraphDbClient::new();
    client.connect().unwrap();
    
    // Test extension through RPC
    let response = client.call_rpc("custom_method", json!({
        "param": "value"
    })).unwrap();
    
    assert_eq!(response.get("result"), Some(&expected_value));
}
```

## 8. Extension checklist

Before submitting an extension:

- [ ] Feature implemented and tested
- [ ] Manifest/contract updated
- [ ] Error codes documented
- [ ] Backward compatibility verified (no breaking changes to public API)
- [ ] Performance benchmarked
- [ ] Audit events logged
- [ ] Documentation updated
- [ ] CI passes all tests
- [ ] Changelog entry added

## 9. Contributing extensions back

To contribute extensions:

1. Create feature branch: `git checkout -b feature/my-extension`
2. Implement and test (see checklist above)
3. Create pull request with:
   - Feature description
   - Rationale for extension
   - Usage examples
   - Test results
4. Code review by maintainers
5. Merge and release in next version
