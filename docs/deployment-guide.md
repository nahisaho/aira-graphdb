# Deployment and Operations Guide (English)

## 1. Docker Deployment

### 1.1 Multi-stage build Dockerfile

```dockerfile
# Stage 1: Builder
FROM rust:1.75 as builder

WORKDIR /usr/src/aira-graphdb

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy source
COPY . .

# Build release binary
RUN cargo build --release --bin aira-graphdb-native

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /usr/src/aira-graphdb/target/release/aira-graphdb-native /usr/local/bin/

# Create non-root user
RUN useradd -m -u 1000 agdb

# Set working directory
WORKDIR /data

# Change ownership
RUN chown -R agdb:agdb /data

# Switch to non-root user
USER agdb

# Expose RPC port
EXPOSE 3001

# Health check
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD nc -zv localhost 3001

# Start service
ENTRYPOINT ["aira-graphdb-native"]
CMD ["--port", "3001"]
```

### 1.2 Build and run

```bash
# Build image
docker build -t aira-graphdb:0.1.1 .

# Run container
docker run -d \
  --name aira-graphdb \
  -p 3001:3001 \
  -v /data/graphdb:/data \
  -e AGDB_DEBUG=0 \
  -e RUST_LOG=info \
  aira-graphdb:0.1.1

# View logs
docker logs -f aira-graphdb

# Stop container
docker stop aira-graphdb
```

## 2. Kubernetes Deployment

### 2.1 Deployment manifest

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

### 2.2 Deploy to Kubernetes

```bash
# Create namespace
kubectl create namespace aira-system

# Create ConfigMap
kubectl create configmap aira-graphdb-config \
  --from-literal=log-level=info \
  -n aira-system

# Apply manifest
kubectl apply -f k8s-deployment.yaml

# Check status
kubectl get deployment -n aira-system
kubectl get pods -n aira-system

# View logs
kubectl logs -n aira-system deployment/aira-graphdb -f

# Port forward for testing
kubectl port-forward -n aira-system svc/aira-graphdb 3001:3001
```

## 3. Environment Variables

### Core Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| AGDB_PORT | 3001 | RPC server port |
| AGDB_DATA_DIR | ./data | Data storage directory |
| AGDB_MAX_CONNECTIONS | 100 | Max concurrent connections |
| AGDB_BATCH_SIZE | 1000 | Batch operation size |
| AGDB_CACHE_SIZE_MB | 512 | In-memory cache size |
| AGDB_DEBUG | 0 | Enable debug mode (0/1) |

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| RUST_LOG | info | Log level (debug/info/warn/error) |
| AGDB_LOG_FORMAT | json | Log format (json/text) |
| AGDB_AUDIT_LOG | 1 | Enable audit logging (0/1) |

### Performance Tuning

| Variable | Default | Description |
|----------|---------|-------------|
| AGDB_WORKERS | cpu_count | Number of worker threads |
| AGDB_FSYNC_INTERVAL_MS | 100 | Fsync interval for durability |
| AGDB_QUERY_TIMEOUT_MS | 30000 | Query execution timeout |
| AGDB_SNAPSHOT_INTERVAL_S | 3600 | Snapshot interval in seconds |

### Example .env file

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

## 4. Monitoring and Alerting

### 4.1 Prometheus metrics

Expose metrics endpoint at `/metrics`:

```bash
curl http://localhost:3001/metrics
```

Key metrics:

- `agdb_queries_total` - Total queries executed
- `agdb_query_duration_seconds` - Query execution duration
- `agdb_cache_hits_total` - Cache hit count
- `agdb_errors_total` - Error count by code
- `agdb_memory_bytes` - Memory usage
- `agdb_disk_bytes` - Disk usage

### 4.2 Prometheus scrape config

```yaml
global:
  scrape_interval: 15s

scrape_configs:
- job_name: 'aira-graphdb'
  static_configs:
  - targets: ['localhost:3001']
  metrics_path: '/metrics'
```

### 4.3 Alert rules

