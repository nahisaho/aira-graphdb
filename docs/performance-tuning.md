# Performance Tuning and Optimization Guide (English)

## 1. Profiling and measurement

### 1.1 Native benchmark suite

Use the built-in benchmark suite to measure performance:

```bash
cargo test --test native_perf_gate -- --nocapture
```

Output: `artifacts/native-bench-report.json`

Key metrics:
- `latency_p99_ms` - 99th percentile query latency
- `throughput_ops_per_sec` - Operations per second
- `memory_peak_mb` - Peak memory usage during test
- `error_rate` - Percentage of failed requests

### 1.2 Local profiling with flamegraph

Install flamegraph tools:
```bash
cargo install flamegraph
rustup component add llvm-tools-embedded
```

Profile a single operation:
```bash
cargo flamegraph --bin aira-graphdb-native --freq 99 -- --db test.db
```

This generates `flamegraph.svg` showing CPU time distribution.

### 1.3 Memory profiling

Use Valgrind on Linux:
```bash
valgrind --tool=massif --massif-out-file=massif.out \
  ./target/release/aira-graphdb-native --db test.db
ms_print massif.out
```

Or use heaptrack:
```bash
heaptrack ./target/release/aira-graphdb-native --db test.db
heaptrack_gui heaptrack.aira-graphdb-native.*.gz
```

### 1.4 Native benchmark profiles

The native perf gate uses fixed profiles and stores the result in `artifacts/native-bench-report.json`.

| Profile | Workload | Baseline | Pass criteria |
|---|---|---|---|
| `P0-LATENCY-BASELINE` | Simple MATCH/RETURN on 100k nodes | `concurrency=1` | P95 ≤ 50ms |
| `P0-NATIVE-READ` | `get_node/get_adjacent/vector_search(topK=10)/lexical_search(topK=10)` | 100k nodes / 100k vectors / `concurrency=8` / `warmup=60s` | 3 runs median: `get_node<=5ms`, `get_adjacent<=10ms`, `vector_search<=30ms`, `lexical_search<=30ms` |
| `P0-NATIVE-WRITE` | `upsert_nodes/upsert_edges/vector_upsert` | 10,000 writes / batch 100 / `concurrency=8` | 3 runs median: write P95 ≤ 25ms and 10,000 writes ≤ 8,000ms |
| `P0-NATIVE-SOAK-SMOKE` | Read/write mix + anomaly injection | 30 min | crash count = 0 and internal failure rate ≤ 0.1% |

## 2. Bottleneck identification

### Common bottlenecks and solutions:

| Bottleneck | Symptom | Solution |
|-----------|---------|----------|
| **Network latency** | High `latency_p99_ms` with low CPU | Reduce RPC round trips, use batch operations |
| **Lock contention** | High `WRITE_LOCK_CONFLICT` errors | Reduce concurrent writers, use transactions |
| **Disk I/O** | High `IO_FAILURE` rate, slow sync | Move to faster storage, increase batch size |
| **Memory pressure** | Increasing GC pauses | Reduce corpus size, split into shards |
| **Parser overhead** | High CPU with moderate throughput | Use prepared statements (if supported) |

### Profiling checklist:

- [ ] Run `native_perf_gate` to establish baseline
- [ ] Identify slowest operations (see report)
- [ ] Profile with flamegraph or perf
- [ ] Correlate code paths with bottlenecks
- [ ] Apply optimization
- [ ] Re-run benchmark to verify improvement
- [ ] Document trade-offs (memory vs latency vs throughput)

## 3. Optimization strategies

### 3.1 Query optimization

**Avoid**: Large MATCH without WHERE

```cypher
# ❌ Slow - full graph scan
MATCH (n) WHERE n.type='Paper' RETURN n

# ✅ Fast - filter early
MATCH (n:Paper) RETURN n
```

**Batch operations**: Instead of N RPC calls, use 1:

```javascript
// ❌ Slow: 1000 RPC calls
for (let i = 0; i < 1000; i++) {
  await db.upsert_nodes([nodes[i]]);
}

// ✅ Fast: 1 RPC call
await db.upsert_nodes(nodes);
```

**Projection over enumeration**:

```json
{"method":"projection_get_transitions","params":{"corpusId":"c1"}}
```

Instead of retrieving all nodes and edges separately.

### 3.2 Storage optimization

**Checkpoint strategy**:

Use `memory_save_checkpoint` periodically to create recovery points:

```json
{"method":"memory_save_checkpoint","params":{"checkpoint":{"jobId":"job-123","state":{...}}}}
```

Benefits:
- Faster recovery on crash
- Reduces WAL replay time
- Enables incremental backup

**Storage format**:

`aira-graphdb` now uses a compact binary snapshot/WAL format with legacy JSON compatibility. This reduces serialization overhead and disk usage compared with the previous pretty-printed JSON layout.
Legacy JSON files are rewritten to the compact format on load.

**Document lifecycle**:

When deleting old documents, use bulk operation:

```json
{"method":"delete_by_document","params":{"corpusId":"c1","documentId":"doc-old"}}
```

