# インストールガイド（日本語）

## 1. 前提条件

- Rust（stable、`cargo` 利用可能）
- Node.js 20+（Node SDK の利用/テスト用）
- Python 3.10+（Python SDK の利用/テスト用）
- Git

## 2. リポジトリ取得

```bash
git clone <your-repo-url> aira-graphdb
cd aira-graphdb
```

## 3. ビルド・動作確認

Rust テスト一式:

```bash
cargo test
```

conformance スイートのみ:

```bash
cargo test --test cypher_conformance
```

生成される conformance アーティファクト:

```text
target/conformance/opencypher9-report.json
```

## 4. SDK テスト

Node SDK:

```bash
cd sdk/node
npm test
cd ../..
```

Python SDK:

```bash
cd sdk/python
PYTHONPATH=. python -m unittest discover -s tests -v
cd ../..
```

## 5. CI release-block ゲート

`/.github/workflows/conformance-gate.yml` で以下を必須化し、失敗時はリリースをブロックします。

- `pass_rate < 100`
- `required_tck_ids` の未解決
- mandatory negative-case セット不充足
- `failed_test_ids` が非空
- native perf gate（`artifacts/native-bench-report.json`）
- native soak gate（`artifacts/native-soak-report.json`、`artifacts/native-audit-events.json`）

soak プロファイル運用:

- `pull_request` => `P0-NATIVE-SOAK-SMOKE`（`durationMinutes=30`）
- `schedule` / `release` => `P0-NATIVE-SOAK`（`durationMinutes=1440`）

CI は以下を artifact として保存します。

- `target/conformance/opencypher9-report.json`
- `artifacts/native-bench-report.json`
- `artifacts/native-soak-report.json`
- `artifacts/native-audit-events.json`

## 6. aira-synapse backend 互換ゲート（Phase 4）

`aira-synapse` 側に専用の互換ワークフローを実装しています。

```text
.github/workflows/aira-synapse-backend-compat.yml
```

このワークフローで以下のジョブ/required context を強制します。

- `storage-port-contract`
- `storage-port-compat`
- `backend-compat`（`merge_group` では `backend-compat-strict` も必須）
- `branch-protection-audit`（`merge_group` では `branch-protection-audit-strict` も必須）

契約定義は本リポジトリ `spec/contracts/` を参照し、以下のartifactを出力します。

- `artifacts/backend-compat-report-untrusted.json`
- `artifacts/backend-compat-report-strict.json`
- `artifacts/branch-protection-audit-untrusted.json`
- `artifacts/branch-protection-audit-strict.json`

## 7. ネイティブRust通信ランタイム

JSON-RPC通信バイナリを起動:

```bash
cargo run --bin aira-graphdb-native -- --db /path/to/aira-graphdb-native.json
```

`aira-synapse` からネイティブ実行を強制する場合:

```bash
export MEMGRAPHRAG_BACKEND=aira-graphdb
export AIRA_GRAPHDB_REPO_PATH=/absolute/path/to/aira-graphdb
```

## 8. Native ゲート確認（ローカル）

native 契約/perf/soak の確認コマンド:

```bash
cargo test --test native_rpc_resilience --quiet
cargo test --test native_perf_gate --quiet
AGDB_NATIVE_SOAK_PROFILE=P0-NATIVE-SOAK-SMOKE cargo test --test native_soak_gate --quiet
cargo test --test native_watchdog --quiet
```

クラッシュ調査ログ検証（強制panicケース含む）:

```bash
cargo test --test native_rpc_resilience -- --nocapture
```
