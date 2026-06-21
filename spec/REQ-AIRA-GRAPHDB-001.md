# REQ-AIRA-GRAPHDB-001: aira-graphdb 要件定義（Phase 1）

| フィールド | 値 |
|-----------|---|
| **ID** | REQ-AIRA-GRAPHDB-001 |
| **バージョン** | 1.7 |
| **ステータス** | Draft |
| **作成日** | 2026-06-20 |
| **更新日** | 2026-06-21 |
| **パッケージ** | `packages/aira-graphdb`（Rust crate） |
| **対象バージョン** | v0.2.0 |

## 1. 背景

`aira-synapse` から利用する新規 GraphDB を Rust でゼロから開発する。  
利用形態は以下の両立を前提とする。

- SQLite / LadybugDB のようなファイルベース（埋め込み）
- Neo4j のようなサーバーベース（常駐プロセス）

対象ユーザーは AIRA エコシステムの他サービス開発者であり、P0 は Property Graph CRUD / Cypher 互換クエリ / 永続化 / Python・Node SDK とする。

## 2. 機能要件（EARS）

### REQ-AGDB-001: Property Graph 永続化コア

**種別**: UBIQUITOUS  
**優先度**: P0

**要件**:  
THE システム SHALL Rust ベースで Property Graph（Node / Edge / Property）を永続化するコアDBを提供する。

**受入基準**:
- [ ] ノード、エッジ、プロパティの作成・読取・更新・削除が可能
- [ ] DB再起動後も永続化データを再読込できる
- [ ] ノードID、エッジIDが一意に管理される

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- db check`

---

### REQ-AGDB-002: 埋め込みモード

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
IF デプロイモードが `EMBEDDED` である場合, THEN THE システム SHALL ローカルDBファイルを直接操作し、常駐サーバープロセスなしで動作する。

**受入基準**:
- [ ] プロセス内ライブラリ呼び出しでDBを利用できる
- [ ] ローカルファイルパス指定でDBを開ける
- [ ] サーバー起動を必須としない

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- embedded open --file <path>`

---

### REQ-AGDB-003: サーバーモード

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
IF デプロイモードが `SERVER` である場合, THEN THE システム SHALL 常駐サーバーとして起動し、TCP経由でクライアント接続を受け付ける。  
IF デプロイモードが `SERVER` で `P0-SERVER-CONCURRENCY` プロファイルを適用する場合, THEN THE システム SHALL 同時に少なくとも32クライアント接続を受け付け、各接続の要求を処理する。

**受入基準**:
- [ ] 単一コマンドでサーバー起動できる
- [ ] 指定ポートで待受できる
- [ ] 複数クライアント接続を処理できる
- [ ] `P0-SERVER-CONCURRENCY` 条件下で同時32接続以上を処理できる

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- server start --port 7687`

---

### REQ-AGDB-004: Python/Node 共通プロトコル

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
WHEN Python または Node クライアントが接続を開始した場合, THE システム SHALL リリース同梱の不変仕様 `AGDB-TYPEMAP-P0@1.0.0` に対して `protocol_version` と `canonical_type_system_version` の両方をネゴシエーションし、合意した型マッピングのみを適用する。  
IF クライアントが `AGDB-TYPEMAP-P0@1.0.0` で未対応の `protocol_version` または `canonical_type_system_version` を提示した場合, THEN THE システム SHALL `PROTOCOL_VERSION_MISMATCH` を返し、セッションを確立しない。

**受入基準**:
- [ ] Python SDK と Node SDK が同一プロトコルバージョンで接続できる
- [ ] 同一入力に対して `AGDB-TYPEMAP-P0` に基づく同一型変換結果を返す
- [ ] `canonical_type_system_version` 不一致時に接続を拒否する
- [ ] プロトコルバージョン不一致時に `PROTOCOL_VERSION_MISMATCH` を返す
- [ ] バージョン不一致時にセッションを確立しない

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- server protocol-version`

---

### REQ-AGDB-005: openCypher 9 実行互換

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
WHEN `AGDB-CYPHER-OPENCYPHER9@1.0.0` で妥当なクエリを受信した場合, THE システム SHALL openCypher 9 の文法および意味規則に従って実行する。  
WHEN `ORDER BY` を含まない read クエリを実行した場合, THE システム SHALL 同一DB状態・同一クエリに対して同一行集合（multiset）を返す。  
WHEN `ORDER BY` を含む read クエリを実行した場合, THE システム SHALL openCypher 9 規則どおりの順序付き行列を返す。  
WHEN クエリが `MATCH/OPTIONAL MATCH/WHERE/WITH/UNWIND/RETURN/ORDER BY/SKIP/LIMIT` を含む場合, THE システム SHALL 句の評価順序とスコープ規則を openCypher 9 仕様どおりに適用する。  
WHEN クエリが `CREATE/MERGE/SET/REMOVE/DELETE/DETACH DELETE` を含む場合, THE システム SHALL 同一初期DB状態・同一クエリに対して同一最終グラフ状態を生成する。

