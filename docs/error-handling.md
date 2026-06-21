# Error Handling and Retry Strategy Guide (English)

## 1. Error Code Reference

All errors from aira-graphdb conform to the canonical error codes defined in:

```text
spec/contracts/agdb-error-codes.v1.0.0.json
```

### Key error categories:

| Category | Examples | When to Retry |
|----------|----------|---|
| **Protocol** | `PROTOCOL_VERSION_MISMATCH` | No (update client/server) |
| **Query Validation** | `UNSUPPORTED_FEATURE`, `INVALID_ARGUMENT`, `INVALID_TOP_K`, `INVALID_THRESHOLD`, `INVALID_CORPUS_ID`, `INVALID_NAMESPACE` | No (fix request) |
| **Transaction Contention** | `RETRYABLE_CONFLICT`, `WRITE_LOCK_CONFLICT` | Yes (with backoff) |
| **Storage** | `INCOMPATIBLE_FORMAT` | No (repair/migrate database) |
| **Auth** | `AUTH_REQUIRED`, `AUTH_FAILED` | No (authenticate again) |
| **Integrity** | `REFERENTIAL_INTEGRITY_VIOLATION` | No (investigate data) |

## 2. Retry Strategy

### Exponential backoff pattern:

```javascript
async function executeWithRetry(fn, maxRetries = 3) {
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    try {
      return await fn();
    } catch (error) {
      const code = error?.code;
      const isRetryable = isRetryableError(code);
      if (!isRetryable || attempt === maxRetries - 1) throw error;

      const delayMs = Math.min(250 * Math.pow(2, attempt), 5000) + Math.floor(Math.random() * 100);
      await sleep(delayMs);
    }
  }
}

function isRetryableError(code) {
  return ['RETRYABLE_CONFLICT', 'WRITE_LOCK_CONFLICT'].includes(code);
}

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}
```

### Python equivalent:

```python
import asyncio
import random

async def execute_with_retry(fn, max_retries=3):
    for attempt in range(max_retries):
        try:
            return await fn()
        except GraphDbError as error:
            code = getattr(error, "code", None)
            if not is_retryable(code) or attempt == max_retries - 1:
                raise
            delay_ms = min(250 * (2 ** attempt) + random.randint(0, 100), 5000)
            await asyncio.sleep(delay_ms / 1000)

def is_retryable(code):
    return code in ['RETRYABLE_CONFLICT', 'WRITE_LOCK_CONFLICT']
```

Retry only idempotent operations. If the request creates side effects, prefer transaction retries at the boundary instead of repeating the whole workflow.

## 3. Common error scenarios and recovery

### 3.1 Protocol mismatches

**Symptom**: `PROTOCOL_VERSION_MISMATCH` during handshake.

**Recovery**:
1. Verify client and server use the same contract version
2. Regenerate or refresh the client SDK after a contract update
3. Review handshake logs before retrying the request

### 3.2 Transaction conflicts and write locks

**Symptom**: `RETRYABLE_CONFLICT` or `WRITE_LOCK_CONFLICT` when multiple writers touch the same data.

**Recovery**:
1. This is expected under concurrent writes or stale transaction state
2. Retry only idempotent operations with exponential backoff and jitter
3. Keep write transactions short and reduce concurrent writers when hot spots appear
4. If the same request keeps failing, inspect transaction scope and lock ownership

### 3.3 Validation failures

**Symptom**: `INVALID_ARGUMENT`, `INVALID_TOP_K`, `INVALID_THRESHOLD`, `INVALID_CORPUS_ID`, or `INVALID_NAMESPACE`.

**Recovery**:
1. Fix the request payload and re-send it; these codes are not retryable
2. Validate `topK`, `threshold`, `corpusId`, and `namespace` before calling the API
3. Use SDK-side validation to fail fast before the request reaches the server

### 3.4 Storage format incompatibility

**Symptom**: `INCOMPATIBLE_FORMAT` when opening or recovering a database file.

**Recovery**:
1. Check the database file version and current runtime version
2. Restore from a compatible snapshot or migrate the file format
3. Do not retry the same file open without changing the database state

### 3.5 Data integrity violations

**Symptom**: `REFERENTIAL_INTEGRITY_VIOLATION` when deleting or updating connected data.

**Recovery**:
1. Delete or update dependent edges first
2. Use detach-style delete operations when the workflow allows it
3. Review the data model before retrying the request

### 3.6 Authentication failures

**Symptom**: `AUTH_REQUIRED` or `AUTH_FAILED` on request execution.

**Recovery**:
1. Re-authenticate before retrying the request
2. Check token signature, claims, and clock skew
3. Confirm the request is sent after the auth handshake succeeds

## 4. Monitoring and alerting

### 4.1 Audit log thresholds

Monitor these events in `<db>.native-audit.log`:

| Event | Threshold | Action |
|-------|-----------|--------|
| `PROCESS_CRASH` | Any occurrence | Immediate investigation |
| `AUTH_FAILED` | >5 per minute | Check JWT/TLS issues |
| `RETRYABLE_CONFLICT` | >20 per minute | Review transaction hot spots |
| `WRITE_LOCK_CONFLICT` | >10 per minute | Review writer contention |
| `INCOMPATIBLE_FORMAT` | Any occurrence | Verify file version / restore backup |

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
    "code": "RETRYABLE_CONFLICT",
    "message": "Transaction conflict detected",
    "details": {
      "context": "corpus_id=corpus-1, nodeId=n-42",
      "transaction_id": "tx-456"
    },
    "suggestions": [
      "Retry with exponential backoff",
      "Reduce concurrent writers"
    ]
  }
}
```

### Error context extraction (TypeScript):

```typescript
function extractErrorContext(error: { code: string; message: string; details?: Record<string, string> }) {
  return {
    code: error.code,
    message: error.message,
    isRetryable: isRetryableError(error.code),
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
            Err(GraphDbError::new(ErrorCode::RetryableConflict, "conflict"))
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
        Err(GraphDbError::new(ErrorCode::InvalidTopK, "invalid topK"))
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
