# トラブルシューティングとデバッグガイド（日本語）

## 1. ログの解釈

### 1.1 監査ログの形式

`<db>.native-audit.log` の全イベントは、以下の JSON 構造に従います：

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

### 1.2 イベントタイプの分類

| タイプ | 意味 | アクション |
|-------|------|----------|
| `AUTH_FAILED` | JWT/TLS 認証拒否 | 認証情報確認、時刻同期確認 |
| `AUTH_REQUIRED_REJECTED` | 認証前にリクエスト受信 | 通常は起動時、プロトコル順序確認 |
| `ROLLBACK_EXECUTED` | トランザクションロールバック | トランザクションログ確認、一時的なら再試行 |
| `REFERENTIAL_INTEGRITY_VIOLATION` | 外部キー違反 | データ関連性を修正 |
| `DETERMINISTIC_CONFLICT` | 書込競合検出 | 並行更新を調査 |
| `PROCESS_CRASH` | Sidecar 異常終了 | 原因確認、再起動 |
| `IO_FAILURE` | 読み書きエラー | ディスク空き、パーミッション確認 |
| `WRITE_LOCK_CONFLICT` | 書込ロック競合 | 並行操作をレビュー |

### 1.3 ログのフィルタリングと検索

```bash
# イベントタイプ別のカウント
cat <db>.native-audit.log | jq -r '.type' | sort | uniq -c

# 時間範囲でのエラー検索
jq 'select(.timestamp > "2026-06-21T12:00:00" and .timestamp < "2026-06-21T13:00:00")' \
  <db>.native-audit.log

# 特定リクエスト抽出
jq 'select(.requestId == "req-abc123")' <db>.native-audit.log

# クラッシュログとコンテキスト
jq 'select(.type == "PROCESS_CRASH") | {timestamp, signal, cause, lastRequestId, uptimeSec}' \
  <db>.native-audit.log

# 高いエラー率アラート
ERRORS=$(jq 'select(.type | startswith("ERROR")) | .type' <db>.native-audit.log | wc -l)
if [ $ERRORS -gt 10 ]; then echo "エラー率が高い"; fi
```

## 2. 一般的な問題と対応策

### 2.1 問題: sidecar への接続が拒否される

**症状**:
```
Error: ECONNREFUSED - connect ECONNREFUSED 127.0.0.1:7687
```

**診断**:
```bash
# Sidecar が実行中か確認
ps aux | grep aira-graphdb-native

# ポートがリッスンしているか確認
netstat -tlnp | grep 7687

# ログで起動エラーを確認
cat aira-graphdb-native.log | tail -50
```

**対応**:
1. Sidecar 起動: `cargo run --bin aira-graphdb-native -- --db /path/to/db.db`
2. ポートが使用中でないか確認: `lsof -i :7687`
3. ファイアウォール確認: `sudo ufw status`（Linux）
4. クラッシュしている場合、`<db>.native-audit.log` の `PROCESS_CRASH` 確認

### 2.2 問題: 全ての書込で「WRITE_LOCK_CONFLICT」が発生

**症状**:
```
全ての書込操作が即座に WRITE_LOCK_CONFLICT で失敗
```

**診断**:
```bash
# WRITE_LOCK_CONFLICT の発生回数
grep WRITE_LOCK_CONFLICT <db>.native-audit.log | wc -l

# ロック保持者情報を確認
grep WRITE_LOCK_CONFLICT <db>.native-audit.log | jq '.details'
```

**対応**:
1. 別クライアントが書込ロックを保持していないか確認
2. Sidecar を再起動してスタックしたロックを解放し、標準の起動コマンドで再起動
3. 並行ライター数を削減（高並行では予想動作）
4. 競合が本質的な場合、コーパス分割を検討

### 2.3 問題: クエリが予期なく空結果を返す

**症状**:
```
Vector search が [] を返す
Lexical search が既知ドキュメントで [] を返す
```

**診断**:
```bash
# データがインデックスされたか確認
curl -X POST localhost:7687 -d '{"method":"get_nodes","params":{"corpusId":"c1"}}'

# ドキュメント存在確認
jq 'select(.method == "vector_upsert")' <db>.native-audit.log | head

# namespace とクエリ確認
grep vector_search <db>.native-audit.log | jq '.params'
```

**対応**:
1. 検索前にデータが挿入されたか確認: `get_nodes` で検証
2. 検索時の namespace が upsert 時と一致するか確認
3. `topK` が結果セットに対して小さすぎないか確認
4. 類似度閾値が高すぎないか確認

### 2.4 問題: メモリ使用量が無限に増加

**症状**:
```
メモリがリクエスト数に比例して増加
プロセスが最終的に OOM キルされる
```

**診断**:
```bash
# 時間経過に伴うメモリ監視
while true; do
  ps aux | grep aira-graphdb-native | grep -v grep | awk '{print $6}' && sleep 1
done

# メモリ不足ログ確認
grep OUT_OF_MEMORY <db>.native-audit.log | head

# ヒーププロファイル
valgrind --tool=massif ./target/release/aira-graphdb-native --db test.db
```