**受入基準**:
- [ ] `AGDB-CYPHER-OPENCYPHER9@1.0.0` の構文に含まれる句を実行できる
- [ ] `OPTIONAL MATCH` を含む読み取りクエリを正しく実行できる
- [ ] `WITH` によるスコープ切替と別名解決を正しく実行できる
- [ ] `UNWIND` と集約関数（`count/sum/avg/min/max/collect`）を正しく実行できる
- [ ] `WHERE` / `WITH` 内の規範式（プロパティ参照、比較、論理演算、リスト/マップリテラル）を正しく評価できる
- [ ] `MERGE` 実行時に on-match/on-create セマンティクスを適用できる
- [ ] `ORDER BY/SKIP/LIMIT` を組み合わせた結果制御が正しく機能する
- [ ] `ORDER BY` なしの read クエリは multiset 同値で検証できる
- [ ] 構文エラー時に標準化エラーコードを返す

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- query "MATCH (n)-[r]->(m) WITH n,r,m RETURN n,r,m ORDER BY n.id SKIP 10 LIMIT 20"`

---

### REQ-AGDB-006: 準拠範囲内クエリの非拒否保証

**種別**: UNWANTED  
**優先度**: P0

**要件**:  
THE システム SHALL NOT `AGDB-CYPHER-OPENCYPHER9@1.0.0` の準拠範囲内クエリに対して `UNSUPPORTED_FEATURE` を返す。  
IF 準拠範囲外の拡張句（例: `CALL`, vendor-specific procedure）を受信した場合, THEN THE システム SHALL 部分実行を行わず `UNSUPPORTED_FEATURE` を返す。

**受入基準**:
- [ ] 準拠範囲内クエリで `UNSUPPORTED_FEATURE` を返さない
- [ ] 準拠範囲外句を含むクエリは部分更新を起こさない
- [ ] 準拠範囲外句に対して `UNSUPPORTED_FEATURE` を返す
- [ ] エラーメッセージに非対応句名を含む

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- query "CALL db.labels()"`

---

### REQ-AGDB-017: openCypher 9 準拠テストスイート

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
WHEN `AGDB-CYPHER-OPENCYPHER9@1.0.0` の互換性検証を実行した場合, THE システム SHALL `spec/conformance/opencypher9-required-tests.yaml` と `spec/conformance/opencypher9-tck-required.yaml` に定義された必須テストIDを自動実行し、必須ケースを 100% PASS させる。  
IF テストスイートに FAIL が存在する場合, THEN THE システム SHALL リリース判定を失敗させる。

**受入基準**:
- [ ] openCypher 9 の必須句に対する互換テストを自動実行できる
- [ ] 必須ケース集合が `spec/conformance/opencypher9-required-tests.yaml` に固定されている
- [ ] openCypher 9 upstream TCK の必須ケース集合が `spec/conformance/opencypher9-tck-required.yaml` に固定されている
- [ ] `spec/conformance/opencypher9-tck-required.yaml` の `snapshot_ref` が commit SHA で固定される
- [ ] 必須ケースの PASS 率が 100% である
- [ ] FAIL 時に CI が失敗し、リリースをブロックする
- [ ] 互換性レポート（句別 PASS/FAIL）を保存する
- [ ] `spec/conformance/opencypher9-required-tests.yaml` の各テストIDに `covers_req` と `covers_acceptance` が定義される
- [ ] 構文エラー、`UNSUPPORTED_FEATURE`、部分更新禁止を検証する negative ケースを必須集合に含む

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**決定根拠**: ADR-AGDB-002  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo test --test cypher_conformance -- --nocapture`

---

### REQ-AGDB-018: 準拠マニフェスト固定

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
WHEN Cypher 準拠ベースラインを確定する場合, THE システム SHALL `spec/contracts/agdb-cypher-opencypher9.v1.0.0.json` を不変マニフェストとして発行する。  
IF full-support スコープで openCypher 9 の規範機能を non-required として分類する場合, THEN THE システム SHALL リリース判定を失敗させる。

**受入基準**:
- [ ] `spec/contracts/agdb-cypher-opencypher9.v1.0.0.json` が存在する
- [ ] 規範句・規範関数・規範式の required/unsupported 状態が upstream TCK スナップショット由来で機械可読に定義される
- [ ] full-support 宣言時に required 欠落を許容しない
- [ ] マニフェストが `coverage_mode=closed_world` を定義し、未分類機能をリリース失敗として扱う
- [ ] マニフェスト内 `normative_feature_count` と `classified_normative_feature_count` が一致しない場合にリリースを失敗させる
- [ ] `spec/conformance/opencypher9-tck-required.yaml` とマニフェストの規範機能集合が一致しない場合にリリースを失敗させる

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**決定根拠**: ADR-AGDB-002  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- query conformance-manifest`

