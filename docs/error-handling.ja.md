# エラーハンドリングと再試行戦略ガイド（日本語）

## 1. エラーコード参照

aira-graphdb からの全エラーは、以下の正準エラーコード定義に従います：

```text
spec/contracts/agdb-error-codes.v1.0.0.json
```

### 主要なエラーカテゴリ：

| カテゴリ | 例 | 再試行の要否 |
|----------|----|----|
| **クライアント入力** | `INVALID_PARAMETER`, `PROTOCOL_VERSION_MISMATCH` | いいえ（入力を修正） |
| **一時的障害** | `IO_FAILURE`, `TIMEOUT`, `LOCK_CONFLICT` | はい（backoff付き） |
| **リソース枯渇** | `OUT_OF_MEMORY`, `STORAGE_QUOTA_EXCEEDED` | コンテキスト次第 |
| **データ整合性** | `REFERENTIAL_INTEGRITY_VIOLATION`, `DETERMINISTIC_CONFLICT` | いいえ（データを調査） |
| **内部エラー** | `INTERNAL_BUG`, `PANIC_DETECTED` | エスカレーション（報告） |

## 2. 再試行戦略

### 指数バックオフパターン：

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

### Python の例：

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

## 3. 一般的なエラーシナリオと復旧

### 3.1 接続失敗

**症状**: native sidecar 接続時に繰り返し `IO_FAILURE` または `TIMEOUT` が発生。

**復旧手順**:
1. sidecar プロセス確認: `ps aux | grep aira-graphdb-native`
2. sidecar リッスン確認: `netstat -tlnp | grep 7687`（TCP利用の場合）
3. sidecar 起動ログ確認: `aira-graphdb-native.log` または stderr
4. クラッシュ検出時: `<db>.native-audit.log` の `PROCESS_CRASH` エントリを確認
5. 破損が疑われる場合: `rm <db>.json && restart` で新規 DB から再スタート

### 3.2 トランザクション競合

**症状**: 複数クライアントが同じコーパスを修正する際に `LOCK_CONFLICT` が発生。

**復旧手順**:
1. これは同一コーパスへの並行書込に対する期待動作です
2. クライアント側 backoff を実装（前述の指数バックオフパターンを参照）
3. 重要データの場合: `BEGIN_TX` → 操作 → `COMMIT_TX` で明示的トランザクション実行
4. 並行クライアント数を削減または master-worker パターン採用を検討

### 3.3 メモリ不足エラー

**症状**: 大規模グラフ処理時に `OUT_OF_MEMORY` が発生。

**復旧手順**:
1. 利用可能メモリ確認: `free -h`（Linux）または `vm_stat`（macOS）
2. sidecar メモリ使用量監視: `ps -aux | grep aira-graphdb-native`
3. バッチサイズ削減: `upsert_nodes([1000 nodes])` の代わり、ループで `upsert_nodes([100 nodes])` を実行
4. システムメモリ増設またはデータを小規模コーパスに分割
5. コンテナ実行の場合: Docker メモリ制限を確認

### 3.4 データ整合性違反

**症状**: ノード削除時に受信エッジがある場合、`REFERENTIAL_INTEGRITY_VIOLATION` が発生。

**復旧手順**:
1. Cypher で `DELETE DETACH` を使用（関連エッジ自動削除）
2. native RPC 利用時: 先に `delete_edges`（参照エッジ）、後に `delete_nodes`
3. 監査ログで状況確認: `grep REFERENTIAL_INTEGRITY_VIOLATION <db>.audit.log`

### 3.5 プロトコルミスマッチ

**症状**: クライアント接続時に `PROTOCOL_VERSION_MISMATCH` または `CANONICAL_TYPE_MISMATCH` が発生。

**復旧手順**:
1. クライアント SDK とsidecar のバージョン確認（両者とも v0.1.1 以上）
2. ハンドシェイク交換をログ確認（`AGDB_DEBUG=1` 有効化可能な場合）
3. クライアントと sidecar を同じバージョンにアップグレード
4. クライアント内のキャッシュ型マップ消去

## 4. 監視とアラート

### 4.1 監査ログの閾値

`<db>.native-audit.log` 内で以下イベントを監視：

| イベント | 閾値 | アクション |
|----------|------|----------|
| `PROCESS_CRASH` | 任意の発生 | 即座に調査 |
| `AUTH_FAILED` | 1分間に >5 件 | JWT/TLS 問題確認 |
| `IO_FAILURE` | 1分間に >10 件 | ディスク/ネットワーク確認 |
| `LOCK_CONFLICT` | 1分間に >50 件 | 並行パターンを見直し |
| `DETERMINISTIC_CONFLICT` | 1分間に >20 件 | データ競合を調査 |

### 4.2 監視クエリ例（jq利用）:

```bash
# イベント種別ごとのカウント
grep "^\{" <db>.native-audit.log | jq -r '.type' | sort | uniq -c

# 最近のエラー検出
tail -100 <db>.native-audit.log | jq '.[] | select(.timestamp > "2026-06-21T00:00:00")'

# クラッシュアラート
grep PROCESS_CRASH <db>.native-audit.log | jq '.[] | {timestamp, signal, lastRequestId, uptimeSec, cause}'
```

## 5. レスポンス内のエラーコンテキスト

### 標準エラーレスポンス形式：

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

### エラーコンテキスト抽出（TypeScript）:

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

## 6. エラーハンドリングテスト

### ユニットテスト例（Rust）:

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

## 7. エスカレーション手順

再試行と標準復旧後もエラーが続く場合：

1. **診断情報の収集**:
   - `<db>.native-audit.log` の最新エントリ
   - sidecar プロセス情報およびリソース使用状況
   - 完全なエラーメッセージ（コンテキストと提案を含む）

2. **Issue 作成**（aira-graphdb リポジトリ）:
   - エラーコード・タイムスタンプ
   - 再現手順
   - クライアントコードとサーバーログ
   - システムリソース状況

3. **サポートに連絡**: Issue 参照とタイムラインを提供
