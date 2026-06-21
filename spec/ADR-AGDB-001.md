# ADR-AGDB-001: Embedded/Server 二重モードと不変仕様契約を採用する

**ステータス**: proposed  
**日付**: 2026-06-20

## Context

aira-graphdb は SQLite/LadybugDB のようなファイルベース利用と、Neo4j のようなサーバーベース利用を両立する必要がある。  
さらに Python/Node SDK の互換を維持するため、型変換・クエリ文法・エラー契約を固定しないと実装差異が発生する。

## Decision

1. コアエンジンは単一実装とし、Runtime で Embedded/Server を切り替える。  
2. 以下をリリース同梱の不変仕様として固定する。  
   - `AGDB-TYPEMAP-P0@1.0.0`  
   - `AGDB-CYPHER-P0-GRAMMAR@1.0.0`  
   - `AGDB-ERROR-CODES@1.0.0`  
3. 認証は TLS 1.3 + JWT 署名検証（JWKS/公開鍵）を必須とする。

## Consequences

- SDK/CLI/サーバー間の契約ずれを抑制できる。  
- 仕様更新時はバージョンを上げる運用が必須になる。  
- 初期設計で仕様ファイル整備コストが増えるが、実装後の互換性問題を低減できる。
