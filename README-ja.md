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