```yaml
groups:
- name: aira-graphdb
  rules:
  - alert: HighErrorRate
    expr: rate(agdb_errors_total[5m]) > 0.1
    annotations:
      summary: "High error rate detected"
  
  - alert: HighMemoryUsage
    expr: agdb_memory_bytes > 1.5e9
    annotations:
      summary: "Memory usage > 1.5GB"
  
  - alert: SlowQueries
    expr: histogram_quantile(0.99, agdb_query_duration_seconds) > 1
    annotations:
      summary: "P99 query latency > 1s"
```

## 5. Backup and Recovery

### 5.1 Backup strategy

```bash
# Create snapshot backup
curl -X POST http://localhost:3001/snapshot \
  -H "Content-Type: application/json" \
  -d '{}' > backup.json

# Compress and store
gzip backup.json
mv backup.json.gz backups/backup-$(date +%Y%m%d-%H%M%S).json.gz

# Upload to S3
aws s3 cp backups/backup-*.json.gz s3://aira-backups/graphdb/
```

### 5.2 Recovery procedure

```bash
# Stop service
systemctl stop aira-graphdb

# Restore from backup
aws s3 cp s3://aira-backups/graphdb/backup-latest.json.gz /tmp/
gunzip /tmp/backup-latest.json.gz

# Restore to data directory
curl -X POST http://localhost:3001/restore \
  -H "Content-Type: application/json" \
  -d @/tmp/backup-latest.json

# Verify integrity
curl http://localhost:3001/verify

# Start service
systemctl start aira-graphdb
```

## 6. Scaling Strategy

### 6.1 Vertical scaling

Increase resource limits per instance:

```yaml
resources:
  requests:
    cpu: 2000m
    memory: 4Gi
  limits:
    cpu: 4000m
    memory: 8Gi
```

### 6.2 Horizontal scaling

Add read replicas with sharding:

```bash
# Split data by corpus hash
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

## 7. Maintenance Tasks

### 7.1 Regular maintenance

```bash
#!/bin/bash
# daily-maintenance.sh

# Create daily snapshot
curl -X POST http://localhost:3001/snapshot > /backups/daily-$(date +%Y%m%d).json

# Verify database integrity
curl http://localhost:3001/verify

# Check disk space
du -sh /data/graphdb

# Rotate logs
find /var/log/aira-graphdb -name "*.log.*" -mtime +30 -delete
```

### 7.2 Upgrade procedure

```bash
# 1. Create backup
curl -X POST http://localhost:3001/snapshot > backup-pre-upgrade.json

# 2. Pull new image
docker pull aira-graphdb:0.1.2

# 3. Stop service
docker stop aira-graphdb

# 4. Start upgraded service
docker run -d \
  --name aira-graphdb \
  -p 3001:3001 \
  -v /data/graphdb:/data \
  aira-graphdb:0.1.2

# 5. Verify health
sleep 5
curl http://localhost:3001/health

# 6. Run compatibility checks
curl http://localhost:3001/verify
```

## 8. Troubleshooting Deployment

### 8.1 Container won't start

```bash
# Check logs
docker logs aira-graphdb

# Check permissions
ls -la /data/graphdb

# Verify port availability
netstat -tuln | grep 3001

# Test with debug mode
docker run -it --rm \
  -e AGDB_DEBUG=1 \
  -e RUST_LOG=debug \
  aira-graphdb:0.1.1
```

### 8.2 High latency

```bash
# Check resource usage
docker stats aira-graphdb

# Review slow query logs
curl http://localhost:3001/metrics | grep query_duration

# Check disk I/O
iostat -x 1

# Monitor with profiling
AGDB_DEBUG=1 docker run aira-graphdb:0.1.1
```

### 8.3 Out of memory

```bash
# Check memory usage
curl http://localhost:3001/metrics | grep memory_bytes

# Increase limit
docker update --memory 4g aira-graphdb

# Or in Kubernetes:
kubectl set resources deployment aira-graphdb \
  -c aira-graphdb \
  --limits=memory=4Gi \
  -n aira-system
```

## 9. Deployment Checklist

- [ ] System requirements met (CPU, memory, disk)
- [ ] Network connectivity verified
- [ ] Data directory prepared
- [ ] Environment variables configured
- [ ] Security settings applied (non-root user, read-only filesystems)
- [ ] Monitoring/alerting configured
- [ ] Backup strategy implemented
- [ ] Disaster recovery tested
- [ ] Load testing completed
- [ ] Documentation reviewed
