# エラーハンドリングと再試行戦略ガイド（日本語）

## 1. エラーコード参照

aira-graphdb からの全エラーは、以下の正準エラーコード定義に従います：

```text
spec/contracts/agdb-error-codes.v1.0.0.json
```

### 主要なエラーカテゴリ：

| カテゴリ | 例 | 再試行の要否 |
|----------|----|----|
| **プロトコル** | `PROTOCOL_VERSION_MISMATCH` | いいえ（クライアント/サーバー更新） |
| **クエリ検証** | `UNSUPPORTED_FEATURE`, `INVALID_ARGUMENT`, `INVALID_TOP_K`, `INVALID_THRESHOLD`, `INVALID_CORPUS_ID`, `INVALID_NAMESPACE` | いいえ（要求を修正） |
| **トランザクション競合** | `RETRYABLE_CONFLICT`, `WRITE_LOCK_CONFLICT` | はい（backoff付き） |
| **ストレージ** | `INCOMPATIBLE_FORMAT` | いいえ（DB 修復/移行） |
| **認証** | `AUTH_REQUIRED`, `AUTH_FAILED` | いいえ（再認証） |
| **整合性** | `REFERENTIAL_INTEGRITY_VIOLATION` | いいえ（データを調査） |

## 2. 再試行戦略

### 指数バックオフパターン：

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

### Python の例：

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

再試行は副作用がない操作に限定してください。副作用を伴う要求は、ワークフロー全体ではなくトランザクション境界で再試行するのが安全です。

## 3. 一般的なエラーシナリオと復旧

### 3.1 プロトコル不一致

**症状**: ハンドシェイク時に `PROTOCOL_VERSION_MISMATCH` が発生。

**復旧手順**:
1. クライアントとサーバーが同じ契約バージョンを使っているか確認
2. 契約更新後はクライアント SDK を再生成または更新
3. リクエストを再送する前にハンドシェイクログを確認

### 3.2 トランザクション競合と書込ロック

**症状**: 同一データへの並行更新で `RETRYABLE_CONFLICT` または `WRITE_LOCK_CONFLICT` が発生。

**復旧手順**:
1. これは並行書込や古いトランザクション状態で想定される動作です
2. 再試行するのは冪等な操作に限定し、指数バックオフとジッターを使う
3. 書込トランザクションを短く保ち、ホットスポットでは同時ライター数を減らす
4. 同じ要求が繰り返し失敗する場合はトランザクション範囲とロック所有者を確認

### 3.3 バリデーション失敗

**症状**: `INVALID_ARGUMENT`, `INVALID_TOP_K`, `INVALID_THRESHOLD`, `INVALID_CORPUS_ID`, `INVALID_NAMESPACE`。

**復旧手順**:
1. 要求ペイロードを修正して再送する（これらは再試行対象外）
2. API 呼び出し前に `topK`, `threshold`, `corpusId`, `namespace` を検証
3. SDK 側で事前検証し、サーバー到達前に失敗させる

### 3.4 ストレージ形式不整合

**症状**: DB ファイルの読み込み/復旧時に `INCOMPATIBLE_FORMAT` が発生。

**復旧手順**:
1. DB ファイルのバージョンと現在のランタイムバージョンを確認
2. 互換スナップショットへ戻すか、ファイル形式を移行
3. DB 状態を変えずに同じ open を再試行しない

### 3.5 データ整合性違反

**症状**: 連結データの削除/更新で `REFERENTIAL_INTEGRITY_VIOLATION` が発生。

**復旧手順**:
1. 依存するエッジを先に削除または更新
2. ワークフローが許す場合は detach-style の削除操作を使用
3. 再試行前にデータモデルを確認

### 3.6 認証失敗

**症状**: リクエスト実行時に `AUTH_REQUIRED` または `AUTH_FAILED` が発生。

**復旧手順**:
1. 再試行前に再認証
2. トークンの署名、claims、時計ずれを確認
3. auth ハンドシェイク成功後に要求が送られているか確認

## 4. 監視とアラート

### 4.1 監査ログの閾値

`<db>.native-audit.log` 内で以下イベントを監視：

| イベント | 閾値 | アクション |
|----------|------|----------|
| `PROCESS_CRASH` | 任意の発生 | 即座に調査 |
| `AUTH_FAILED` | 1分間に >5 件 | JWT/TLS 問題確認 |
| `RETRYABLE_CONFLICT` | 1分間に >20 件 | トランザクションのホットスポット確認 |
| `WRITE_LOCK_CONFLICT` | 1分間に >10 件 | ライター競合を確認 |
| `INCOMPATIBLE_FORMAT` | 任意の発生 | ファイルバージョンを確認 / バックアップから復元 |

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

### エラーコンテキスト抽出（TypeScript）:

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

## 6. エラーハンドリングテスト

### ユニットテスト例（Rust）:

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
