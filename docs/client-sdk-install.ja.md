# クライアント SDK インストールガイド（日本語）

## 概要

aira-graphdb は3つの言語用クライアント SDK を提供しており、これらを使用してネイティブ GraphDB サービスと通信できます：

- **Node.js SDK**: JavaScript/TypeScript アプリケーション用
- **Python SDK**: Python アプリケーション用
- **Rust クライアント**: Rust アプリケーション用

すべての SDK は JSON-RPC プロトコルを使用して TCP 経由で aira-graphdb サービスと通信します。

## 1. Node.js SDK (@aira/graphdb-sdk)

### 1.1 インストール

**npm レジストリから：**

```bash
npm install @aira/graphdb-sdk
```

**ローカルリポジトリから：**

```bash
cd sdk/node
npm install
npm link  # オプション：開発用

# または親ディレクトリから：
cd /path/to/aira-graphdb
npm install sdk/node
```

### 1.2 基本的な使用方法

**ハンドシェイクとプロトコルネゴシエーション：**

```javascript
import { createHandshakeRequest, loadTypeMapContract, loadErrorCodeContract } from "@aira/graphdb-sdk";

// ハンドシェイクリクエストを作成
const handshakeReq = createHandshakeRequest(
  "protocol-p0@1.0.0",
  "canonical-types@1.0.0"
);

// コントラクトをロード
const typeMapContract = loadTypeMapContract();
const errorCodeContract = loadErrorCodeContract();

console.log("ハンドシェイク:", handshakeReq);
console.log("型システム:", typeMapContract.spec_id);
```

**エラーハンドリング：**

```javascript
import { mapKnownError } from "@aira/graphdb-sdk";

// エラーコードを既知のエラーにマップ
const error = mapKnownError("INVALID_QUERY", "Query parsing failed");
console.log(error);  // { code: "INVALID_QUERY", message: "Query parsing failed" }
```

### 1.3 GraphDB サービスへの接続

**TCP 接続例：**

```javascript
import net from "net";
import { createHandshakeRequest } from "@aira/graphdb-sdk";

const client = net.createConnection({ port: 3001, host: "localhost" });

client.on("connect", () => {
  const handshakeReq = createHandshakeRequest(
    "protocol-p0@1.0.0",
    "canonical-types@1.0.0"
  );
  client.write(JSON.stringify(handshakeReq) + "\n");
});

client.on("data", (data) => {
  console.log("レスポンス:", data.toString());
  client.end();
});

client.on("error", (err) => {
  console.error("接続エラー:", err);
});
```

### 1.4 テスト

```bash
cd sdk/node
npm test
```

## 2. Python SDK (aira-graphdb-sdk)

### 2.1 インストール

**ローカルリポジトリから：**

```bash
# 開発モード
cd /path/to/aira-graphdb/sdk/python
pip install -e .

# または標準インストール
pip install .
```

**配布版（将来）：**

```bash
pip install aira-graphdb-sdk
```

### 2.2 基本的な使用方法

**ハンドシェイクとプロトコルネゴシエーション：**

```python
from aira_graphdb_sdk import (
    create_handshake_request,
    load_typemap_contract,
    load_error_contract,
)

# ハンドシェイクリクエストを作成
handshake_req = create_handshake_request(
    "protocol-p0@1.0.0",
    "canonical-types@1.0.0"
)

# コントラクトをロード
typemap = load_typemap_contract()
errors = load_error_contract()

print(f"ハンドシェイク: {handshake_req}")
print(f"型システム: {typemap['spec_id']}")
```

**エラーハンドリング：**

```python
from aira_graphdb_sdk import map_known_error

# エラーコードを既知のエラーにマップ
error = map_known_error("INVALID_QUERY", "Query parsing failed")
print(error)  # {'code': 'INVALID_QUERY', 'message': 'Query parsing failed'}
```

### 2.3 GraphDB サービスへの接続

**ソケット接続例：**

```python
import socket
import json
from aira_graphdb_sdk import create_handshake_request

def connect_to_graphdb(host: str = "localhost", port: int = 3001):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    
    try:
        sock.connect((host, port))
        
        # ハンドシェイク送信
        handshake = create_handshake_request(
            "protocol-p0@1.0.0",
            "canonical-types@1.0.0"
        )
        sock.sendall(json.dumps(handshake).encode() + b"\n")
        
        # レスポンス受信
        response = sock.recv(4096).decode()
        print(f"レスポンス: {response}")
        
    except Exception as e:
        print(f"接続エラー: {e}")
    finally:
        sock.close()

# 接続
connect_to_graphdb()
```

### 2.4 テスト

```bash
cd sdk/python
pip install pytest
pytest tests/
```

## 3. Rust クライアント

### 3.1 インストール

**Cargo.toml に追加：**

```toml
[dependencies]
aira_graphdb = { path = "../aira-graphdb" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

**またはレジストリから（将来）：**

```toml
[dependencies]
aira_graphdb = "0.1"
```

### 3.2 基本的な使用方法

**ハンドシェイクとプロトコルネゴシエーション：**

```rust
use aira_graphdb::protocol::{HandshakeRequest, negotiate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handshake = HandshakeRequest {
        protocol_version: "protocol-p0@1.0.0".into(),
        canonical_type_system_version: "canonical-types@1.0.0".into(),
    };
    
    let response = negotiate(&handshake)?;
    println!("ネゴシエーション完了: {}", response.accepted);
    
    Ok(())
}
```

**クエリ実行：**

```rust
use aira_graphdb::graph::InMemoryGraphStore;
use aira_graphdb::query::execute_query;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryGraphStore::new();
    
    // ノード作成
    execute_query(&mut store, "CREATE (n:Paper {title:'GraphDB'})")?;
    
    // ノードクエリ
    let results = execute_query(&mut store, "MATCH (n:Paper) RETURN n")?;
    println!("結果: {:?}", results);
    
    Ok(())
}
```

**エラーハンドリング：**

```rust
use aira_graphdb::errors::GraphDbError;

