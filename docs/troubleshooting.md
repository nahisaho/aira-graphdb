# Troubleshooting and Debugging Guide (English)

## 1. Log interpretation

### 1.1 Audit log format

All events in `<db>.native-audit.log` follow this JSON structure:

```json
{
  "type": "AUTH_FAILED",
  "timestamp": "2026-06-21T12:30:45.123Z",
  "requestId": "req-abc123",
  "errorCode": "AUTH_REQUIRED",
  "details": {
    "attemptedToken": "eyJ...",
    "reason": "JWT signature invalid"
  }
}
```

### 1.2 Event type classification

| Type | Meaning | Action |
|------|---------|--------|
| `AUTH_FAILED` | JWT/TLS auth rejected | Check credentials, clock sync |
| `AUTH_REQUIRED_REJECTED` | Request before auth complete | Normal on startup, check protocol order |
| `ROLLBACK_EXECUTED` | Transaction rolled back | Check transaction logs, retry if transient |
| `REFERENTIAL_INTEGRITY_VIOLATION` | Foreign key violation | Fix data relationships |
| `DETERMINISTIC_CONFLICT` | Conflict detected in write | Investigate concurrent updates |
| `PROCESS_CRASH` | Sidecar exited abnormally | Check cause, restart |
| `IO_FAILURE` | Read/write error | Check disk space, permissions |
| `WRITE_LOCK_CONFLICT` | Write lock contention | Review concurrent operations |

### 1.3 Filtering and searching logs

```bash
# Count events by type
cat <db>.native-audit.log | jq -r '.type' | sort | uniq -c

# Find errors in timerange
jq 'select(.timestamp > "2026-06-21T12:00:00" and .timestamp < "2026-06-21T13:00:00")' \
  <db>.native-audit.log

# Extract specific request
jq 'select(.requestId == "req-abc123")' <db>.native-audit.log

# Find all crashes with context
jq 'select(.type == "PROCESS_CRASH") | {timestamp, signal, cause, lastRequestId, uptimeSec}' \
  <db>.native-audit.log

# Alert on high error rate
ERRORS=$(jq 'select(.type | startswith("ERROR")) | .type' <db>.native-audit.log | wc -l)
if [ $ERRORS -gt 10 ]; then echo "High error rate detected"; fi
```

## 2. Common issues and solutions

### 2.1 Issue: "Connection refused" when connecting to sidecar

**Symptoms**:
```
Error: ECONNREFUSED - connect ECONNREFUSED 127.0.0.1:7687
```

**Diagnosis**:
```bash
# Check if sidecar is running
ps aux | grep aira-graphdb-native

# Check if port is listening
netstat -tlnp | grep 7687

# Check logs for startup errors
cat aira-graphdb-native.log | tail -50
```

**Solutions**:
1. Start sidecar: `cargo run --bin aira-graphdb-native -- --db /path/to/db.db`
2. Verify port is not in use: `lsof -i :7687`
3. Check firewall: `sudo ufw status` (Linux)
4. If crashed, check `<db>.native-audit.log` for `PROCESS_CRASH`

### 2.2 Issue: "WRITE_LOCK_CONFLICT" on every write

**Symptoms**:
```
All write operations fail with WRITE_LOCK_CONFLICT immediately
```

**Diagnosis**:
```bash
# Check for stuck transactions
grep WRITE_LOCK_CONFLICT <db>.native-audit.log | wc -l

# See if lock holder info is logged
grep WRITE_LOCK_CONFLICT <db>.native-audit.log | jq '.details'
```

**Solutions**:
1. Check if another client is holding write lock: review concurrent clients
2. Restart the sidecar to clear stuck locks, then relaunch it with the standard startup command
3. Reduce concurrent writers (expected at high concurrency)
4. Split corpus if contention is inherent to workload

### 2.3 Issue: Queries return empty results unexpectedly

**Symptoms**:
```
Vector search returns [] instead of results
Lexical search returns [] for known documents
```

**Diagnosis**:
```bash
# Verify data was indexed
curl -X POST localhost:7687 -d '{"method":"get_nodes","params":{"corpusId":"c1"}}'

# Check document existence
jq 'select(.method == "vector_upsert")' <db>.native-audit.log | head

# Verify namespace/query
grep vector_search <db>.native-audit.log | jq '.params'
```

**Solutions**:
1. Confirm data was inserted: call `get_nodes` before search
2. Check namespace in search matches namespace in upsert
3. Verify topK is not too small for result set
4. Confirm similarity threshold (if specified) is not too high

### 2.4 Issue: Memory usage grows unbounded

**Symptoms**:
```
Memory increases linearly with requests
Process eventually OOM kills
```

**Diagnosis**:
```bash
# Monitor memory over time
while true; do
  ps aux | grep aira-graphdb-native | grep -v grep | awk '{print $6}' && sleep 1
done

# Check for memory leaks in audit logs
grep OUT_OF_MEMORY <db>.native-audit.log | head

# Profile heap
valgrind --tool=massif ./target/release/aira-graphdb-native --db test.db
```