**対応**:
1. バッチサイズ削減: `upsert_nodes([10000])` → `upsert_nodes([100])`
2. チェックポイント有効化: メモリスナップショット定期保存
3. 定期的な古データ削除: `delete_by_document` で陳腐データ削除
4. コーパスが大きすぎる場合は分割

### 2.5 問題: Sidecar がランダムにクラッシュ

**症状**:
```
PROCESS_CRASH がクライアントエラーなしで監査ログに記録
接続が予期なく切断
```

**診断**:
```bash
# クラッシュログ確認
grep PROCESS_CRASH <db>.native-audit.log | jq '.{timestamp, signal, cause, lastRequestId}'

# クラッシュ前の最近リクエスト
grep lastRequestId <db>.native-audit.log | tail -5

# システムログ確認
dmesg | tail -20  # OOM キラー確認
journalctl -xe   # システムイベント
```

**対応**:
1. signal が SIGKILL の場合: システムメモリ確認（`free -h`）、RAM 増設またはバッチサイズ削減
2. signal が SIGSEGV の場合: バグ報告（クラッシュダンプ付き）
3. 原因が "panic" の場合: Rust バックトレース確認、詳細付きで issue 報告
4. コアダンプ有効化でデバッグ:
   ```bash
   ulimit -c unlimited
   cargo run --bin aira-graphdb-native -- --db test.db
   ```

## 3. デバッグモード

### 3.1 デバッグログ有効化

```bash
# デバッグ環境変数設定
export AGDB_DEBUG=1
export RUST_LOG=debug

# デバッグ出力付きで sidecar 実行
cargo run --bin aira-graphdb-native -- --db test.db 2>&1 | tee debug.log
```

デバッグ出力に含まれる項目：
- クエリ解析ステップ
- キャッシュヒット/ミス
- ロック取得/解放
- トランザクション状態遷移

### 3.2 最小再現テスト作成

問題を再現するテストを作成：

```javascript
async function testReproduction() {
  const client = new GraphDbClient();
  
  // ステップ 1: セットアップ
  await client.handshake();
  await client.auth(token);
  
  // ステップ 2: 問題再現
  await client.upsert_nodes([...]);
  
  // ステップ 3: 問題検証
  const result = await client.get_nodes({corpusId:'c1'});
  assert(result.length > 0, "ノード期待、空取得");
}

testReproduction().catch(err => {
  console.error("再現失敗:", err);
  process.exit(1);
});
```

`reproduce.js` として保存して実行：
```bash
node reproduce.js 2>&1 | tee reproduction.log
```

`reproduction.log` と `<db>.native-audit.log` をバグレポートに添付。

## 4. 性能デバッグ

### 4.1 遅いクエリの特定

```bash
# ログからクエリタイミング抽出
jq 'select(.type == "QUERY_EXECUTED") | {query: .details.query, duration_ms: .details.duration_ms}' \
  <db>.native-audit.log | sort -k 3 -n | tail -10
```

### 4.2 ロック競合の調査

```bash
# WRITE_LOCK_CONFLICT を発生させたリクエスト検索
jq 'select(.type == "WRITE_LOCK_CONFLICT") | .details.context' <db>.native-audit.log | sort | uniq -c

# 最も競合の多いコーパス特定
jq 'select(.type == "WRITE_LOCK_CONFLICT") | .details.context' <db>.native-audit.log | \
  grep -o "corpus_id=[^,]*" | sort | uniq -c
```

## 5. データ整合性チェック

### 5.1 グラフの一貫性検証

```json
{"method":"memory_validate_integrity","params":{"corpusId":"c1"}}
```

見つかった不一貫性の配列を返す（空 = 有効）。

### 5.2 データスナップショットの比較

```json
// 現在の状態を保存
{"method":"memory_save","params":{"snapshot":{"corpusId":"c1",...}}}

// 後で読込と比較
{"method":"memory_load","params":{"corpusId":"c1"}}
```

### 5.3 参照整合性チェック

```bash
# 全エッジ取得
jq '.[] | select(.method == "get_edges") | .result' <audit>

# 各エッジのソース/ターゲットノードが存在するか検証
jq '.[] | select(.method == "get_nodes") | .result | map(.nodeId)' <audit>
```

## 6. バグレポート用診断情報の収集

バグ報告時に以下を含める：

1. **環境**:
   - OS とバージョン
   - Rust バージョン（`rustc --version`）
   - Node/Python SDK バージョン
   - システムメモリ・ディスク

2. **再現手順**:
   - 最小再現コード
   - 入力データ（共有可能な場合）
   - 期待値 vs 実際値

3. **ログ**:
   - `<db>.native-audit.log` 全体
   - `aira-graphdb-native` stderr/stdout
   - クラッシュダンプまたはコアファイル

4. **メトリクス**:
   - `artifacts/native-bench-report.json`（パフォーマンスベースライン）
   - メモリ/CPU グラフ（入手可能な場合）
   - 問題発生の時間

5. **設定**:
   - Sidecar 起動コマンド
   - 設定した環境変数
   - コーパスサイズとクライアント数

共有前にログから機密データを削除してください。
