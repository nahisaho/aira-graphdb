# aira-graphdb

Rust-based GraphDB for AIRA ecosystem services, designed to support both:

- **Embedded mode** (file-based, SQLite/LadybugDB-like)
- **Server mode** (daemon/TCP, Neo4j-like)

This repository follows SDD artifacts and currently includes Phase 1–4 implementation, including native Rust transport performance and stability gates.

## Current status

- Requirements: `spec/REQ-AIRA-GRAPHDB-001.md`
- Design: `spec/DES-AIRA-GRAPHDB-001.md`
- ADR: `spec/ADR-AGDB-001.md`
- Task breakdown: `spec/PLAN-AIRA-GRAPHDB-001.md`
- Immutable contracts:
  - `spec/contracts/agdb-typemap-p0.v1.0.0.json`
  - `spec/contracts/agdb-cypher-p0-grammar.v1.0.0.json`
  - `spec/contracts/agdb-error-codes.v1.0.0.json`

## Repository layout

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

## Prerequisites

- Rust (stable, with `cargo`)
- Node.js (for Node SDK tests)
- Python 3.10+ (for Python SDK tests)

## Build and test

Run Rust tests:

```bash
cargo test
```

Run native transport contract/perf/soak tests:

```bash
cargo test --test native_rpc_resilience
cargo test --test native_perf_gate
cargo test --test native_soak_gate
```

Run Node SDK tests:

```bash
cd sdk/node
npm test
```

Run Python SDK tests:

```bash
cd sdk/python
PYTHONPATH=. python -m unittest discover -s tests -v
```

## Implemented capabilities

- Contract loading for type mapping / Cypher P0 grammar / error codes
- Centralized error registry and code mapping
- In-memory graph domain model (node/edge CRUD)
- Storage snapshot + WAL-based persistence/recovery
- Transaction manager (begin/commit/rollback)
- File write lock guard (`WRITE_LOCK_CONFLICT`)
- Minimal query execution for P0 subset surface
- Protocol handshake negotiation
- Auth boundary validation logic (TLS/JWT policy checks)
- Embedded and server runtime scaffolding
- Native benchmark and soak profile helpers
- Native request anomaly audit logging (`<db>.native-audit.log`)
- Native runtime crash auto logging (`PROCESS_CRASH`)
- Native CI quality gates:
  - perf gate artifact: `artifacts/native-bench-report.json`
  - soak gate artifact: `artifacts/native-soak-report.json`
  - audit artifact: `artifacts/native-audit-events.json`

## CI gate policy (native transport)

- `pull_request`: `P0-NATIVE-SOAK-SMOKE` (30 min profile contract)
- `schedule` / `release`: `P0-NATIVE-SOAK` (24h profile contract)
- Mandatory thresholds:
  - `crashCount == 0`
  - `internalFailureRate <= 0.001`
  - required audit fields present for native request anomaly events

## Native crash forensics

When the native sidecar panics or exits abnormally, `<db>.native-audit.log` receives a `PROCESS_CRASH` event with:

- `errorCode`
- `timestamp`
- `processExitCode`
- `signal`
- `lastRequestId`
- `uptimeSec`
- `cause` (when available)
