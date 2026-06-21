# Installation Guide (English)

## 1. Prerequisites

- Rust (stable, with `cargo`)
- Node.js 20+ (for Node SDK usage/tests)
- Python 3.10+ (for Python SDK usage/tests)
- Git

## 2. Clone Repository

```bash
git clone <your-repo-url> aira-graphdb
cd aira-graphdb
```

## 3. Build and Verify

Run all Rust tests:

```bash
cargo test
```

Run conformance suite only:

```bash
cargo test --test cypher_conformance
```

Generated conformance artifact:

```text
target/conformance/opencypher9-report.json
```

## 4. SDK Test Commands

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

## 5. CI Release-Block Gate

`/.github/workflows/conformance-gate.yml` enforces release blocking when any of these fail:

- `pass_rate < 100`
- unresolved `required_tck_ids`
- mandatory negative-case set incomplete
- non-empty `failed_test_ids`
- native perf gate (`artifacts/native-bench-report.json`)
- native soak gate (`artifacts/native-soak-report.json`, `artifacts/native-audit-events.json`)

Soak profile policy:

- `pull_request` => `P0-NATIVE-SOAK-SMOKE` (`durationMinutes=30`)
- `schedule` / `release` => `P0-NATIVE-SOAK` (`durationMinutes=1440`)

The workflow uploads these artifacts:

- `target/conformance/opencypher9-report.json`
- `artifacts/native-bench-report.json`
- `artifacts/native-soak-report.json`
- `artifacts/native-audit-events.json`

## 6. aira-synapse Backend Compatibility Gate (Phase 4)

`aira-synapse` now includes a dedicated compatibility workflow:

```text
.github/workflows/aira-synapse-backend-compat.yml
```

It enforces these jobs and required contexts:

- `storage-port-contract`
- `storage-port-compat`
- `backend-compat` (+ `backend-compat-strict` on `merge_group`)
- `branch-protection-audit` (+ `branch-protection-audit-strict` on `merge_group`)

The workflow consumes contracts from this repository under `spec/contracts/` and uploads:

- `artifacts/backend-compat-report-untrusted.json`
- `artifacts/backend-compat-report-strict.json`
- `artifacts/branch-protection-audit-untrusted.json`
- `artifacts/branch-protection-audit-strict.json`

## 7. Native Rust Transport Runtime

Build and run the native JSON-RPC transport binary:

```bash
cargo run --bin aira-graphdb-native -- --db /path/to/aira-graphdb-native.json
```

`aira-synapse` can force this runtime with:

```bash
export MEMGRAPHRAG_BACKEND=aira-graphdb
export AIRA_GRAPHDB_REPO_PATH=/absolute/path/to/aira-graphdb
```

## 8. Native gate checks (local)

Run native contract/perf/soak checks locally:

```bash
cargo test --test native_rpc_resilience --quiet
cargo test --test native_perf_gate --quiet
AGDB_NATIVE_SOAK_PROFILE=P0-NATIVE-SOAK-SMOKE cargo test --test native_soak_gate --quiet
```

For crash forensics verification (includes forced panic contract case):

```bash
cargo test --test native_rpc_resilience -- --nocapture
```
