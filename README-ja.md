# aira-graphdb

AIRA エコシステム向けの Rust 製 GraphDB です。  
以下 2 つの利用形態を同一コアで提供することを目的としています。

- **Embedded モード**（SQLite/LadybugDB 風のファイルベース利用）
- **Server モード**（Neo4j 風の常駐/TCP 利用）

本リポジトリには、SDD の Phase 1〜4 実装成果物が含まれ、native Rust 通信層の性能/安定性ゲートまで実装済みです。

## 現在の成果物

- 要件定義: `spec/REQ-AIRA-GRAPHDB-001.md`
- 設計書: `spec/DES-AIRA-GRAPHDB-001.md`
- ADR: `spec/ADR-AGDB-001.md`
- タスク分解: `spec/PLAN-AIRA-GRAPHDB-001.md`
- 不変仕様:
  - `spec/contracts/agdb-typemap-p0.v1.0.0.json`
  - `spec/contracts/agdb-cypher-p0-grammar.v1.0.0.json`
  - `spec/contracts/agdb-error-codes.v1.0.0.json`

## ディレクトリ構成

```text
src/
  auth.rs
  audit.rs
  bench.rs
  contracts.rs
  conformance.rs
  errors.rs
  graph.rs
  lock.rs
  native_bench.rs
  protocol.rs
  query.rs
  runtime.rs
  server.rs
  storage.rs
  tx.rs
  bin/
    aira-graphdb-native.rs
tests/
  cypher_conformance.rs
  integration_flow.rs
  native_perf_gate.rs
  native_rpc_resilience.rs
  native_soak_gate.rs
sdk/
  node/
  python/
spec/
  REQ-*.md
  DES-*.md
  ADR-*.md
  PLAN-*.md
  contracts/*.json
```

## 前提環境

- Rust（stable + cargo）
- Node.js（Node SDK テスト用）
- Python 3.10+（Python SDK テスト用）

## ビルド・テスト

Rust テスト:

```bash
cargo test
```

native 通信層テスト:

```bash
cargo test --test native_rpc_resilience
cargo test --test native_perf_gate
cargo test --test native_soak_gate
```

Node SDK テスト:

```bash
cd sdk/node
npm test
```

Python SDK テスト:

```bash
cd sdk/python
PYTHONPATH=. python -m unittest discover -s tests -v
```

## 実装済み機能

- 型変換/Cypher P0/エラー仕様の契約ロード
- エラーコードレジストリ
- In-Memory グラフモデル（Node/Edge CRUD）
- スナップショット + WAL 永続化/復旧
- トランザクション管理（begin/commit/rollback）
- ファイル書込みロック（`WRITE_LOCK_CONFLICT`）
- P0 向け最小クエリ実行層
- プロトコルハンドシェイク
- 認証境界検証（TLS/JWT ポリシー）
- Embedded/Server ランタイム骨格
- Native ベンチマーク/soak プロファイル補助
- Native 異常系監査ログ（`<db>.native-audit.log`）
- Native ランタイムクラッシュ自動監査（`PROCESS_CRASH`）
- kill-level 終了を対象とする外部 watchdog クラッシュ追跡（`artifacts/watchdog-crash-report.json`）
- Native CI 品質ゲート
  - perf artifact: `artifacts/native-bench-report.json`
  - soak artifact: `artifacts/native-soak-report.json`
  - audit artifact: `artifacts/native-audit-events.json`

## CI ゲート運用（native 通信層）

- `pull_request`: `P0-NATIVE-SOAK-SMOKE`（30分プロファイル契約）
- `schedule` / `release`: `P0-NATIVE-SOAK`（24時間プロファイル契約）
- 必須しきい値:
  - `crashCount == 0`
  - `internalFailureRate <= 0.001`
  - Native 異常系イベント監査ログ必須項目の充足

## Native クラッシュ調査

native sidecar が panic/異常終了した場合、`<db>.native-audit.log` に `PROCESS_CRASH` が自動記録されます。

- `errorCode`
- `timestamp`
- `processExitCode`
- `signal`
- `lastRequestId`
- `uptimeSec`
- `cause`（取得できる場合）

`CALL/APOC` は `spec/contracts/apoc-procedure-manifest.v1.0.0.yaml` の許可集合で実行し、関係走査 `()-[]->()/()-[]-()` は conformance ケースで検証します。

## データ登録とインデックス作成（native JSON-RPC）

sidecar 起動:

```bash
cargo run --bin aira-graphdb-native -- --db /path/to/aira-graphdb-native.json
```

グラフデータ登録:

```json
{"id":1,"method":"upsert_nodes","params":{"nodes":[{"nodeId":"n1","corpusId":"c1","layer":"paper","ref":{},"label":"Paper"}]}}
{"id":2,"method":"upsert_edges","params":{"edges":[{"edgeId":"e1","corpusId":"c1","sourceNodeId":"n1","targetNodeId":"n1","relation":"SELF","weight":1.0}]}}
```

ベクトル/全文インデックス用データ登録:

```json
{"id":3,"method":"vector_upsert","params":{"vectors":[{"id":"v1","corpusId":"c1","namespace":"default","values":[0.1,0.2,0.3],"metadata":{"documentId":"d1"}}]}}
{"id":4,"method":"lexical_index_passages","params":{"passages":[{"passageId":"p1","corpusId":"c1","documentId":"d1","text":"graph database"}]}}
```

インデックス検索:

```json
{"id":5,"method":"vector_search","params":{"corpusId":"c1","namespace":"default","queryVector":[0.1,0.2,0.3],"topK":10}}
{"id":6,"method":"lexical_search","params":{"corpusId":"c1","query":"graph database","topK":10}}
```

補足: 現在の native ランタイムでは、グラフ/ベクトル/全文のインメモリインデックスは upsert/delete 時に自動更新されるため、明示的な `create index` コマンドは不要です。

### 利用可能クエリ（RPCメソッド）一覧

| メソッド | 説明 |
|---|---|
| `ping` | ヘルスチェック |
| `upsert_nodes`, `upsert_edges` | ノード/エッジ登録・更新 |
| `get_node`, `get_nodes`, `get_edges`, `get_adjacent` | グラフ読み取り |
| `delete_nodes`, `delete_edges`, `delete_by_document`, `delete_by_corpus` | グラフ削除 |
| `vector_upsert`, `vector_search`, `vector_delete_by_document` | ベクトル登録・検索・削除 |
| `lexical_index_passages`, `lexical_search`, `lexical_delete_by_document` | 全文インデックス登録・検索・削除 |
| `memory_save`, `memory_load`, `memory_save_checkpoint`, `memory_load_checkpoint`, `memory_validate_integrity` | メモリ保存/復元/整合性確認 |
| `projection_get_transitions`, `projection_get_dangling_nodes`, `projection_get_node_count` | 投影情報取得 |