---

### REQ-AGDB-019: aira-synapse ストレージポート互換

**種別**: EVENT-DRIVEN  
**優先度**: P0

**要件**:  
WHEN `aira-synapse` が `backend=aira-graphdb` を選択した場合, THE システム SHALL `spec/contracts/aira-synapse-storage-ports.v1.0.0.json` に定義された `IGraphStore` / `IVectorIndex` / `IMemoryStore` / `IGraphProjection` / `ILexicalRetriever` の全メソッド契約（入力・出力・例外）に互換なアダプターAPIを提供する。  
IF `aira-synapse` の既存ユースケースが上記ポートを呼び出す場合, THEN THE システム SHALL アプリケーション層の呼び出し契約（引数・戻り値・失敗時シグネチャ）を維持して処理する。

**受入基準**:
- [ ] `aira-synapse` 側で `backend=aira-graphdb` を選択できる設定が存在する
- [ ] 5ポートの全必須メソッドが `spec/contracts/aira-synapse-storage-ports.v1.0.0.json` の契約テストを 100% PASS する
- [ ] 既存ユースケース統合テスト（`tests/integration/storage-port-compat/*.spec.ts`）が 100% PASS する
- [ ] 失敗時に `AGDB-ERROR-CODES@1.0.0` の固定コードへ写像できる

**トレーサビリティ**: DES-AGDB-009  
**パッケージ**: `packages/aira-graphdb`, `packages/memgraphrag`  
**CLI**: `MEMGRAPHRAG_BACKEND=aira-graphdb npm run test --workspace packages/memgraphrag`

---

### REQ-AGDB-020: ベクトル検索・全文検索互換

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
WHEN `aira-synapse` からベクトル `upsert/search/deleteByDocument` を要求された場合, THE システム SHALL `corpusId/namespace` 条件と `topK/threshold` 条件を満たす検索結果を返す。  
WHEN `aira-synapse` から全文検索を要求された場合, THE システム SHALL `memoryType` が `passage|fact` の結果を統合し、各結果に `documentId/text/score/memoryType` を含め、score 降順・同点時 documentId 昇順で返す。  
IF 同一データセット・同一クエリで baseline backend（neo4j）と比較した場合, THEN THE システム SHALL `topK` 結果の documentId 集合一致率を 1.0（threshold 指定時は threshold 適用後集合一致）にする。

**受入基準**:
- [ ] ベクトル upsert 後に `topK` 件検索できる
- [ ] `corpusId` と `namespace` のフィルタが保持される
- [ ] `threshold` 指定時に閾値未満を除外できる
- [ ] `deleteByDocument` 後に対象ドキュメントの検索ヒットが消える
- [ ] 全文検索で passage/fact 相当の統合結果を返せる
- [ ] 全文検索結果は score 降順、同点時 documentId 昇順であることを固定データセットで検証できる
- [ ] baseline backend（neo4j）との同一クエリ比較で `topK` 結果集合が一致する
- [ ] baseline backend（neo4j）との同一クエリ比較で、`threshold` 指定時は `threshold` 適用後の `topK` 結果集合が一致する
- [ ] 不正な `topK/threshold/corpusId/namespace` に対して `AGDB-ERROR-CODES@1.0.0` の固定エラーコードを返す

**トレーサビリティ**: DES-AGDB-010  
**パッケージ**: `packages/aira-graphdb`, `packages/memgraphrag`  
**CLI**: `cargo test --test aira_synapse_adapter_vector_lexical -- --nocapture`

---

### REQ-AGDB-021: バックエンド切替時の互換動作保証

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
IF `aira-synapse` の設定で `sqlite` / `ladybug` / `neo4j` に加えて `aira-graphdb` を選択した場合, THEN THE システム SHALL 下記「P0 共通ユースケース正準リスト」に定義された各互換テストIDで、成功/失敗判定、戻り値スキーマ、失敗時エラーコードを baseline backend（neo4j）と一致させる。  
IF `aira-graphdb` バックエンドで互換性テストが失敗した場合, THEN THE システム SHALL CI を失敗させてリリースをブロックし、失敗レポートに固定エラーコードと `failedCompatibilityTestIds`（非空配列）を含める。

