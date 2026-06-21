# 拡張ガイド（日本語）

## 1. カスタム APOC プロシージャの追加

APOC プロシージャはマニフェストで定義されています：

```text
spec/contracts/apoc-procedure-manifest.v1.0.0.yaml
```

カスタムプロシージャを追加するには：

### 1.1 マニフェスト更新

```yaml
procedures:
  apoc.custom.myProc:
    name: "apoc.custom.myProc"
    namespace: "custom"
    deprecated: false
    mode: "READ"  # または "WRITE"
    signature: "(input :: STRING) :: (output :: ANY)"
    description: "デモ用カスタムプロシージャ"
    failureCodes:
      - "INVALID_INPUT"
      - "CUSTOM_ERROR"
    sideEffects: false
```

### 1.2 Rust で実装

`src/query.rs` でハンドラを追加：

```rust
fn call_custom_myproc(args: Vec<Value>) -> Result<Vec<Value>, GraphDbError> {
    let input = args.get(0)
        .and_then(Value::as_str)
        .ok_or_else(|| GraphDbError::client_error("INVALID_INPUT"))?;
    
    // カスタムロジック
    let output = format!("処理済み: {}", input);
    Ok(vec![json!(output)])
}
```

ディスパッチャに登録：

```rust
"apoc.custom.myProc" => call_custom_myproc(args),
```

### 1.3 テスト

```rust
#[test]
fn test_custom_apoc() {
    let result = call_custom_myproc(vec![json!("hello")]);
    assert_eq!(result, Ok(vec![json!("処理済み: hello")]));
}
```

## 2. メモリストレージ型の追加

メモリストレージシステムはスナップショットとチェックポイントをサポートしています。新しいストレージ型を追加するには：

### 2.1 契約内で型を定義

`spec/contracts/agdb-typemap-p0.v1.0.0.json` で：

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

### 2.2 ストレージを実装

`src/storage.rs` で：

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

スナップショットに追加：

```rust
pub struct DbSnapshot {
    pub nodes: HashMap<String, GraphNode>,
    pub edges: HashMap<String, GraphEdge>,
    pub vectors: HashMap<String, VectorRecord>,
    pub passages: HashMap<String, Passage>,
    pub custom_records: HashMap<String, CustomMemoryRecord>,  // 新規
    pub snapshots: HashMap<String, Value>,
    pub checkpoints: HashMap<String, Value>,
}
```

### 2.3 RPC メソッドを追加

`src/bin/aira-graphdb-native.rs` で：

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

## 3. 新しいクエリ言語サポートの追加

Cypher 以外の追加クエリ言語をサポートするには：

### 3.1 文法を定義

新しい契約ファイルを作成：

```text
spec/contracts/agdb-custom-ql-grammar.v1.0.0.json
```

### 3.2 パーサーを実装

新しいモジュール `src/query_lang_custom.rs` で：

```rust
pub struct CustomQLParser;

impl CustomQLParser {
    pub fn parse(input: &str) -> Result<QueryPlan, ParseError> {
        // カスタムパース処理
        Ok(QueryPlan::default())
    }
}
```

### 3.3 エクゼキューターに登録

`src/query.rs` で：

```rust
pub fn execute_query(store: &mut impl IGraphStore, query: &str) -> Result<Vec<Row>, GraphDbError> {
    if query.starts_with("CUSTOM::") {
        let plan = CustomQLParser::parse(&query[8..])?;
        execute_plan(store, plan)
    } else {
        // 既存の Cypher パス
        execute_cypher(store, query)
    }
}
```

## 4. ベクトル埋め込みバックエンドの追加

現在のベクトル検索はコサイン類似度をメモリ内ストレージで使用しています。外部バックエンドを追加するには：

### 4.1 バックエンドインターフェースを定義

`src/vector_backends.rs` を作成：

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

### 4.2 バックエンドを実装

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

### 4.3 ランタイムに登録

`src/bin/aira-graphdb-native.rs` で：

```rust
let backend: Box<dyn VectorBackend> = match env::var("AGDB_VECTOR_BACKEND") {
    Ok(b) if b == "pinecone" => Box::new(PineconeBackend::new()?),
    _ => Box::new(LocalBackend::new()),
};
```

## 5. カスタムトランザクション分離レベル

カスタムトランザクション分離を追加するには：

### 5.1 分離レベルを定義

`src/tx.rs` で：

```rust
pub enum IsolationLevel {
    READ_UNCOMMITTED,
    READ_COMMITTED,
    REPEATABLE_READ,
    SERIALIZABLE,
    CUSTOM_SNAPSHOT_ISOLATION,  // 新規
}
```

### 5.2 分離ロジックを実装

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

### 5.3 テスト

```rust
#[test]
fn test_snapshot_isolation() {
    let tx1 = Transaction::new().with_isolation(IsolationLevel::CUSTOM_SNAPSHOT_ISOLATION);
    let tx2 = Transaction::new().with_isolation(IsolationLevel::CUSTOM_SNAPSHOT_ISOLATION);
    
    tx1.write(...).unwrap();
    assert!(tx2.read(...).is_ok()); // スナップショット前の状態を見る
    assert!(tx1.commit().is_ok());
    assert!(tx2.read(...).is_ok()); // コミット後の状態を見る
}
```

## 6. カスタム監査イベント型の追加

ドメイン固有のイベントを追跡するには：

### 6.1 イベント型を定義

`spec/contracts/agdb-error-codes.v1.0.0.json` で追加：

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

### 6.2 カスタムイベントをログ

`src/audit.rs` で：

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

### 6.3 アプリケーションで使用

```rust
log_custom_event("CUSTOM_BUSINESS_EVENT", &hashmap![
    "userId".to_string() => user_id,
    "action".to_string() => "CREATE_DOCUMENT",
    "entityId".to_string() => doc_id,
])?;
```

## 7. 拡張テスト

### 7.1 ユニットテストテンプレート

```rust
#[cfg(test)]
mod custom_extension_tests {
    use super::*;

    #[test]
    fn test_extension_basic() {
        // セットアップ
        let mut store = InMemoryGraphStore::new();
        
        // 実行
        let result = execute_custom_operation(&mut store, &input);
        
        // 検証
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

### 7.2 統合テスト

```rust
#[test]
fn test_extension_with_full_stack() {
    let mut client = GraphDbClient::new();
    client.connect().unwrap();
    
    // RPC を通じて拡張機能をテスト
    let response = client.call_rpc("custom_method", json!({
        "param": "value"
    })).unwrap();
    
    assert_eq!(response.get("result"), Some(&expected_value));
}
```

## 8. 拡張チェックリスト

拡張を提出する前に：

- [ ] 機能実装・テスト完了
- [ ] マニフェスト・契約更新
- [ ] エラーコード文書化
- [ ] 後方互換性確認（公開 API への破壊的変更なし）
- [ ] パフォーマンス計測
- [ ] 監査イベントログ
- [ ] ドキュメント更新
- [ ] CI 全テスト合格
- [ ] CHANGELOG エントリ追加

## 9. 拡張の貢献

拡張を貢献するには：

1. フィーチャーブランチを作成: `git checkout -b feature/my-extension`
2. 実装とテスト（上記チェックリスト参照）
3. プルリクエスト作成：
   - フィーチャー説明
   - 拡張の理由
   - 使用例
   - テスト結果
4. メンテナーによるコードレビュー
5. マージと次バージョンでリリース