This removes all related graph data in one operation.

### 3.3 Connection pooling

For multiple clients, use connection pooling to reduce handshake overhead:

```javascript
class GraphDbPool {
  constructor(size = 10) {
    this.connections = [];
    for (let i = 0; i < size; i++) {
      this.connections.push(new GraphDbClient());
    }
    this.nextIndex = 0;
  }
  
  getConnection() {
    const conn = this.connections[this.nextIndex];
    this.nextIndex = (this.nextIndex + 1) % this.connections.length;
    return conn;
  }
}
```

### 3.4 Caching strategies

**Vector search result caching**:

Cache embeddings locally to avoid redundant searches:

```javascript
const embeddingCache = new Map();

async function searchWithCache(query, topK) {
  const cacheKey = `${query}-${topK}`;
  if (embeddingCache.has(cacheKey)) {
    return embeddingCache.get(cacheKey);
  }
  
  const results = await db.vector_search({
    corpusId: 'c1',
    queryVector: embedQuery(query),
    topK
  });
  embeddingCache.set(cacheKey, results);
  return results;
}
```

Implement TTL (time-to-live):
```javascript
const cache = new Map();
setInterval(() => {
  const now = Date.now();
  for (const [key, {timestamp}] of cache.entries()) {
    if (now - timestamp > 3600000) { // 1 hour
      cache.delete(key);
    }
  }
}, 300000); // Check every 5 minutes
```

## 4. Scaling strategies

### 4.1 Sharding by corpus

Instead of one large corpus, split into multiple corpora:

```json
// Before: 1 corpus with 10M nodes
{"corpusId":"all","nodes":[...]}

// After: 10 corpora with 1M nodes each
{"corpusId":"corpus-0","nodes":[...]}
{"corpusId":"corpus-1","nodes":[...]}
```

Benefits:
- Parallelizable queries
- Lower per-corpus memory
- Independent recovery

### 4.2 Horizontal scaling

Run multiple sidecar instances behind a load balancer:

```bash
# Instance 1
cargo run --bin aira-graphdb-native -- --db /data/db-1.db

# Instance 2
cargo run --bin aira-graphdb-native -- --db /data/db-2.db
```

Route by corpus hash:

```javascript
function getInstanceForCorpus(corpusId, instanceCount) {
  return hash(corpusId) % instanceCount;
}
```

## 5. Configuration tuning

### 5.1 Environment variables

```bash
# Increase max connections
AGDB_MAX_CONNECTIONS=1000

# Reduce batch timeout
AGDB_BATCH_TIMEOUT_MS=500

# Enable debug logging
AGDB_DEBUG=1
```

### 5.2 Runtime parameters

In `native_bench.rs`, adjust profile parameters:

```rust
pub struct SoakProfile {
    pub duration_minutes: u64,
    pub batch_size: usize,           // Increase for throughput
    pub concurrent_clients: usize,   // Tune for contention
    pub random_seed: u64,
}
```

### 5.3 Native benchmark thresholds

The perf gate also reads these override values when tuning is needed:

```bash
AGDB_NATIVE_BENCH_GET_NODE_P95_MS=5
AGDB_NATIVE_BENCH_GET_ADJ_P95_MS=10
AGDB_NATIVE_BENCH_VECTOR_P95_MS=30
AGDB_NATIVE_BENCH_LEXICAL_P95_MS=30
AGDB_NATIVE_BENCH_WRITE_P95_MS=25
AGDB_NATIVE_BENCH_WRITE_10K_MS=8000
```

## 6. Monitoring optimization impact

### 6.1 Before/After comparison

```bash
# Baseline
cargo test --test native_perf_gate
cp artifacts/native-bench-report.json baseline.json

# Apply optimization

# Re-test
cargo test --test native_perf_gate
cp artifacts/native-bench-report.json optimized.json

# Compare
jq '.latency_p99_ms' baseline.json optimized.json
```

### 6.2 Regression detection

Add this to CI to catch performance regressions:

```yaml
- name: Compare with baseline
  run: |
    jq '.latency_p99_ms' baseline.json > baseline.txt
    jq '.latency_p99_ms' optimized.json > current.txt
    REGRESSION=$(awk 'NR==2{if($1 > baseline*1.1) print 1; else print 0}' baseline.txt current.txt)
    if [ $REGRESSION -eq 1 ]; then exit 1; fi
```

### 6.3 Native benchmark report interpretation

- `rounds` is `3` by default; the report uses the median of three runs
- `thresholds_ms` shows the active perf gate limits
- `metrics_ms` records the measured P95 values
- `gate_pass=false` means the perf gate should block release

## 7. Common trade-offs

| Optimization | Benefit | Cost |
|---|---|---|
| Larger batch size | Higher throughput | Higher memory, higher latency per batch |
| Sharding by corpus | Lower contention | Operational complexity, higher storage |
| Connection pooling | Lower latency | Higher memory |
| Result caching | Reduced queries | Stale data risk, invalidation overhead |
| Indexes on properties | Faster queries | More memory, slower writes |

Choose based on your workload's priorities.
