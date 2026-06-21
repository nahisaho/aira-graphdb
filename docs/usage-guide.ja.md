# 利用ガイド（日本語）

## 1. Rust API

### ハンドシェイク

```rust
use aira_graphdb::protocol::{HandshakeRequest, negotiate};

let response = negotiate(&HandshakeRequest {
    protocol_version: "protocol-p0@1.0.0".into(),
    canonical_type_system_version: "canonical-types@1.0.0".into(),
})?;
assert!(response.accepted);
# Ok::<(), aira_graphdb::errors::GraphDbError>(())
```

### クエリ実行

```rust
use aira_graphdb::graph::InMemoryGraphStore;
use aira_graphdb::query::{execute_query, execute_query_with_dialect, CypherDialect};

let mut store = InMemoryGraphStore::new();
execute_query(&mut store, "CREATE (n:Paper {title:'GraphDB'})")?;
execute_query(&mut store, "MERGE (n:Paper {title:'GraphDB'}) ON MATCH SET n.status='existing'")?;
let _ = execute_query(&mut store, "MATCH (n:Paper) WITH n RETURN n ORDER BY n.id SKIP 0 LIMIT 1")?;
let _ = execute_query_with_dialect(
    &mut store,
    "MATCH (n) RETURN n UNION MATCH (m) RETURN m",
    CypherDialect::Neo4jCompat,
)?;
# Ok::<(), aira_graphdb::errors::GraphDbError>(())
```

## 2. サーバー通信（JSON Lines）

実行境界は `CONNECTED -> TLS_OK -> AUTH_OK -> APP_READY` です。

```json
{"type":"handshake","protocol_version":"protocol-p0@1.0.0","canonical_type_system_version":"canonical-types@1.0.0"}
{"type":"auth","bearer_token":"<jwt>"}
{"type":"begin_tx"}
{"type":"query","tx_id":"tx-1","query":"CREATE (n:Paper {title:'GraphDB'})"}
{"type":"commit_tx","tx_id":"tx-1"}
```

`APP_READY` 前のアプリ要求は `AUTH_REQUIRED` で拒否されます。

### データ登録とインデックス作成（native JSON-RPC）

sidecar 起動:

```bash
cargo run --bin aira-graphdb-native -- --db /path/to/aira-graphdb-native.db
```

ノード/エッジ登録:

```json
{"id":1,"method":"upsert_nodes","params":{"nodes":[{"nodeId":"n1","corpusId":"c1","layer":"paper","ref":{},"label":"Paper"}]}}
{"id":2,"method":"upsert_edges","params":{"edges":[{"edgeId":"e1","corpusId":"c1","sourceNodeId":"n1","targetNodeId":"n1","relation":"SELF","weight":1.0}]}}
```

インデックス用データ登録:

```json
{"id":3,"method":"vector_upsert","params":{"vectors":[{"id":"v1","corpusId":"c1","namespace":"default","values":[0.1,0.2,0.3],"metadata":{"documentId":"d1"}}]}}
{"id":4,"method":"lexical_index_passages","params":{"passages":[{"passageId":"p1","corpusId":"c1","documentId":"d1","text":"graph database"}]}}
```

検索:

```json
{"id":5,"method":"vector_search","params":{"corpusId":"c1","namespace":"default","queryVector":[0.1,0.2,0.3],"topK":10}}
{"id":6,"method":"lexical_search","params":{"corpusId":"c1","query":"graph database","topK":10}}
```

現在の RPC では `create index` は独立メソッドではありません。グラフ/ベクトル/全文インデックスは upsert/delete 成功時に自動更新されます。

### 利用可能クエリ（RPCメソッド）一覧

| メソッド | 説明 |
|---|---|
| `ping` | ヘルスチェック（`{"pong":true}` を返す） |
| `upsert_nodes` / `upsert_edges` | ノード/エッジを登録・更新 |
| `get_node` / `get_nodes` | 単一ノード取得 / 条件付きノード一覧取得 |
| `get_edges` / `get_adjacent` | エッジ一覧取得 / ノード隣接エッジ取得 |
| `delete_nodes` / `delete_edges` | 指定ノード/エッジを削除 |
| `delete_by_document` / `delete_by_corpus` | ドキュメント単位 / コーパス単位の一括削除 |
| `vector_upsert` / `vector_search` / `vector_delete_by_document` | ベクトルデータ登録・検索・削除 |
| `lexical_index_passages` / `lexical_search` / `lexical_delete_by_document` | 全文インデックス登録・検索・削除 |
| `memory_save` / `memory_load` | メモリスナップショット保存・読込 |
| `memory_save_checkpoint` / `memory_load_checkpoint` | チェックポイント保存・読込 |
| `memory_validate_integrity` | メモリ整合性検証（現状は空配列を返す） |
| `projection_get_transitions` / `projection_get_dangling_nodes` / `projection_get_node_count` | 投影情報（遷移/ダングリング/ノード数）の取得 |

## 3. Conformance レポート

`build_and_persist_conformance_report` が以下を生成します。

```text
target/conformance/opencypher9-report.json
```

レポートには次を含みます。

- `pass_rate`
- `unresolved_tck_ids`
- `mandatory_negative_cases_satisfied`
- `failed_test_ids`
- clause/feature 別 PASS/FAIL

## 4. 監査ログイベント

サーバー/runtime の監査ログは `<db-file>.audit.log` に追記されます。  
Native JSON-RPC の異常系監査ログは `<db-file>.native-audit.log` に追記されます。

