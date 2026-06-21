# デプロイメント・運用ガイド（日本語）

## 1. Docker デプロイメント

### 1.1 マルチステージビルド Dockerfile

```dockerfile
# ステージ 1: ビルダー
FROM rust:1.75 as builder

WORKDIR /usr/src/aira-graphdb

# 依存パッケージをインストール
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# ソースコードをコピー
COPY . .

# リリースバイナリをビルド
RUN cargo build --release --bin aira-graphdb-native

# ステージ 2: ランタイム
FROM debian:bookworm-slim

# ランタイム依存パッケージをインストール
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# ビルダーからバイナリをコピー
COPY --from=builder /usr/src/aira-graphdb/target/release/aira-graphdb-native /usr/local/bin/

# 非 root ユーザーを作成
RUN useradd -m -u 1000 agdb

# 作業ディレクトリを設定
WORKDIR /data

# 所有権を変更
RUN chown -R agdb:agdb /data

# 非 root ユーザーに切り替え
USER agdb

# RPC ポートを公開
EXPOSE 3001

# ヘルスチェック
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD nc -zv localhost 3001

# サービス開始
ENTRYPOINT ["aira-graphdb-native"]
CMD ["--port", "3001"]
```

### 1.2 ビルド・実行

```bash
# イメージをビルド
docker build -t aira-graphdb:0.1.1 .

# コンテナを実行
docker run -d \
  --name aira-graphdb \
  -p 3001:3001 \
  -v /data/graphdb:/data \
  -e AGDB_DEBUG=0 \
  -e RUST_LOG=info \
  aira-graphdb:0.1.1

# ログを表示
docker logs -f aira-graphdb

# コンテナを停止
docker stop aira-graphdb
```

## 2. Kubernetes デプロイメント

### 2.1 Deployment マニフェスト

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: aira-graphdb
  namespace: aira-system
  labels:
    app: aira-graphdb
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app: aira-graphdb
  template:
    metadata:
      labels:
        app: aira-graphdb
    spec:
      serviceAccountName: aira-graphdb
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000
      containers:
      - name: aira-graphdb
        image: aira-graphdb:0.1.1
        imagePullPolicy: IfNotPresent
        ports:
        - name: rpc
          containerPort: 3001
          protocol: TCP
        env:
        - name: AGDB_DEBUG
          value: "0"
        - name: RUST_LOG
          value: info
        - name: AGDB_PORT
          value: "3001"
        - name: AGDB_DATA_DIR
          value: /data
        resources:
          requests:
            cpu: 500m
            memory: 512Mi
          limits:
            cpu: 2000m
            memory: 2Gi
        livenessProbe:
          tcpSocket:
            port: rpc
          initialDelaySeconds: 10
          periodSeconds: 10
          timeoutSeconds: 3
          failureThreshold: 3
        readinessProbe:
          exec:
            command:
            - /bin/sh
            - -c
            - "echo 'PING' | nc localhost 3001"
          initialDelaySeconds: 5
          periodSeconds: 5
          timeoutSeconds: 2
          failureThreshold: 2
        volumeMounts:
        - name: data
          mountPath: /data
        - name: config
          mountPath: /etc/aira-graphdb
          readOnly: true
        securityContext:
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
          capabilities:
            drop:
            - ALL
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: aira-graphdb-data
      - name: config
        configMap:
          name: aira-graphdb-config
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - weight: 100
            podAffinityTerm:
              labelSelector:
                matchExpressions:
                - key: app
                  operator: In
                  values:
                  - aira-graphdb
              topologyKey: kubernetes.io/hostname
---
apiVersion: v1
kind: Service
metadata:
  name: aira-graphdb
  namespace: aira-system
spec:
  type: ClusterIP
  ports:
  - port: 3001
    targetPort: rpc
    protocol: TCP
  selector:
    app: aira-graphdb
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: aira-graphdb-data
  namespace: aira-system