**Solutions**:
1. Reduce batch size: `upsert_nodes([100])` instead of `upsert_nodes([10000])`
2. Enable checkpointing: save memory snapshots periodically
3. Delete old data regularly: call `delete_by_document` for stale data
4. Split corpus if data set is too large for single process

### 2.5 Issue: Sidecar crashes randomly

**Symptoms**:
```
PROCESS_CRASH in audit log with no client error
Connection closes unexpectedly
```

**Diagnosis**:
```bash
# Check crash logs
grep PROCESS_CRASH <db>.native-audit.log | jq '.{timestamp, signal, cause, lastRequestId}'

# Check recent requests before crash
grep lastRequestId <db>.native-audit.log | tail -5

# Get system logs
dmesg | tail -20  # Check for OOM killer
journalctl -xe   # System event log
```

**Solutions**:
1. If signal is SIGKILL: check system memory (`free -h`), increase RAM or reduce batch size
2. If signal is SIGSEGV: file bug report with crash dump
3. If cause is "panic": check Rust backtrace, file issue with details
4. Enable core dumps for debugging:
   ```bash
   ulimit -c unlimited
   cargo run --bin aira-graphdb-native -- --db test.db
   ```

## 3. Debug mode

### 3.1 Enable debug logging

```bash
# Set debug environment variable
export AGDB_DEBUG=1
export RUST_LOG=debug

# Run sidecar with debug output
cargo run --bin aira-graphdb-native -- --db test.db 2>&1 | tee debug.log
```

Debug output includes:
- Query parsing steps
- Cache hits/misses
- Lock acquisition/release
- Transaction state transitions

### 3.2 Minimal reproducible test

Create a test that reproduces the issue:

```javascript
async function testReproduction() {
  const client = new GraphDbClient();
  
  // Step 1: Setup
  await client.handshake();
  await client.auth(token);
  
  // Step 2: Reproduce issue
  await client.upsert_nodes([...]);
  
  // Step 3: Verify problem
  const result = await client.get_nodes({corpusId:'c1'});
  assert(result.length > 0, "Expected nodes, got empty");
}

testReproduction().catch(err => {
  console.error("Reproduction failed:", err);
  process.exit(1);
});
```

Save this to `reproduce.js` and run:
```bash
node reproduce.js 2>&1 | tee reproduction.log
```

Attach both `reproduction.log` and `<db>.native-audit.log` to bug report.

## 4. Performance debugging

### 4.1 Identify slow queries

```bash
# Extract query timing from logs
jq 'select(.type == "QUERY_EXECUTED") | {query: .details.query, duration_ms: .details.duration_ms}' \
  <db>.native-audit.log | sort -k 3 -n | tail -10
```

### 4.2 Lock contention investigation

```bash
# Find requests that hit WRITE_LOCK_CONFLICT
jq 'select(.type == "WRITE_LOCK_CONFLICT") | .details.context' <db>.native-audit.log | sort | uniq -c

# Identify which corpus has most contention
jq 'select(.type == "WRITE_LOCK_CONFLICT") | .details.context' <db>.native-audit.log | \
  grep -o "corpus_id=[^,]*" | sort | uniq -c
```

## 5. Data integrity checks

### 5.1 Verify graph consistency

```json
{"method":"memory_validate_integrity","params":{"corpusId":"c1"}}
```

Returns array of inconsistencies found (empty = valid).

### 5.2 Compare data snapshots

```json
// Save current state
{"method":"memory_save","params":{"snapshot":{"corpusId":"c1",...}}}

// Later, load and compare
{"method":"memory_load","params":{"corpusId":"c1"}}
```

### 5.3 Check referential integrity

```bash
# Get all edges
jq '.[] | select(.method == "get_edges") | .result' <audit>

# Verify each edge's source/target nodes exist
jq '.[] | select(.method == "get_nodes") | .result | map(.nodeId)' <audit>
```

## 6. Collecting diagnostics for bug reports

When filing a bug, include:

1. **Environment**:
   - OS and version
   - Rust version (`rustc --version`)
   - Node/Python SDK versions
   - System memory and disk space

2. **Reproduction steps**:
   - Minimal code to reproduce
   - Input data (if shareable)
   - Expected vs actual behavior

3. **Logs**:
   - Complete `<db>.native-audit.log`
   - `aira-graphdb-native` stderr/stdout
   - Any crash dumps or core files

4. **Metrics**:
   - `artifacts/native-bench-report.json` (performance baseline)
   - Memory/CPU graphs if available
   - Timing of issue occurrence

5. **Configuration**:
   - Sidecar startup command
   - Environment variables set
   - Corpus size and client count

Sanitize logs to remove sensitive data before sharing.
