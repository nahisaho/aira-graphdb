# ADR-AGDB-002: openCypher 9 フル対応を TCK 固定スナップショットで検証する

**ステータス**: accepted  
**日付**: 2026-06-21

## Context

Cypher を「フル対応」と宣言するには、実装者裁量の部分集合ではなく、規範機能集合を固定して機械検証できる必要がある。  
従来の句単位サブセット仕様だけでは、互換主張の再現性と監査可能性が不足する。

## Decision

1. `AGDB-CYPHER-OPENCYPHER9@1.0.0` を closed-world マニフェストとして採用する。  
2. 規範機能集合は `spec/conformance/opencypher9-tck-required.yaml` の固定スナップショット（コミットSHA）を根拠とし、`spec/conformance/opencypher9-required-tests.yaml` のシナリオ必須集合と同時に運用する。  
3. リリースゲートで以下を必須化する。  
   - required TCK IDs 100% PASS  
   - required scenario suite 100% PASS  
   - `normative_feature_count == classified_normative_feature_count`  
   - マニフェスト機能集合と TCK required 集合の同期  
   - `covers_req` / `covers_acceptance` メタデータ検証  
   - syntax/unsupported/partial-update の negative ケース必須化  
   - 互換性レポートの保存

## Consequences

- フル対応主張の監査可能性が向上する。  
- 仕様更新時に TCK スナップショット更新と再検証コストが発生する。  
- CI の conformance ジョブが重くなるため、段階実行とキャッシュ設計が必要になる。
