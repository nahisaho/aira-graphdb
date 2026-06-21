# 性能チューニングと最適化ガイド（日本語）

## 1. プロファイリングと測定

### 1.1 Native ベンチマークスイート

組み込みベンチマークスイートで性能を測定：

```bash
cargo test --test native_perf_gate -- --nocapture
```

出力: `artifacts/native-bench-report.json`

主要メトリクス：
- `latency_p99_ms` - 99パーセンタイルクエリレイテンシ
- `throughput_ops_per_sec` - 1秒あたりのオペレーション数
- `memory_peak_mb` - テスト中のピークメモリ使用量
- `error_rate` - 失敗したリクエストの割合

### 1.2 Flamegraph を使ったローカルプロファイリング

Flamegraph ツールをインストール：
```bash
cargo install flamegraph
rustup component add llvm-tools-embedded
```

単一操作をプロファイル：
```bash
cargo flamegraph --bin aira-graphdb-native --freq 99 -- --db test.json
```

CPU 時間の分布を示す `flamegraph.svg` が生成されます。

### 1.3 メモリプロファイリング

Linux 上で Valgrind を利用：
```bash
valgrind --tool=massif --massif-out-file=massif.out \
  ./target/release/aira-graphdb-native --db test.json
ms_print massif.out
```

または heaptrack を利用：
```bash
heaptrack ./target/release/aira-graphdb-native --db test.json
heaptrack_gui heaptrack.aira-graphdb-native.*.gz
```

## 2. ボトルネック識別

### 一般的なボトルネックと対応策：

| ボトルネック | 症状 | 対応策 |
|-----------|-----|------|
| **ネットワークレイテンシ** | 高い `latency_p99_ms`、低 CPU | RPC ラウンドトリップ削減、バッチ操作利用 |
| **ロック競合** | 高い `LOCK_CONFLICT` エラー率 | 並行ライター削減、トランザクション利用 |
| **ディスク I/O** | 高い `IO_FAILURE` 率、遅い同期 | 高速ストレージ移行、バッチサイズ増加 |
| **メモリ圧力** | GC ポーズ増加 | コーパスサイズ削減、シャード化 |
| **パーサオーバーヘッド** | 高 CPU、中程度スループット | プリペアドステートメント利用 |

### プロファイリングチェックリスト：

- [ ] `native_perf_gate` を実行してベースライン確立
- [ ] レポートから最遅オペレーション特定
- [ ] Flamegraph または perf でプロファイル
- [ ] コードパスとボトルネックを関連付け
- [ ] 最適化を適用
- [ ] ベンチマーク再実行で改善確認
- [ ] トレードオフを記録（メモリ vs レイテンシ vs スループット）

## 3. 最適化戦略

### 3.1 クエリ最適化

**回避すべき**: WHERE なしの大規模 MATCH

```cypher
# ❌ 遅い - フルグラフスキャン
MATCH (n) WHERE n.type='Paper' RETURN n

# ✅ 高速 - 早期フィルタ
MATCH (n:Paper) RETURN n
```

**バッチ操作**: N 個の RPC 呼び出しではなく 1 個に集約：

```javascript
// ❌ 遅い: 1000 回の RPC 呼び出し
for (let i = 0; i < 1000; i++) {
  await db.upsert_nodes([nodes[i]]);
}

// ✅ 高速: 1 回の RPC 呼び出し
await db.upsert_nodes(nodes);
```

**列挙ではなく投影を利用**:

```json
{"method":"projection_get_transitions","params":{"corpusId":"c1"}}
```

全ノード・エッジを個別に取得する代わりに。

### 3.2 ストレージ最適化

**チェックポイント戦略**:

`memory_save_checkpoint` で定期的に復旧ポイント作成：

```json
{"method":"memory_save_checkpoint","params":{"checkpoint":{"jobId":"job-123","state":{...}}}}
```

メリット：
- クラッシュ時の復旧高速化
- WAL リプレイ時間短縮
- 増分バックアップ可能化

