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
use aira_graphdb::query::execute_query;

let mut store = InMemoryGraphStore::new();
execute_query(&mut store, "CREATE (n:Paper {title:'GraphDB'})")?;
execute_query(&mut store, "MERGE (n:Paper {title:'GraphDB'}) ON MATCH SET n.status='existing'")?;
let _ = execute_query(&mut store, "MATCH (n:Paper) WITH n RETURN n ORDER BY n.id SKIP 0 LIMIT 1")?;
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
| 非対応拡張（`CALL`、vendor procedure、APOC） | 明示拒否 | `UNSUPPORTED_FEATURE` + `details.unsupported_clause` |
| 関係走査パターン（`()-[]->()`） | 未対応 | 現行実行スコープ外 |

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
- 永続化: `--db <path>` で指定した JSON スナップショット

この経路により、`aira-graphdb` backend での従来のSQLite互換フォールバックを置き換えています。

## 8. Native RPC レジリエンス契約

native レジリエンス契約テストは、不正JSON・未知メソッド・実行失敗で固定エラーコードを返しつつ、sidecar プロセスが継続稼働することを検証します。

```bash
cargo test --test native_rpc_resilience -- --nocapture
```

同テストには、`PROCESS_CRASH` 自動監査を検証する強制panicケースも含まれます。
