# Error Handling and Retry Strategy Guide (English)

## 1. Error Code Reference

All errors from aira-graphdb conform to the canonical error codes defined in:

```text
spec/contracts/agdb-error-codes.v1.0.0.json
```

### Key error categories:

| Category | Examples | When to Retry |
|----------|----------|---|
| **Client Input** | `INVALID_PARAMETER`, `PROTOCOL_VERSION_MISMATCH` | No (fix input) |
| **Transient Failures** | `IO_FAILURE`, `TIMEOUT`, `LOCK_CONFLICT` | Yes (with backoff) |
| **Resource Exhaustion** | `OUT_OF_MEMORY`, `STORAGE_QUOTA_EXCEEDED` | Depends on context |
| **Data Integrity** | `REFERENTIAL_INTEGRITY_VIOLATION`, `DETERMINISTIC_CONFLICT` | No (investigate data) |
| **Internal Errors** | `INTERNAL_BUG`, `PANIC_DETECTED` | Escalate (report issue) |

## 2. Retry Strategy

### Exponential backoff pattern:

```javascript
async function executeWithRetry(fn, maxRetries = 3) {
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    try {
      return await fn();
    } catch (error) {
      const isRetryable = isRetryableError(error);
      if (!isRetryable || attempt === maxRetries - 1) throw error;
      
      const delayMs = Math.min(1000 * Math.pow(2, attempt), 30000);
      await sleep(delayMs);
    }
  }
}

function isRetryableError(error) {
  const code = error.errorCode;
  return ['IO_FAILURE', 'TIMEOUT', 'LOCK_CONFLICT'].includes(code);
}

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}
```

### Python equivalent:

```python
import time
import random

async def execute_with_retry(fn, max_retries=3):
    for attempt in range(max_retries):
        try:
            return await fn()
        except GraphDbError as error:
            if not is_retryable(error) or attempt == max_retries - 1:
                raise
            delay_ms = min(1000 * (2 ** attempt) + random.randint(0, 100), 30000)
            await asyncio.sleep(delay_ms / 1000)

def is_retryable(error):
    return error.error_code in ['IO_FAILURE', 'TIMEOUT', 'LOCK_CONFLICT']
```

## 3. Common error scenarios and recovery

### 3.1 Connection failures

**Symptom**: Repeated `IO_FAILURE` or `TIMEOUT` when connecting to native sidecar.

**Recovery**:
1. Check sidecar process: `ps aux | grep aira-graphdb-native`
2. Verify sidecar is listening: `netstat -tlnp | grep 7687` (if using TCP)
3. Review sidecar startup logs: check `aira-graphdb-native.log` or stderr
4. If crash detected: check `<db>.native-audit.log` for `PROCESS_CRASH` entries
5. Restart sidecar with fresh database if corruption suspected: `rm <db>.json && restart`

### 3.2 Transaction conflicts

**Symptom**: `LOCK_CONFLICT` when multiple clients modify the same corpus.

**Recovery**:
1. This is expected behavior for concurrent writes to the same corpus
2. Implement client-side backoff (see exponential backoff pattern above)
3. For critical data: use transactions with explicit `BEGIN_TX` → operations → `COMMIT_TX`
4. Consider reducing concurrent client count or using master-worker pattern

### 3.3 Out-of-memory errors

**Symptom**: `OUT_OF_MEMORY` when processing large graphs.

**Recovery**:
1. Check available system memory: `free -h` (Linux) or `vm_stat` (macOS)
2. Monitor sidecar memory usage: `ps -aux | grep aira-graphdb-native`
3. Reduce batch size: instead of `upsert_nodes([1000 nodes])`, use `upsert_nodes([100 nodes])` in a loop
4. Increase system memory or split data into smaller corpora
5. Confirm heap limits if running in container: check Docker memory constraints

### 3.4 Data integrity violations

**Symptom**: `REFERENTIAL_INTEGRITY_VIOLATION` when deleting a node with incoming edges.

**Recovery**:
1. Use `DELETE DETACH` in Cypher to automatically delete related edges
2. When using native RPC: call `delete_edges` first (for edges referencing the node), then `delete_nodes`
3. Review audit logs for context: `grep REFERENTIAL_INTEGRITY_VIOLATION <db>.audit.log`

### 3.5 Protocol mismatches

**Symptom**: `PROTOCOL_VERSION_MISMATCH` or `CANONICAL_TYPE_MISMATCH` on client connection.

**Recovery**:
1. Verify client SDK version matches sidecar version (both should be v0.1.1+)
2. Check handshake exchange in logs (add `AGDB_DEBUG=1` if available)
3. Upgrade both client and sidecar to same version
4. Clear any cached type mappings in client

## 4. Monitoring and alerting

### 4.1 Audit log thresholds

Monitor these events in `<db>.native-audit.log`:

| Event | Threshold | Action |
|-------|-----------|--------|
| `PROCESS_CRASH` | Any occurrence | Immediate investigation |
| `AUTH_FAILED` | >5 per minute | Check JWT/TLS issues |
| `IO_FAILURE` | >10 per minute | Check disk/network |
| `LOCK_CONFLICT` | >50 per minute | Review concurrency pattern |
| `DETERMINISTIC_CONFLICT` | >20 per minute | Investigate data conflicts |

### 4.2 Sample monitoring query (using jq):

```bash
# Count events by type
grep "^\{" <db>.native-audit.log | jq -r '.type' | sort | uniq -c

# Find recent errors
tail -100 <db>.native-audit.log | jq '.[] | select(.timestamp > "2026-06-21T00:00:00")'

# Alert on crashes
grep PROCESS_CRASH <db>.native-audit.log | jq '.[] | {timestamp, signal, lastRequestId, uptimeSec, cause}'
```

## 5. Error context in responses

### Standard error response format:

```json
{
  "id": "req-123",
  "error": {
    "code": "LOCK_CONFLICT",
    "message": "Write lock held by another transaction",
    "details": {
      "context": "corpus_id=corpus-1, nodeId=n-42",
      "duration_ms": 5000,
      "transaction_id": "tx-456"
    },
    "suggestions": [
      "Retry with exponential backoff",
      "Check transaction isolation levels"
    ]
  }
}
```

### Error context extraction (TypeScript):

```typescript
function extractErrorContext(error: GraphDbError) {
  return {
    code: error.code,
    message: error.message,
    isRetryable: isRetryableError(error),
    suggestedAction: getSuggestedAction(error.code),
    timestamp: new Date().toISOString(),
    context: error.details?.context
  };
}
```

## 6. Testing error handling

### Unit test example (Rust):

```rust
#[tokio::test]
async fn test_retry_on_lock_conflict() {
    let mut attempt_count = 0;
    let result = execute_with_retry(|| {
        attempt_count += 1;
        if attempt_count < 2 {
            Err(GraphDbError::LockConflict)
        } else {
            Ok(())
        }
    }, 3).await;
    assert_eq!(attempt_count, 2);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_no_retry_on_client_error() {
    let result = execute_with_retry(|| {
        Err(GraphDbError::InvalidParameter)
    }, 3).await;
    assert!(result.is_err());
}
```

## 7. Escalation path

If errors persist after retry and standard recovery:

1. **Collect diagnostics**:
   - All recent entries in `<db>.native-audit.log`
   - Sidecar process info and resource usage
   - Full error message including context and suggestions

2. **Create issue** in aira-graphdb repository with:
   - Error code and timestamp
   - Steps to reproduce
   - Client code and server logs
   - System resource status

3. **Contact support** with issue reference and timeline