spec:
  accessModes:
  - ReadWriteOnce
  resources:
    requests:
      storage: 10Gi
  storageClassName: standard
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: aira-graphdb
  namespace: aira-system
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: aira-graphdb
rules:
- apiGroups: [""]
  resources: ["pods"]
  verbs: ["get", "list"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: aira-graphdb
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: aira-graphdb
subjects:
- kind: ServiceAccount
  name: aira-graphdb
  namespace: aira-system
```

### 2.2 Kubernetes へデプロイ

```bash
# namespace を作成
kubectl create namespace aira-system

# ConfigMap を作成
kubectl create configmap aira-graphdb-config \
  --from-literal=log-level=info \
  -n aira-system

# マニフェストを適用
kubectl apply -f k8s-deployment.yaml

# ステータスを確認
kubectl get deployment -n aira-system
kubectl get pods -n aira-system

# ログを表示
kubectl logs -n aira-system deployment/aira-graphdb -f

# ポート転送してテスト
kubectl port-forward -n aira-system svc/aira-graphdb 3001:3001
```

## 3. 環境変数

### コア設定

| 変数 | デフォルト | 説明 |
|------|-----------|------|
| AGDB_PORT | 3001 | RPC サーバーポート |
| AGDB_DATA_DIR | ./data | データストレージディレクトリ |
| AGDB_MAX_CONNECTIONS | 100 | 最大同時接続数 |
| AGDB_BATCH_SIZE | 1000 | バッチ操作サイズ |
| AGDB_CACHE_SIZE_MB | 512 | メモリ内キャッシュサイズ |
| AGDB_DEBUG | 0 | デバッグモード有効化（0/1） |

### ロギング

| 変数 | デフォルト | 説明 |
|------|-----------|------|
| RUST_LOG | info | ログレベル（debug/info/warn/error） |
| AGDB_LOG_FORMAT | json | ログフォーマット（json/text） |
| AGDB_AUDIT_LOG | 1 | 監査ログ有効化（0/1） |

### パフォーマンスチューニング

| 変数 | デフォルト | 説明 |
|------|-----------|------|
| AGDB_WORKERS | cpu_count | ワーカースレッド数 |
| AGDB_FSYNC_INTERVAL_MS | 100 | 永続性の fsync 間隔 |
| AGDB_QUERY_TIMEOUT_MS | 30000 | クエリ実行タイムアウト |
| AGDB_SNAPSHOT_INTERVAL_S | 3600 | スナップショット間隔（秒） |

### .env ファイル例

```bash
AGDB_PORT=3001
AGDB_DATA_DIR=/var/lib/aira-graphdb
AGDB_MAX_CONNECTIONS=200
AGDB_CACHE_SIZE_MB=1024
AGDB_DEBUG=0
RUST_LOG=info
AGDB_LOG_FORMAT=json
AGDB_AUDIT_LOG=1
AGDB_WORKERS=8
AGDB_FSYNC_INTERVAL_MS=100
AGDB_QUERY_TIMEOUT_MS=30000
AGDB_SNAPSHOT_INTERVAL_S=3600
```

## 4. モニタリングとアラート

### 4.1 Prometheus メトリクス

`/metrics` エンドポイントでメトリクスを公開：

```bash
curl http://localhost:3001/metrics
```

主要メトリクス：

- `agdb_queries_total` - 実行されたクエリ総数
- `agdb_query_duration_seconds` - クエリ実行時間
- `agdb_cache_hits_total` - キャッシュヒット数
- `agdb_errors_total` - エラーコード別エラー数
- `agdb_memory_bytes` - メモリ使用量
- `agdb_disk_bytes` - ディスク使用量

### 4.2 Prometheus スクレイプ設定

```yaml
global:
  scrape_interval: 15s

scrape_configs:
- job_name: 'aira-graphdb'
  static_configs:
  - targets: ['localhost:3001']
  metrics_path: '/metrics'
```

### 4.3 アラートルール

```yaml
groups:
- name: aira-graphdb
  rules:
  - alert: HighErrorRate
    expr: rate(agdb_errors_total[5m]) > 0.1
    annotations:
      summary: "高いエラー率を検出"
  
  - alert: HighMemoryUsage
    expr: agdb_memory_bytes > 1.5e9
    annotations:
      summary: "メモリ使用量が 1.5GB を超過"
  
  - alert: SlowQueries
    expr: histogram_quantile(0.99, agdb_query_duration_seconds) > 1
    annotations:
      summary: "P99 クエリレイテンシーが 1s を超過"
```

## 5. バックアップと復旧

### 5.1 バックアップ戦略

```bash
# スナップショットバックアップを作成
curl -X POST http://localhost:3001/snapshot \
  -H "Content-Type: application/json" \
  -d '{}' > backup.json

# 圧縮して保存
gzip backup.json
mv backup.json.gz backups/backup-$(date +%Y%m%d-%H%M%S).json.gz

# S3 にアップロード
aws s3 cp backups/backup-*.json.gz s3://aira-backups/graphdb/
```

### 5.2 復旧手順

```bash
# サービスを停止
systemctl stop aira-graphdb

# バックアップから復旧
aws s3 cp s3://aira-backups/graphdb/backup-latest.json.gz /tmp/
gunzip /tmp/backup-latest.json.gz

# データディレクトリに復旧
curl -X POST http://localhost:3001/restore \
  -H "Content-Type: application/json" \
  -d @/tmp/backup-latest.json

# 整合性を検証
curl http://localhost:3001/verify

# サービスを開始
systemctl start aira-graphdb
```

## 6. スケーリング戦略

### 6.1 垂直スケーリング

インスタンスあたりのリソースリミットを増加：

```yaml
resources:
  requests:
    cpu: 2000m
    memory: 4Gi
  limits:
    cpu: 4000m
    memory: 8Gi
```

### 6.2 水平スケーリング

シャーディングを使用した読み取りレプリカを追加：

```bash
# データをコーパスハッシュで分割
REPLICA_COUNT=3
for i in {1..3}; do
  SHARD=$((i-1))
  docker run -d \
    --name aira-graphdb-shard-$SHARD \
    -e AGDB_SHARD_ID=$SHARD \
    -e AGDB_SHARD_COUNT=$REPLICA_COUNT \
    aira-graphdb:0.1.1
done
```

## 7. メンテナンスタスク

### 7.1 定期メンテナンス

```bash
#!/bin/bash
# daily-maintenance.sh

# 日次スナップショットを作成
curl -X POST http://localhost:3001/snapshot > /backups/daily-$(date +%Y%m%d).json

# データベース整合性を検証
curl http://localhost:3001/verify

# ディスク使用量を確認
du -sh /data/graphdb

# ログをローテーション
find /var/log/aira-graphdb -name "*.log.*" -mtime +30 -delete
```

### 7.2 アップグレード手順

```bash
# 1. バックアップを作成
curl -X POST http://localhost:3001/snapshot > backup-pre-upgrade.json

# 2. 新しいイメージを取得
docker pull aira-graphdb:0.1.2

# 3. サービスを停止
docker stop aira-graphdb

# 4. アップグレード済みサービスを開始
docker run -d \
  --name aira-graphdb \
  -p 3001:3001 \
  -v /data/graphdb:/data \
  aira-graphdb:0.1.2

# 5. ヘルスを確認
sleep 5
curl http://localhost:3001/health

# 6. 互換性チェックを実行
curl http://localhost:3001/verify
```

## 8. デプロイメントのトラブルシューティング

### 8.1 コンテナが起動しない

```bash
# ログを確認
docker logs aira-graphdb

# パーミッションを確認
ls -la /data/graphdb

# ポート可用性を確認
netstat -tuln | grep 3001

# デバッグモードで試す
docker run -it --rm \
  -e AGDB_DEBUG=1 \
  -e RUST_LOG=debug \
  aira-graphdb:0.1.1
```

### 8.2 高レイテンシー

```bash
# リソース使用量を確認
docker stats aira-graphdb

# スロークエリログを確認
curl http://localhost:3001/metrics | grep query_duration

# ディスク I/O を確認
iostat -x 1

# プロファイリングで監視
AGDB_DEBUG=1 docker run aira-graphdb:0.1.1
```

### 8.3 メモリ不足

```bash
# メモリ使用量を確認
curl http://localhost:3001/metrics | grep memory_bytes

# リミットを増加
docker update --memory 4g aira-graphdb

# または Kubernetes で：
kubectl set resources deployment aira-graphdb \
  -c aira-graphdb \
  --limits=memory=4Gi \
  -n aira-system
```

## 9. デプロイメントチェックリスト

- [ ] システム要件を満たしている（CPU、メモリ、ディスク）
- [ ] ネットワーク接続を確認済み
- [ ] データディレクトリを準備
- [ ] 環境変数を設定
- [ ] セキュリティ設定を適用（非 root ユーザー、読み取り専用ファイルシステム）
- [ ] モニタリング・アラートを設定
- [ ] バックアップ戦略を実装
- [ ] 災害復旧をテスト
- [ ] ロードテストを完了
- [ ] ドキュメントを確認
