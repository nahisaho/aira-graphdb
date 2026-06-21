# aira-graphdb Documentation

## 📚 Complete Documentation Map

### 🚀 Getting Started
- **[Installation Guide](install.md)** / **[インストールガイド](install.ja.md)**
  - System requirements, build from source, binary releases, Docker setup
  - システム要件、ソースビルド、バイナリリリース、Docker セットアップ

- **[Usage Guide](usage-guide.md)** / **[利用ガイド](usage-guide.ja.md)**
  - Quick start, basic operations, query examples, embedding vectors
  - Neo4j-compatible Cypher dialect usage
  - クイックスタート、基本操作、クエリ例、ベクトル埋め込み
  - Neo4j 互換 Cypher ダイアレクトの利用

- **[Client SDK Installation](client-sdk-install.md)** / **[クライアント SDK インストール](client-sdk-install.ja.md)** ⭐ NEW
  - Node.js SDK, Python SDK, Rust client setup and usage
  - Node.js SDK、Python SDK、Rust クライアント セットアップと使用方法

### 🛠️ Operations & Maintenance

- **[Error Handling Guide](error-handling.md)** / **[エラーハンドリングガイド](error-handling.ja.md)**
  - Error codes, retry strategies, common error scenarios
  - Recovery procedures, monitoring thresholds, error context extraction
  - エラーコード、リトライ戦略、一般的なエラーシナリオ
  - 復旧手順、監視閾値、エラーコンテキスト抽出

- **[Performance Tuning Guide](performance-tuning.md)** / **[性能チューニングガイド](performance-tuning.ja.md)**
  - Profiling tools (flamegraph, valgrind, heaptrack)
  - Query optimization, storage strategies, connection pooling
  - Horizontal scaling, configuration tuning, benchmarking
  - プロファイリングツール、クエリ最適化、ストレージ戦略
  - 水平スケーリング、設定チューニング、ベンチマーク

- **[Troubleshooting Guide](troubleshooting.md)** / **[トラブルシューティングガイド](troubleshooting.ja.md)**
  - Audit log interpretation, 5+ common issue scenarios
  - Debug mode activation, minimal reproducible tests
  - Diagnostic collection, performance debugging
  - 監査ログ解釈、5以上の一般的な問題シナリオ
  - デバッグモード有効化、最小再現テスト
  - 診断情報収集、性能デバッグ

### 🔧 Advanced

- **[Extension and Customization Guide](extension-guide.md)** / **[拡張・カスタマイズガイド](extension-guide.ja.md)**
  - Adding custom APOC procedures, memory storage types
  - New query language support, Neo4j-compatible query gating, vector embedding backends
  - Custom transaction isolation, audit event types
  - Extension testing and contribution guidelines
  - カスタム APOC プロシージャ、メモリストレージ型追加
  - 新しいクエリ言語サポート、ベクトル埋め込みバックエンド
  - カスタムトランザクション分離、監査イベント型

- **[Deployment Guide](deployment-guide.md)** / **[デプロイメント・運用ガイド](deployment-guide.ja.md)**
  - Docker multi-stage build, Dockerfile best practices
  - Kubernetes deployment, YAML templates, networking
  - Native Rust transport runtime, backend selection
  - Environment variables, Prometheus metrics, alerting
  - Backup/recovery procedures, scaling strategies
  - バックアップと復旧、メンテナンスタスク、アップグレード手順
  - Docker マルチステージビルド、Dockerfile ベストプラクティス
  - Kubernetes デプロイメント、YAML テンプレート、ネットワーク

## 📖 Documentation by Task

### I want to...

**Get started**
→ Start with [Installation Guide](install.md) and [Usage Guide](usage-guide.md)

**Integrate with my application**
→ Follow [Client SDK Installation](client-sdk-install.md) for Node.js, Python, or Rust

**Deploy to production**
→ Read [Deployment Guide](deployment-guide.md) for Docker/Kubernetes setup

**Troubleshoot an issue**
→ Check [Troubleshooting Guide](troubleshooting.md) for diagnostic steps and common solutions

**Optimize performance**
→ See [Performance Tuning Guide](performance-tuning.md) for profiling and optimization techniques

**Handle errors gracefully**
→ Reference [Error Handling Guide](error-handling.md) for error codes and retry strategies

**Extend the system**
→ Follow [Extension and Customization Guide](extension-guide.md) for adding custom procedures, types, and query gating

## 📋 Document Overview

| Document | Size | Topics | Audience |
|----------|------|--------|----------|
| Installation | 4 KB | Build, setup, requirements | New users, DevOps |
| Usage | 6 KB | Examples, queries, vectors, Neo4j compat | Developers |
| **Client SDK Installation** | **10 KB** | **Node.js, Python, Rust SDKs** | **Developers** ⭐ |
| Error Handling | 7 KB | Codes, recovery, monitoring | Operators, Developers |
| Performance Tuning | 7 KB | Profiling, optimization | Operators, Performance engineers |
| Troubleshooting | 9 KB | Debugging, common issues | Operators, Support |
| Extension and Customization | 9 KB | Custom procedures, types | Advanced developers |
| Deployment | 11 KB | Docker, K8s, operations | DevOps, Architects |

## 🎯 Quick Reference

### Error Codes
See [Error Handling Guide - Error Code Reference](error-handling.md#1-error-code-reference) for complete list

### Common Environment Variables
See [Deployment Guide - Environment Variables](deployment-guide.md#3-environment-variables)

### Performance Metrics
See [Deployment Guide - Prometheus Metrics](deployment-guide.md#41-prometheus-metrics)

### Backup/Recovery
See [Deployment Guide - Backup and Recovery](deployment-guide.md#5-backup-and-recovery)

## 📝 Documentation Standards

All guides follow these conventions:

- **Section numbering**: Consistent hierarchical numbering (1. Main, 1.1 Sub, 1.1.1 Detail)
- **Code examples**: Rust, JSON-RPC, shell scripts, YAML manifests
- **Tables**: Reference data in structured table format
- **Checklists**: Verification and quality assurance via checkboxes
- **Language**: English and Japanese (日本語) versions maintained in parallel
- **File naming**: `{name}.md` (English), `{name}.ja.md` (Japanese)

## 🔗 Related Resources

- **Requirements**: See `spec/REQ-AIRA-GRAPHDB-001.md` for feature specification
- **Design**: See `spec/DES-AIRA-GRAPHDB-001.md` for architecture and contracts
- **Implementation Plan**: See `spec/PLAN-AIRA-GRAPHDB-001.md` for roadmap
- **README**: See `README.md` and `README-ja.md` for project overview

## 🐛 Feedback

If you find issues, missing sections, or improvements needed:

1. Check existing documentation sections
2. Create a GitHub issue with:
   - Document name and section
   - What's missing or unclear
   - Suggested improvement
3. Submit a pull request with fixes

## 📞 Support

For help with specific scenarios:

- **Build/Setup issues** → [Installation Guide](install.md)
- **Runtime errors** → [Error Handling Guide](error-handling.md) + [Troubleshooting Guide](troubleshooting.md)
- **Performance problems** → [Performance Tuning Guide](performance-tuning.md)
- **Production deployment** → [Deployment Guide](deployment-guide.md)
- **System extension/customization** → [Extension and Customization Guide](extension-guide.md)