**P0 共通ユースケース正準リスト**:

| ユースケースID | ユースケース | 互換テストID |
|---|---|---|
| AGDB-P0-UC-001 | グラフ CRUD（ノード/エッジ作成・取得・削除） | `storage-port-compat:graph-crud` |
| AGDB-P0-UC-002 | ベクトル upsert/search/deleteByDocument | `storage-port-compat:vector-crud` |
| AGDB-P0-UC-003 | 全文検索（passage/fact 統合） | `storage-port-compat:lexical-search` |
| AGDB-P0-UC-004 | メモリ永続化 CRUD | `storage-port-compat:memory-crud` |
| AGDB-P0-UC-005 | 射影取得（graph projection） | `storage-port-compat:projection` |
| AGDB-P0-UC-006 | バックエンド切替回帰（sqlite/ladybug/neo4j/aira-graphdb） | `backend-compat:routing-and-fallback` |

**受入基準**:
- [ ] `storageFactory` 相当のルーティングで `aira-graphdb` を解決できる
- [ ] backend 切替時、未処理例外で終了せず、失敗時は `AGDB-ERROR-CODES@1.0.0` の固定エラーコードを返す
- [ ] backend 互換回帰テスト（`tests/integration/backend-compat/*.spec.ts`）が 100% PASS する
- [ ] 「P0 共通ユースケース正準リスト」の全ユースケースIDに対して、対応する互換テストIDが 1:1 で存在する
- [ ] GitHub Actions workflow `.github/workflows/aira-synapse-backend-compat.yml` の job `backend-compat` を Required Check として設定し、失敗時にマージ不可となる
- [ ] 互換テスト失敗時の失敗レポートに `AGDB-ERROR-CODES@1.0.0` の固定エラーコードと `failedCompatibilityTestIds`（非空配列）が必ず含まれる

**トレーサビリティ**: DES-AGDB-011  
**パッケージ**: `packages/aira-graphdb`, `packages/memgraphrag`, `.github/workflows`  
**CLI**: `npm run test --workspace packages/memgraphrag && cargo test --test aira_synapse_adapter_integration`

---

### REQ-AGDB-007: COMMIT 成功時の耐久化保証

**種別**: EVENT-DRIVEN  
**優先度**: P0

**要件**:  
WHEN トランザクションの `COMMIT` が成功を返す場合, THE システム SHALL 当該トランザクションのログを stable storage へ永続化完了した後にのみ成功応答を返す。  
WHEN 成功応答後にプロセスクラッシュまたはホスト再起動が発生した場合, THE システム SHALL 復旧後に当該更新を可視化する。

**受入基準**:
- [ ] `COMMIT` 成功応答は stable storage への永続化完了後に返される
- [ ] COMMIT成功後の障害復旧で更新が保持される
- [ ] WALまたは同等メカニズムが有効である
- [ ] 復旧後に整合性チェックを通過する

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- db recover --file <path>`

---

### REQ-AGDB-008: クラッシュ後の部分反映禁止

**種別**: UNWANTED  
**優先度**: P0

**要件**:  
THE システム SHALL NOT クラッシュ復旧後に単一トランザクションの部分的副作用を可視化する。

**受入基準**:
- [ ] 復旧後、トランザクションは全反映または無反映のどちらか
- [ ] 部分更新を検出した場合は起動を失敗させる
- [ ] 整合性違反を監査ログに記録する

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- db verify --file <path>`

---

### REQ-AGDB-009: 公式SDK提供（Python/Node）

**種別**: UBIQUITOUS  
**優先度**: P0

**要件**:  
THE システム SHALL Python SDK および Node SDK を公式提供し、P0のCRUD・クエリAPIを同等機能で公開する。

**受入基準**:
- [ ] Python SDK と Node SDK の主要APIが機能等価
- [ ] 接続、CRUD、クエリ、トランザクションAPIを提供
- [ ] SDK入出力の型変換が `AGDB-TYPEMAP-P0` に準拠する
- [ ] サンプルコードで基本操作を実行できる

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`, `sdk/python`, `sdk/node`  
**CLI**: `npm run sdk:node:test` / `python -m pytest sdk/python/tests`

---

### REQ-AGDB-010: ROLLBACK 無副作用

**種別**: EVENT-DRIVEN  
**優先度**: P0

**要件**:  
WHEN トランザクションが `ROLLBACK` された場合, THE システム SHALL 当該トランザクションの副作用を永続化しない。

**受入基準**:
- [ ] ROLLBACK後に作成・更新・削除が反映されない
- [ ] サーバー再起動後も副作用がない
- [ ] ROLLBACKイベントを監査ログに記録する

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- tx rollback-test --file <path>`