実装済みイベント種別:

- `AUTH_FAILED`
- `AUTH_REQUIRED_REJECTED`
- `ROLLBACK_EXECUTED`
- `REFERENTIAL_INTEGRITY_VIOLATION`
- `DETERMINISTIC_CONFLICT`

Native 異常系イベントは次の必須項目を持ちます。

- `errorCode`
- `failureClass`（`INTERNAL_BUG | IO_FAILURE | OOM | TIMEOUT | CLIENT_INPUT`）
- `requestId`
- `timestamp`

Native クラッシュ時は `PROCESS_CRASH` が自動記録され、以下を含みます。

- `errorCode`
- `timestamp`
- `processExitCode`
- `signal`
- `lastRequestId`
- `uptimeSec`
- `cause`（取得できる場合）

## 5. Native Perf/Soak 品質ゲート成果物

Native ゲート実行で以下を生成します。

```text
artifacts/native-bench-report.json
artifacts/native-soak-report.json
artifacts/native-audit-events.json
```

`native-soak-report.json` の主要フィールド:

- `profile`（PR: `P0-NATIVE-SOAK-SMOKE`、schedule/release: `P0-NATIVE-SOAK`）
- `durationMinutes`（30 または 1440）
- `crashCount`（必須: `0`）
- `internalFailureRate`（必須: `<= 0.001`）
- `requiredFieldsValid`
- `gatePass`

## 6. openCypher 対応状況（現状）

| 領域 | 状態 | 補足 |
|---|---|---|
| `MATCH`, `OPTIONAL MATCH`, `WHERE`, `WITH`, `RETURN` | 対応（プロファイル範囲） | `WITH` 別名スコープ検証あり |
| `ORDER BY`, `SKIP`, `LIMIT` | 対応 | 行比較戦略を ordered/multiset で切替（`resolve_row_comparison_strategy`） |
| `CREATE`, `MERGE`, `SET`, `REMOVE`, `DELETE`, `DETACH` | 対応（プロファイル範囲） | `MERGE` の on-create/on-match 実装済み |
| `UNWIND` + 集約 | 対応 | `count/sum/avg/min/max/collect` |
| `CALL` + APOC サブセット（`apoc.meta.schema`, `apoc.coll.toSet`, `apoc.text.join`, `apoc.refactor.rename.label`） | 対応（manifest制御） | 許可集合は `spec/contracts/apoc-procedure-manifest.v1.0.0.yaml` で固定 |
| Neo4j 互換 Cypher ダイアレクト | 対応（ガード付き） | `execute_query_with_dialect(..., CypherDialect::Neo4jCompat)` は `UNION` / `UNION ALL` / `CASE` に対応し、`FOREACH` / variable-length path / `shortestPath(...)` を含む非対応拡張を `UNSUPPORTED_FEATURE` で拒否 |
| 関係走査パターン（`()-[]->()`, `()-[]-()`） | 対応 | 単一ホップ走査 + `OPTIONAL MATCH/WHERE/WITH/ORDER BY/SKIP/LIMIT` 契約ケース |

## 7. aira-synapse との Storage Port 互換

正準契約:

```text
spec/contracts/aira-synapse-storage-ports.v1.0.0.json
```

Phase 4 で実装済みの主な内容:

- `IGraphStore / IVectorIndex / IMemoryStore / IGraphProjection / ILexicalRetriever` のAST抽出ベース契約一致検証
- `memgraphrag` の `storageFactory` で `backend=aira-graphdb` を選択可能化
- storage-port互換統合テスト（`graph/vector/lexical/memory/projection`）
- vector/lexical互換評価器と固定validationエラーコード
  - `INVALID_TOP_K`
  - `INVALID_THRESHOLD`
  - `INVALID_CORPUS_ID`
  - `INVALID_NAMESPACE`

互換ワークフローで参照する契約:

- `.github/workflows/aira-synapse-backend-compat.yml`
- `spec/contracts/p0-compat-test-map.v1.0.0.json`
- `spec/contracts/backend-compat-failure-report.v1.0.0.json`
- `spec/contracts/branch-protection-policy.v1.0.0.json`
- `spec/contracts/event-scope-map.v1.0.0.json`

### ネイティブ通信経路

`backend=aira-graphdb` は、以下のネイティブRustサイドカー経路を使用します。

- Rustバイナリ: `aira-graphdb-native`（`src/bin/aira-graphdb-native.rs`）
- 通信: stdin/stdout 上の JSON-RPC
- 永続化: `--db <path>` で指定した compact binary スナップショット/WAL

この経路により、`aira-graphdb` backend での従来のSQLite互換フォールバックを置き換えています。

## 8. Native RPC レジリエンス契約

native レジリエンス契約テストは、不正JSON・未知メソッド・実行失敗で固定エラーコードを返しつつ、sidecar プロセスが継続稼働することを検証します。

```bash
cargo test --test native_rpc_resilience -- --nocapture
```

同テストには、`PROCESS_CRASH` 自動監査を検証する強制panicケースも含まれます。

## 9. 外部 watchdog によるクラッシュ追跡

`SIGKILL` など kill-level の終了は外部 watchdog 経路で検知し、以下に保存します。

```text
artifacts/watchdog-crash-report.json
```

ローカル実行:

```bash
cargo test --test native_watchdog -- --nocapture
```