fn handle_query_error(error: GraphDbError) {
    match error {
        GraphDbError::ClientError { code, message } => {
            eprintln!("クライアントエラー [{}]: {}", code, message);
        }
        GraphDbError::InvalidQuery { query, reason } => {
            eprintln!("無効なクエリ: {} ({})", query, reason);
        }
        _ => eprintln!("GraphDB エラー: {}", error),
    }
}
```

### 3.3 ライブラリの使用

**Cargo プロジェクトで：**

```bash
# 新しいプロジェクト作成
cargo new my-graphdb-app
cd my-graphdb-app

# 依存関係追加
cargo add aira_graphdb

# または Cargo.toml を手動編集
```

**ビルド：**

```bash
cargo build
cargo run
```

**テスト：**

```bash
cargo test
```

## 4. 接続詳細

### 4.1 サービスエンドポイント

aira-graphdb ネイティブサービスのリッスンポイント：

- **デフォルト**: `localhost:3001`
- **設定方法**: `AGDB_PORT` 環境変数
- **プロトコル**: TCP 上の JSON-RPC

### 4.2 ハンドシェイクプロトコル

すべてのクライアントはハンドシェイクで初期化する必要があります：

```json
{
  "type": "handshake",
  "protocol_version": "protocol-p0@1.0.0",
  "canonical_type_system_version": "canonical-types@1.0.0"
}
```

レスポンス：

```json
{
  "accepted": true,
  "protocol_version": "protocol-p0@1.0.0",
  "server_version": "aira-graphdb-native 0.1.2"
}
```

### 4.3 認証

ハンドシェイク後、認証リクエストを送信：

```json
{
  "type": "auth",
  "bearer_token": "jwt-token-here"
}
```

## 5. 一般的な操作

### 5.1 ノード・エッジ管理

**ノード操作（JSON-RPC 経由）：**

```json
{"id":1,"method":"upsert_nodes","params":{"nodes":[{"nodeId":"n1","corpusId":"c1","layer":"paper","label":"Paper"}]}}
{"id":2,"method":"get_node","params":{"nodeId":"n1"}}
{"id":3,"method":"delete_nodes","params":{"nodeIds":["n1"]}}
```

### 5.2 ベクトル検索

```json
{"id":4,"method":"vector_upsert","params":{"vectors":[{"id":"v1","corpusId":"c1","namespace":"default","values":[0.1,0.2,0.3]}]}}
{"id":5,"method":"vector_search","params":{"corpusId":"c1","namespace":"default","queryVector":[0.1,0.2,0.3],"topK":10}}
```

### 5.3 テキスト検索

```json
{"id":6,"method":"lexical_index_passages","params":{"passages":[{"passageId":"p1","corpusId":"c1","text":"graph database"}]}}
{"id":7,"method":"lexical_search","params":{"corpusId":"c1","query":"graph database","topK":10}}
```

## 6. トラブルシューティング

### 接続を拒否されました

```bash
# サービスが実行中か確認
ps aux | grep aira-graphdb-native

# サービスを開始（必要な場合）
cargo run --bin aira-graphdb-native -- --port 3001
```

### ハンドシェイク失敗

- プロトコルバージョンを確認: `protocol-p0@1.0.0`
- 型システムバージョン確認: `canonical-types@1.0.0`
- [エラーハンドリングガイド](error-handling.ja.md) でエラーコード確認

### SDK が見つかりません

**Node.js:**
```bash
npm install @aira/graphdb-sdk
# または
npm install /path/to/aira-graphdb/sdk/node
```

**Python:**
```bash
pip install -e /path/to/aira-graphdb/sdk/python
```

**Rust:**
```toml
[dependencies]
aira_graphdb = { path = "../../aira-graphdb" }
```

## 7. SDK リファレンス

### Node.js API

| 関数 | 戻り値 | 用途 |
|------|-------|------|
| `createHandshakeRequest(protocol, version)` | Object | ハンドシェイクペイロード作成 |
| `loadTypeMapContract()` | Object | 型システムコントラクト読み込み |
| `loadErrorCodeContract()` | Object | エラーコード読み込み |
| `mapKnownError(code, message)` | Object | 既知エラーまたは UNSUPPORTED_FEATURE にマップ |

### Python API

| 関数 | 戻り値 | 用途 |
|------|-------|------|
| `create_handshake_request(protocol, version)` | dict | ハンドシェイクペイロード作成 |
| `load_typemap_contract()` | dict | 型システムコントラクト読み込み |
| `load_error_contract()` | dict | エラーコード読み込み |
| `map_known_error(code, message)` | dict | 既知エラーまたは UNSUPPORTED_FEATURE にマップ |

### Rust API

| モジュール | 主要型 | 用途 |
|-----------|--------|------|
| `protocol` | `HandshakeRequest`, `negotiate()` | プロトコル処理 |
| `query` | `execute_query()`, `QueryPlan` | クエリ実行 |
| `graph` | `InMemoryGraphStore` | グラフストレージ |
| `errors` | `GraphDbError` | エラーハンドリング |

## 8. 次のステップ

1. **インストール**: 使用言語を選択して SDK をインストール
2. **検証**: SDK テストを実行
3. **接続**: aira-graphdb サービスに接続
4. **操作**: 上記の使用例を参照
5. **トラブルシューティング**: 問題が発生した場合は [トラブルシューティングガイド](troubleshooting.ja.md) を参照

詳細な操作例については、[利用ガイド](usage-guide.ja.md) を参照してください。