---

### REQ-AGDB-011: 競合トランザクションの決定的動作

**種別**: EVENT-DRIVEN  
**優先度**: P0

**要件**:  
WHEN 同一ノードまたは同一エッジに対する書込み競合トランザクションが発生した場合, THE システム SHALL `SERIALIZABLE` 分離レベルを維持し、各トランザクションを `COMMIT` または `RETRYABLE_CONFLICT` のいずれかで終了させる。  
IF 同一競合シナリオを同一スケジュールで再実行した場合, THEN THE システム SHALL 同一終了結果を返す。

**受入基準**:
- [ ] 競合時の戻り値が `COMMIT` または `RETRYABLE_CONFLICT` に限定される
- [ ] 同一条件で非決定的な結果を返さない
- [ ] 分離レベルが `SERIALIZABLE` であることを設定またはメタ情報で確認できる

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- tx conflict-test --file <path>`

---

### REQ-AGDB-012: エッジ参照整合性

**種別**: EVENT-DRIVEN  
**優先度**: P0

**要件**:  
WHEN エッジの作成または更新を要求された場合, THE システム SHALL 始点ノードと終点ノードの存在を検証し、存在しない参照を拒否する。  
WHEN 既存エッジに参照されているノードの削除を要求された場合, THE システム SHALL `DETACH DELETE` が明示されない限り要求を拒否し、`REFERENTIAL_INTEGRITY_VIOLATION` を返す。  
WHEN `DETACH DELETE` が明示された場合, THE システム SHALL 当該ノードに接続する全エッジを同一トランザクションで削除する。

**受入基準**:
- [ ] 存在しないノード参照のエッジ作成を拒否する
- [ ] 拒否時は標準化エラーコードを返す
- [ ] 参照中ノードの通常削除を `REFERENTIAL_INTEGRITY_VIOLATION` で拒否する
- [ ] `DETACH DELETE` 指定時は関連エッジを同一トランザクションで削除する
- [ ] 参照整合性違反を監査ログに記録する

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- query "CREATE ()-[:R]->(:Missing)"`

---

### REQ-AGDB-013: unsafe同時書き込み禁止

**種別**: UNWANTED  
**優先度**: P0

**要件**:  
WHEN embedded モードまたは server モードのいずれかが同一DBファイルに対して排他書込みロックを保持中に、別プロセスが書込み可能モードでオープンを要求した場合, THE システム SHALL `WRITE_LOCK_CONFLICT` を返して要求を拒否し、対象ファイルを変更しない。

**受入基準**:
- [ ] 同一ファイルへの二重writer起動を拒否する
- [ ] ロック競合時に `WRITE_LOCK_CONFLICT` を返す
- [ ] 競合拒否時に対象ファイルを変更しない

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- lock test --file <path>`

---

### REQ-AGDB-014: フォーマット互換エラー

**種別**: EVENT-DRIVEN  
**優先度**: P0

**要件**:  
WHEN 非互換フォーマットバージョンのDBファイルをロードした場合, THE システム SHALL `INCOMPATIBLE_FORMAT` エラーで失敗し、データを変更しない。

**受入基準**:
- [ ] 非互換ファイル読み込み時に起動失敗する
- [ ] `INCOMPATIBLE_FORMAT` エラーを返す
- [ ] 読み込み失敗時にファイル内容が不変である

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- db open --file <path>`

---

### REQ-AGDB-015: サーバー認証必須

**種別**: EVENT-DRIVEN  
**優先度**: P0

**要件**:  
WHEN server モードでクライアントが接続する場合, THE システム SHALL `TLS 1.3` 以上を必須とし、ハンドシェイク完了前のアプリケーション要求を拒否する。  
WHEN クライアントが Bearer token を提示した場合, THE システム SHALL 設定済み `JWKS` または公開鍵で署名検証を行い、`issuer`、`audience`、`exp`、`nbf` を検証する。  
IF Bearer token の JOSE header の `alg` がサーバー設定の許可アルゴリズム一覧に含まれない、または `alg=none` である場合, THEN THE システム SHALL `AUTH_FAILED` を返してセッションを確立しない。  
IF Bearer token の `kid` に一致する鍵を設定済み `JWKS` または公開鍵集合から一意に解決できない場合, THEN THE システム SHALL `AUTH_FAILED` を返してセッションを確立しない。  
IF 署名検証またはクレーム検証に失敗した場合, THEN THE システム SHALL `AUTH_FAILED` を返してセッションを確立しない。  
WHEN server モードで未認証クライアントがクエリまたはトランザクション要求を送信した場合, THE システム SHALL `AUTH_REQUIRED` を返して要求を拒否する。

