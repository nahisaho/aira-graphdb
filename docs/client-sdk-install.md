# Client SDK Installation Guide (English)

## Overview

aira-graphdb provides client SDKs for three languages to interact with the native GraphDB service:

- **Node.js SDK**: For JavaScript/TypeScript applications
- **Python SDK**: For Python applications  
- **Rust Client**: For Rust applications

All SDKs communicate with the aira-graphdb service via JSON-RPC protocol over TCP.

## 1. Node.js SDK (@aira/graphdb-sdk)

### 1.1 Installation

**From npm registry:**

```bash
npm install @aira/graphdb-sdk
```

**From local repository:**

```bash
cd sdk/node
npm install
npm link  # Optional: for development

# Or from parent directory:
cd /path/to/aira-graphdb
npm install sdk/node
```

### 1.2 Basic Usage

**Handshake and Protocol Negotiation:**

```javascript
import { createHandshakeRequest, loadTypeMapContract, loadErrorCodeContract } from "@aira/graphdb-sdk";

// Create handshake request
const handshakeReq = createHandshakeRequest(
  "protocol-p0@1.0.0",
  "canonical-types@1.0.0"
);

// Load contracts
const typeMapContract = loadTypeMapContract();
const errorCodeContract = loadErrorCodeContract();

console.log("Handshake:", handshakeReq);
console.log("Type System:", typeMapContract.spec_id);
```

**Error Handling:**

```javascript
import { mapKnownError } from "@aira/graphdb-sdk";

// Map error code to known error
const error = mapKnownError("INVALID_QUERY", "Query parsing failed");
console.log(error);  // { code: "INVALID_QUERY", message: "Query parsing failed" }
```

### 1.3 Connecting to GraphDB Service

**TCP Connection Example:**

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
  console.log("Response:", data.toString());
  client.end();
});

client.on("error", (err) => {
  console.error("Connection error:", err);
});
```

### 1.4 Testing

```bash
cd sdk/node
npm test
```

## 2. Python SDK (aira-graphdb-sdk)

### 2.1 Installation

**From local repository:**

```bash
# Development mode
cd /path/to/aira-graphdb/sdk/python
pip install -e .

# Or standard installation
pip install .
```

**For distribution (future):**

```bash
pip install aira-graphdb-sdk
```

### 2.2 Basic Usage

**Handshake and Protocol Negotiation:**

```python
from aira_graphdb_sdk import (
    create_handshake_request,
    load_typemap_contract,
    load_error_contract,
)

# Create handshake request
handshake_req = create_handshake_request(
    "protocol-p0@1.0.0",
    "canonical-types@1.0.0"
)

# Load contracts
typemap = load_typemap_contract()
errors = load_error_contract()

print(f"Handshake: {handshake_req}")
print(f"Type System: {typemap['spec_id']}")
```

**Error Handling:**

```python
from aira_graphdb_sdk import map_known_error

# Map error code to known error
error = map_known_error("INVALID_QUERY", "Query parsing failed")
print(error)  # {'code': 'INVALID_QUERY', 'message': 'Query parsing failed'}
```

### 2.3 Connecting to GraphDB Service

**Socket Connection Example:**

```python
import socket
import json
from aira_graphdb_sdk import create_handshake_request

def connect_to_graphdb(host: str = "localhost", port: int = 3001):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    
    try:
        sock.connect((host, port))
        
        # Send handshake
        handshake = create_handshake_request(
            "protocol-p0@1.0.0",
            "canonical-types@1.0.0"
        )
        sock.sendall(json.dumps(handshake).encode() + b"\n")
        
        # Receive response
        response = sock.recv(4096).decode()
        print(f"Response: {response}")
        
    except Exception as e:
        print(f"Connection error: {e}")
    finally:
        sock.close()