**ドキュメントライフサイクル**:

古いドキュメント削除時、一括操作を使用：

```json
{"method":"delete_by_document","params":{"corpusId":"c1","documentId":"doc-old"}}
```

関連グラフデータを 1 操作で削除。

### 3.3 コネクションプーリング

複数クライアントの場合、ハンドシェイクオーバーヘッド削減にコネクションプーリング利用：

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

### 3.4 キャッシング戦略

**ベクトル検索結果のキャッシング**:

埋め込みをローカルキャッシュして冗長検索を回避：

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

TTL（有効期限）を実装：
```javascript
const cache = new Map();
setInterval(() => {
  const now = Date.now();
  for (const [key, {timestamp}] of cache.entries()) {
    if (now - timestamp > 3600000) { // 1 時間
      cache.delete(key);
    }
  }
}, 300000); // 5 分ごとにチェック
```

## 4. スケーリング戦略

### 4.1 コーパスによるシャーディング

1 つの大規模コーパスではなく複数コーパスに分割：

```json
// 前: 1 コーパスに 1000 万ノード
{"corpusId":"all","nodes":[...]}

// 後: 10 コーパス、各 100 万ノード
{"corpusId":"corpus-0","nodes":[...]}
{"corpusId":"corpus-1","nodes":[...]}
```

メリット：
- クエリ並列化可能
- コーパス単位のメモリ削減
- 独立した復旧

### 4.2 水平スケーリング

ロードバランサ背後で複数 sidecar インスタンス実行：

```bash
# インスタンス 1
cargo run --bin aira-graphdb-native -- --db /data/db-1.json

# インスタンス 2
cargo run --bin aira-graphdb-native -- --db /data/db-2.json
```

コーパス ID のハッシュでルーティング：

```javascript
function getInstanceForCorpus(corpusId, instanceCount) {
  return hash(corpusId) % instanceCount;
}
```

## 5. 設定チューニング

### 5.1 環境変数

```bash
# 最大接続数増加
AGDB_MAX_CONNECTIONS=1000

# バッチタイムアウト短縮
AGDB_BATCH_TIMEOUT_MS=500

# デバッグログ有効化
AGDB_DEBUG=1
```

### 5.2 ランタイムパラメータ

`native_bench.rs` でプロファイルパラメータを調整：

```rust
pub struct SoakProfile {
    pub duration_minutes: u64,
    pub batch_size: usize,           // スループット向上で増加
    pub concurrent_clients: usize,   // 競合でチューニング
    pub random_seed: u64,
}
```

## 6. 最適化効果の監視

### 6.1 前後比較

```bash
# ベースライン
cargo test --test native_perf_gate
cp artifacts/native-bench-report.json baseline.json

# 最適化適用

# 再テスト
cargo test --test native_perf_gate
cp artifacts/native-bench-report.json optimized.json

# 比較
jq '.latency_p99_ms' baseline.json optimized.json
```

### 6.2 回帰検出

CI に追加して性能低下を検出：

```yaml
- name: Compare with baseline
  run: |
    jq '.latency_p99_ms' baseline.json > baseline.txt
    jq '.latency_p99_ms' optimized.json > current.txt
    REGRESSION=$(awk 'NR==2{if($1 > baseline*1.1) print 1; else print 0}' baseline.txt current.txt)
    if [ $REGRESSION -eq 1 ]; then exit 1; fi
```

## 7. 一般的なトレードオフ

| 最適化 | メリット | コスト |
|------|----------|------|
| より大きなバッチサイズ | より高いスループット | メモリ増加、バッチごとのレイテンシ上昇 |
| コーパスシャーディング | 競合低下 | 運用複雑性、ストレージ増加 |
| コネクションプーリング | レイテンシ低下 | メモリ増加 |
| 結果キャッシング | クエリ削減 | データ鮮度低下、無効化オーバーヘッド |
| プロパティインデックス | クエリ高速化 | メモリ増加、書込遅延 |

ワークロードの優先順位に基づいて選択してください。