**受入基準**:
- [ ] 未認証接続を拒否する
- [ ] 認証成功後にのみクエリ実行可能
- [ ] 未認証要求に `AUTH_REQUIRED` を返す
- [ ] 認証検証失敗時に `AUTH_FAILED` を返す
- [ ] `TLS 1.3` 未満または非暗号化接続をセッション確立前に拒否する
- [ ] Bearer token の署名を `JWKS` または公開鍵で検証する
- [ ] Bearer token の `issuer`、`audience`、`exp`、`nbf` を検証する
- [ ] 許可外 `alg` および `alg=none` を `AUTH_FAILED` で拒否する
- [ ] 未解決または非一意な `kid` を `AUTH_FAILED` で拒否する
- [ ] 認証失敗イベントを監査ログに記録する

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- auth test --port 7687`

---

### REQ-AGDB-016: 標準エラーコードの固定定義

**種別**: UBIQUITOUS  
**優先度**: P0

**要件**:  
THE システム SHALL リリース同梱の不変エラー仕様 `AGDB-ERROR-CODES@1.0.0` に失敗種別と固定エラーコードを1対1で定義し、Python SDK・Node SDK・サーバーCLIで同一コードを返す。  
THE システム SHALL `AGDB-TYPEMAP-P0@1.0.0` で定義された各型変換失敗を固定エラーコードへ1対1で割り当てる。

**受入基準**:
- [ ] 失敗種別とエラーコードの対応表を `AGDB-ERROR-CODES@1.0.0` として仕様に保持する
- [ ] Python SDK、Node SDK、サーバーCLIで同一失敗に同一エラーコードを返す
- [ ] 型変換失敗と固定エラーコードの対応を `AGDB-TYPEMAP-P0@1.0.0` と整合させる
- [ ] 既存の `PROTOCOL_VERSION_MISMATCH` / `UNSUPPORTED_FEATURE` / `RETRYABLE_CONFLICT` / `WRITE_LOCK_CONFLICT` / `INCOMPATIBLE_FORMAT` / `AUTH_REQUIRED` / `AUTH_FAILED` を固定コードとして扱う

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`, `sdk/python`, `sdk/node`  
**CLI**: `cargo run -p aira-graphdb -- errors list`

## 3. 非機能要件

### REQ-AGDB-NF-001: P0レイテンシ目標（単一ノード）

**種別**: COMPLEX  
**優先度**: P1

**要件**:  
WHILE 単一ノード構成でベンチマークプロファイル `P0-LATENCY-BASELINE`（10万ノード、同時実行1、ウォームアップ後）を実行中, THE システム SHALL 単純MATCH/RETURNクエリのP95レイテンシを 50ms 以下に維持する。