# Connect
connect_to_graphdb()
```

### 2.4 Testing

```bash
cd sdk/python
pip install pytest
pytest tests/
```

## 3. Rust Client

### 3.1 Installation

**Add to Cargo.toml:**

```toml
[dependencies]
aira_graphdb = { path = "../aira-graphdb" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

**Or from registry (future):**

```toml
[dependencies]
aira_graphdb = "0.1"
```

### 3.2 Basic Usage

**Handshake and Protocol Negotiation:**

```rust
use aira_graphdb::protocol::{HandshakeRequest, negotiate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handshake = HandshakeRequest {
        protocol_version: "protocol-p0@1.0.0".into(),
        canonical_type_system_version: "canonical-types@1.0.0".into(),
    };
    
    let response = negotiate(&handshake)?;
    println!("Negotiation accepted: {}", response.accepted);
    
    Ok(())
}
```

**Query Execution:**

```rust
use aira_graphdb::graph::InMemoryGraphStore;
use aira_graphdb::query::execute_query;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryGraphStore::new();
    
    // Create node
    execute_query(&mut store, "CREATE (n:Paper {title:'GraphDB'})")?;
    
    // Query nodes
    let results = execute_query(&mut store, "MATCH (n:Paper) RETURN n")?;
    println!("Results: {:?}", results);
    
    Ok(())
}
```

**Error Handling:**

```rust
use aira_graphdb::errors::GraphDbError;

fn handle_query_error(error: GraphDbError) {
    match error {
        GraphDbError::ClientError { code, message } => {
            eprintln!("Client error [{}]: {}", code, message);
        }
        GraphDbError::InvalidQuery { query, reason } => {
            eprintln!("Invalid query: {} ({})", query, reason);
        }
        _ => eprintln!("GraphDB error: {}", error),
    }
}
```

### 3.3 Working with the Library

**In a Cargo project:**

```bash
# Create new project
cargo new my-graphdb-app
cd my-graphdb-app

# Add dependency
cargo add aira_graphdb

# Or edit Cargo.toml manually
```

**Building:**

```bash
cargo build
cargo run
```

**Testing:**

```bash
cargo test
```

## 4. Connection Details

### 4.1 Service Endpoint

The aira-graphdb native service listens on:

- **Default**: `localhost:3001`
- **Configurable via**: `AGDB_PORT` environment variable
- **Protocol**: JSON-RPC over TCP

### 4.2 Handshake Protocol

All clients must initiate with a handshake:

```json
{
  "type": "handshake",
  "protocol_version": "protocol-p0@1.0.0",
  "canonical_type_system_version": "canonical-types@1.0.0"
}
```

Response:

```json
{
  "accepted": true,
  "protocol_version": "protocol-p0@1.0.0",
  "server_version": "aira-graphdb-native 0.1.2"
}
```

### 4.3 Authentication

After handshake, send auth request:

```json
{
  "type": "auth",
  "bearer_token": "jwt-token-here"
}
```

## 5. Common Operations

### 5.1 Node and Edge Management

**Node Operations (via JSON-RPC):**

```json
{"id":1,"method":"upsert_nodes","params":{"nodes":[{"nodeId":"n1","corpusId":"c1","layer":"paper","label":"Paper"}]}}
{"id":2,"method":"get_node","params":{"nodeId":"n1"}}
{"id":3,"method":"delete_nodes","params":{"nodeIds":["n1"]}}
```

### 5.2 Vector Search

```json
{"id":4,"method":"vector_upsert","params":{"vectors":[{"id":"v1","corpusId":"c1","namespace":"default","values":[0.1,0.2,0.3]}]}}
{"id":5,"method":"vector_search","params":{"corpusId":"c1","namespace":"default","queryVector":[0.1,0.2,0.3],"topK":10}}
```

### 5.3 Lexical Search

```json
{"id":6,"method":"lexical_index_passages","params":{"passages":[{"passageId":"p1","corpusId":"c1","text":"graph database"}]}}
{"id":7,"method":"lexical_search","params":{"corpusId":"c1","query":"graph database","topK":10}}
```

## 6. Troubleshooting

### Connection refused

```bash
# Check service is running
ps aux | grep aira-graphdb-native

# Start service if needed
cargo run --bin aira-graphdb-native -- --port 3001
```

### Handshake failed

- Verify protocol version matches: `protocol-p0@1.0.0`
- Check type system version: `canonical-types@1.0.0`
- See [Error Handling Guide](error-handling.md) for error codes

### SDK not found

**Node.js:**
```bash
npm install @aira/graphdb-sdk
# or
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

## 7. SDK Reference

### Node.js API

| Function | Returns | Purpose |
|----------|---------|---------|
| `createHandshakeRequest(protocol, version)` | Object | Create handshake payload |
| `loadTypeMapContract()` | Object | Load type system contract |
| `loadErrorCodeContract()` | Object | Load error codes |
| `mapKnownError(code, message)` | Object | Map to known error or UNSUPPORTED_FEATURE |

### Python API

| Function | Returns | Purpose |
|----------|---------|---------|
| `create_handshake_request(protocol, version)` | dict | Create handshake payload |
| `load_typemap_contract()` | dict | Load type system contract |
| `load_error_contract()` | dict | Load error codes |
| `map_known_error(code, message)` | dict | Map to known error or UNSUPPORTED_FEATURE |

### Rust API

| Module | Key Types | Purpose |
|--------|-----------|---------|
| `protocol` | `HandshakeRequest`, `negotiate()` | Protocol handling |
| `query` | `execute_query()`, `QueryPlan` | Query execution |
| `graph` | `InMemoryGraphStore` | Graph storage |
| `errors` | `GraphDbError` | Error handling |

## 8. Next Steps

1. **Installation**: Choose your language and install the SDK
2. **Verification**: Run the SDK tests
3. **Connection**: Connect to your aira-graphdb service
4. **Operations**: See usage examples above
5. **Troubleshooting**: Refer to [Troubleshooting Guide](troubleshooting.md) if issues arise

For detailed operation examples, see the **[Usage Guide](usage-guide.md)**.