**受入基準**:
- [ ] `P0-LATENCY-BASELINE` 条件下で P95 50ms 以下
- [ ] ベンチマーク結果をレポートとして保存する

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- bench latency`

---

### REQ-AGDB-NF-002: 将来クラスタ移行メタデータ

**種別**: STATE-DRIVEN  
**優先度**: P1

**要件**:  
WHILE 単一ノードモードで運用中, THE システム SHALL 将来の水平分散移行に備えて partition と replica のメタデータをバージョン付きシステムカタログに永続化する。

**受入基準**:
- [ ] system catalog に partition/replica メタ情報を保持する
- [ ] catalog schema version を確認できる
- [ ] メタデータ移行手順をドキュメント化する

**トレーサビリティ**: DES-AIRA-GRAPHDB-001（予定）  
**パッケージ**: `packages/aira-graphdb`  
**CLI**: `cargo run -p aira-graphdb -- catalog show`

---

### REQ-AGDB-NF-003: Native Write スループット/レイテンシ保証

**種別**: STATE-DRIVEN  
**優先度**: P0

**要件**:  
WHILE `backend=aira-graphdb` かつベンチマークプロファイル `P0-NATIVE-WRITE`（バッチ100、総書込10,000、同時接続8、永続化有効）を実行中, THE システム SHALL 書込みAPIの P95 レイテンシを 25ms 以下に維持する。  
WHILE `backend=aira-graphdb` かつベンチマークプロファイル `P0-NATIVE-WRITE`（バッチ100、総書込10,000、同時接続8、永続化有効）を実行中, THE システム SHALL 総書込 10,000 件を 8 秒以内で完了する。  
WHEN 書込みAPIが成功を返す場合, THE システム SHALL **1 API リクエスト単位**で原子的に永続化し、同一リクエスト内データの部分反映を許容しない。

**受入基準**:
- [ ] `P0-NATIVE-WRITE` 条件下で `upsert_nodes/upsert_edges/vector_upsert` の P95 が 25ms 以下
- [ ] 総書込10,000件を 8 秒以内で完了できる
- [ ] 書込み中クラッシュ注入後の再起動で JSON 破損を検出しない
- [ ] クラッシュ注入時、成功応答済み API リクエストは全反映、未成功応答リクエストは無反映である
- [ ] ベンチマーク結果を CI アーティファクトとして保存する

**トレーサビリティ**: DES-AGDB-012  
**パッケージ**: `packages/aira-graphdb`, `packages/memgraphrag`  
**CLI**: `cargo run -p aira-graphdb -- bench native-write`

---

### REQ-AGDB-NF-004: Native Read レイテンシ保証

**種別**: STATE-DRIVEN  
**優先度**: P0

**要件**:  
WHILE `backend=aira-graphdb` かつベンチマークプロファイル `P0-NATIVE-READ`（10万ノード、10万ベクトル、同時接続8、ウォームアップ60秒後、各API 10,000リクエスト測定）を実行中, THE システム SHALL `get_node` の P95 レイテンシを 5ms 以下に維持する。  
WHILE `backend=aira-graphdb` かつベンチマークプロファイル `P0-NATIVE-READ`（10万ノード、10万ベクトル、同時接続8、ウォームアップ60秒後、各API 10,000リクエスト測定）を実行中, THE システム SHALL `get_adjacent` の P95 レイテンシを 10ms 以下に維持する。  
WHILE `backend=aira-graphdb` かつベンチマークプロファイル `P0-NATIVE-READ`（10万ノード、10万ベクトル、同時接続8、ウォームアップ60秒後、各API 10,000リクエスト測定）を実行中, THE システム SHALL `vector_search(topK=10)` の P95 レイテンシを 30ms 以下に維持する。  
WHILE `backend=aira-graphdb` かつベンチマークプロファイル `P0-NATIVE-READ`（10万ノード、10万ベクトル、同時接続8、ウォームアップ60秒後、各API 10,000リクエスト測定）を実行中, THE システム SHALL `lexical_search(topK=10)` の P95 レイテンシを 30ms 以下に維持する。

**受入基準**:
- [ ] `get_node` の P95 が 5ms 以下
- [ ] `get_adjacent` の P95 が 10ms 以下
- [ ] `vector_search(topK=10)` の P95 が 30ms 以下
- [ ] `lexical_search(topK=10)` の P95 が 30ms 以下
- [ ] 各APIで 10,000 リクエスト以上を測定し、ウォームアップ 60 秒後の計測のみを集計する
- [ ] ベンチマーク結果を CI アーティファクトとして保存する

**トレーサビリティ**: DES-AGDB-012  
**パッケージ**: `packages/aira-graphdb`, `packages/memgraphrag`  
**CLI**: `cargo run -p aira-graphdb -- bench native-read`

---

### REQ-AGDB-NF-005: Native 通信層の継続稼働性

**種別**: COMPLEX  
**優先度**: P0

**要件**:  
WHEN Native 通信層が不正な JSON-RPC リクエストを受信した場合, THE システム SHALL プロセス全体を停止せず、当該リクエストに対して `INVALID_REQUEST_JSON` を返す。  
WHEN Native 通信層が未知メソッド要求を受信した場合, THE システム SHALL プロセス全体を停止せず、当該リクエストに対して `UNSUPPORTED_FEATURE` を返す。  
WHEN Native 通信層が単一リクエスト実行失敗を検出した場合, THE システム SHALL プロセス全体を停止せず、当該リクエストに対して `REQUEST_EXECUTION_FAILED` を返す。  
THE システム SHALL `REQUEST_EXECUTION_FAILED` の failureClass を `INTERNAL_BUG | IO_FAILURE | OOM | TIMEOUT | CLIENT_INPUT` の closed enum で分類する。  
WHILE 連続稼働プロファイル `P0-NATIVE-SOAK`（24時間、read/write 混在負荷）を実行中, THE システム SHALL 異常終了せず要求を処理し続ける。
WHILE 連続稼働プロファイル `P0-NATIVE-SOAK`（24時間、read/write 混在負荷）を実行中, THE システム SHALL 内部障害エラー率（`REQUEST_EXECUTION_FAILED` のうち内部障害分類件数 / 総リクエスト件数）を 0.1% 以下に維持する。  
WHEN リクエスト単位の異常系イベント（不正JSON、未知メソッド、実行失敗）を検出した場合, THE システム SHALL 監査ログへ `errorCode`, `failureClass`, `requestId`, `timestamp` を記録する。  
WHEN プロセス異常終了を検出した場合, THE システム SHALL 監査ログへ `errorCode`, `timestamp`, `processExitCode`, `signal`, `lastRequestId`, `uptimeSec` を記録する。

**受入基準**:
- [ ] 不正 JSON 入力時に sidecar がクラッシュせず、`INVALID_REQUEST_JSON` を返す
- [ ] 未知メソッド要求時に sidecar がクラッシュせず、`UNSUPPORTED_FEATURE` を返す
- [ ] 単一リクエスト実行失敗時に sidecar がクラッシュせず、`REQUEST_EXECUTION_FAILED` を返す
- [ ] `REQUEST_EXECUTION_FAILED` の failureClass が `INTERNAL_BUG | IO_FAILURE | OOM | TIMEOUT | CLIENT_INPUT` のいずれかで必ず記録される
- [ ] 24時間連続稼働テストでプロセス異常終了回数が 0 回
- [ ] soak 実行中の内部障害エラー率（failureClass が `INTERNAL_BUG | IO_FAILURE | OOM | TIMEOUT` の件数 / 成功・失敗を含む総リクエスト件数）が 0.1% 以下
- [ ] リクエスト単位異常イベント監査ログに `errorCode`, `failureClass`, `requestId`, `timestamp` が必ず含まれる
- [ ] 異常終了イベント監査ログに `errorCode`, `timestamp`, `processExitCode`, `signal`, `lastRequestId`, `uptimeSec` が必ず含まれる

**トレーサビリティ**: DES-AGDB-013  
**パッケージ**: `packages/aira-graphdb`, `packages/memgraphrag`  
**CLI**: `cargo run -p aira-graphdb -- soak native-rw --hours 24`

## 4. REQ-DES トレーサビリティマトリクス

本節のマトリクスを REQ↔DES の正式なソース・オブ・トゥルースとし、各要件セクション内の旧来トレーサビリティ行より優先する。

| REQ | DES |
|-----|-----|
| REQ-AGDB-001 | DES-AGDB-001 |
| REQ-AGDB-002 | DES-AGDB-002 |
| REQ-AGDB-003 | DES-AGDB-002 |
| REQ-AGDB-004 | DES-AGDB-003 |
| REQ-AGDB-005 | DES-AGDB-004 |
| REQ-AGDB-006 | DES-AGDB-004 |
| REQ-AGDB-007 | DES-AGDB-001 |
| REQ-AGDB-008 | DES-AGDB-001, DES-AGDB-005 |
| REQ-AGDB-009 | DES-AGDB-003 |
| REQ-AGDB-010 | DES-AGDB-001, DES-AGDB-005 |
| REQ-AGDB-011 | DES-AGDB-005 |
| REQ-AGDB-012 | DES-AGDB-005 |
| REQ-AGDB-013 | DES-AGDB-002 |
| REQ-AGDB-014 | DES-AGDB-001 |
| REQ-AGDB-015 | DES-AGDB-003, DES-AGDB-005 |
| REQ-AGDB-016 | DES-AGDB-003 |
| REQ-AGDB-017 | DES-AGDB-006 |
| REQ-AGDB-018 | DES-AGDB-006 |
| REQ-AGDB-019 | DES-AGDB-009 |
| REQ-AGDB-020 | DES-AGDB-010 |
| REQ-AGDB-021 | DES-AGDB-011 |
| REQ-AGDB-NF-001 | DES-AGDB-007 |
| REQ-AGDB-NF-002 | DES-AGDB-008 |
| REQ-AGDB-NF-003 | DES-AGDB-012 |
| REQ-AGDB-NF-004 | DES-AGDB-012 |
| REQ-AGDB-NF-005 | DES-AGDB-013 |

## 5. スコープ外（v0.2.0）

- 分散クラスタの本実装（リーダー選出、再配置、自動フェイルオーバー）
- Neo4j 固有拡張（APOC, vendor-specific procedure）の互換
- 多テナント課金機能
- LadybugDB 固有 SQL/Cypher 拡張の完全互換（v0.2.0 では対象外）
