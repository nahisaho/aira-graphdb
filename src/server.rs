use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde::{Deserialize, Serialize};

use crate::errors::GraphDbError;
use crate::protocol::HandshakeRequest;
use crate::query::{QueryResult, execute_query};
use crate::runtime::ServerRuntime;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerRequest {
    Ping,
    Handshake {
        protocol_version: String,
        canonical_type_system_version: String,
    },
    Auth {
        bearer_token: String,
    },
    BeginTx,
    Query {
        tx_id: String,
        query: String,
    },
    CommitTx {
        tx_id: String,
    },
    RollbackTx {
        tx_id: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerResponse {
    Pong,
    HandshakeOk {
        protocol_version: String,
        canonical_type_system_version: String,
    },
    AuthOk,
    TxBegun {
        tx_id: String,
    },
    QueryResult {
        result: QueryResult,
    },
    TxCommitted {
        tx_id: String,
    },
    TxRolledBack {
        tx_id: String,
    },
    Error {
        code: String,
        message: String,
        details: Option<std::collections::HashMap<String, String>>,
    },
}

pub struct ServerTransport {
    listener: TcpListener,
    runtime: Arc<Mutex<ServerRuntime>>,
}

#[derive(Debug, Default)]
struct ConnectionContext {
    state: SessionState,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    #[default]
    Connected,
    TlsOk,
    AuthOk,
    AppReady,
}

impl ServerTransport {
    pub fn bind(addr: &str, runtime: ServerRuntime) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            runtime: Arc::new(Mutex::new(runtime)),
        })
    }

    pub fn local_addr(&self) -> io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }

    pub fn run(self, max_connections: Option<usize>) -> io::Result<()> {
        let mut handles: Vec<JoinHandle<io::Result<()>>> = Vec::new();
        let mut accepted = 0usize;
        loop {
            if let Some(max) = max_connections && accepted >= max {
                break;
            }
            let (stream, _) = self.listener.accept()?;
            accepted += 1;
            let runtime = self.runtime.clone();
            handles.push(thread::spawn(move || handle_connection(stream, runtime)));
        }
        for handle in handles {
            let join_result = handle.join().map_err(|_| io::Error::other("server thread panic"))?;
            join_result?;
        }
        Ok(())
    }
}

fn handle_connection(stream: TcpStream, runtime: Arc<Mutex<ServerRuntime>>) -> io::Result<()> {
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    let mut ctx = ConnectionContext::default();

    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request = serde_json::from_str::<ServerRequest>(trimmed);
        let response = match request {
            Ok(req) => process_request(req, &runtime, &mut ctx),
            Err(err) => ServerResponse::Error {
                code: "UNSUPPORTED_FEATURE".to_string(),
                message: format!("invalid request payload: {err}"),
                details: None,
            },
        };
        write_response(&mut writer, &response)?;
    }

    Ok(())
}

fn process_request(
    request: ServerRequest,
    runtime: &Arc<Mutex<ServerRuntime>>,
    ctx: &mut ConnectionContext,
) -> ServerResponse {
    match request {
        ServerRequest::Ping => ServerResponse::Pong,
        ServerRequest::Handshake {
            protocol_version,
            canonical_type_system_version,
        } => {
            let mut_guard = runtime.lock();
            let mut_guard = match mut_guard {
                Ok(guard) => guard,
                Err(_) => return lock_poisoned_error(),
            };
            let req = HandshakeRequest {
                protocol_version: protocol_version.clone(),
                canonical_type_system_version: canonical_type_system_version.clone(),
            };
            match mut_guard.handshake(req) {
                Ok(()) => {
                    ctx.state = SessionState::TlsOk;
                    ServerResponse::HandshakeOk {
                        protocol_version,
                        canonical_type_system_version,
                    }
                }
                Err(err) => to_error_response(err),
            }
        }
        ServerRequest::Auth { bearer_token } => {
            if ctx.state != SessionState::TlsOk {
                record_audit(
                    runtime,
                    "AUTH_REQUIRED_REJECTED",
                    Some("AUTH_REQUIRED"),
                    "handshake required before auth",
                    None,
                    std::collections::HashMap::from([("request".to_string(), "auth".to_string())]),
                );
                return ServerResponse::Error {
                    code: "AUTH_REQUIRED".to_string(),
                    message: "handshake required before auth".to_string(),
                    details: None,
                };
            }
            let mut_guard = runtime.lock();
            let mut_guard = match mut_guard {
                Ok(guard) => guard,
                Err(_) => return lock_poisoned_error(),
            };
            match mut_guard.authorize(Some(&bearer_token)) {
                Ok(()) => {
                    ctx.state = SessionState::AuthOk;
                    ctx.state = SessionState::AppReady;
                    ServerResponse::AuthOk
                }
                Err(err) => {
                    let _ = mut_guard.tx_manager.record_audit_event(
                        "AUTH_FAILED",
                        Some(err.code.as_str()),
                        &err.message,
                        None,
                        std::collections::HashMap::new(),
                    );
                    to_error_response(err)
                }
            }
        }
        ServerRequest::BeginTx => {
            if ctx.state != SessionState::AppReady {
                record_audit(
                    runtime,
                    "AUTH_REQUIRED_REJECTED",
                    Some("AUTH_REQUIRED"),
                    "APP_READY required before begin_tx",
                    None,
                    std::collections::HashMap::from([("request".to_string(), "begin_tx".to_string())]),
                );
                return ServerResponse::Error {
                    code: "AUTH_REQUIRED".to_string(),
                    message: "APP_READY required before begin_tx".to_string(),
                    details: None,
                };
            }
            let mut guard = match runtime.lock() {
                Ok(guard) => guard,
                Err(_) => return lock_poisoned_error(),
            };
            let tx_id = guard.tx_manager.begin();
            ServerResponse::TxBegun { tx_id }
        }
        ServerRequest::Query { tx_id, query } => {
            if ctx.state != SessionState::AppReady {
                record_audit(
                    runtime,
                    "AUTH_REQUIRED_REJECTED",
                    Some("AUTH_REQUIRED"),
                    "APP_READY required before query",
                    None,
                    std::collections::HashMap::from([("request".to_string(), "query".to_string())]),
                );
                return ServerResponse::Error {
                    code: "AUTH_REQUIRED".to_string(),
                    message: "APP_READY required before query".to_string(),
                    details: None,
                };
            }
            let mut guard = match runtime.lock() {
                Ok(guard) => guard,
                Err(_) => return lock_poisoned_error(),
            };
            match guard
                .tx_manager
                .graph_mut(&tx_id)
                .and_then(|graph| execute_query(graph, &query))
            {
                Ok(result) => ServerResponse::QueryResult { result },
                Err(err) => {
                    audit_on_error_with_guard(&guard, &err, Some(&tx_id));
                    to_error_response(err)
                }
            }
        }
        ServerRequest::CommitTx { tx_id } => {
            if ctx.state != SessionState::AppReady {
                record_audit(
                    runtime,
                    "AUTH_REQUIRED_REJECTED",
                    Some("AUTH_REQUIRED"),
                    "APP_READY required before commit",
                    None,
                    std::collections::HashMap::from([("request".to_string(), "commit_tx".to_string())]),
                );
                return ServerResponse::Error {
                    code: "AUTH_REQUIRED".to_string(),
                    message: "APP_READY required before commit".to_string(),
                    details: None,
                };
            }
            let mut guard = match runtime.lock() {
                Ok(guard) => guard,
                Err(_) => return lock_poisoned_error(),
            };
            match guard.tx_manager.commit(&tx_id) {
                Ok(()) => ServerResponse::TxCommitted { tx_id },
                Err(err) => {
                    audit_on_error_with_guard(&guard, &err, Some(&tx_id));
                    to_error_response(err)
                }
            }
        }
        ServerRequest::RollbackTx { tx_id } => {
            if ctx.state != SessionState::AppReady {
                record_audit(
                    runtime,
                    "AUTH_REQUIRED_REJECTED",
                    Some("AUTH_REQUIRED"),
                    "APP_READY required before rollback",
                    None,
                    std::collections::HashMap::from([("request".to_string(), "rollback_tx".to_string())]),
                );
                return ServerResponse::Error {
                    code: "AUTH_REQUIRED".to_string(),
                    message: "APP_READY required before rollback".to_string(),
                    details: None,
                };
            }
            let mut guard = match runtime.lock() {
                Ok(guard) => guard,
                Err(_) => return lock_poisoned_error(),
            };
            match guard.tx_manager.rollback(&tx_id) {
                Ok(()) => ServerResponse::TxRolledBack { tx_id },
                Err(err) => {
                    audit_on_error_with_guard(&guard, &err, Some(&tx_id));
                    to_error_response(err)
                }
            }
        }
    }
}

fn write_response(writer: &mut TcpStream, response: &ServerResponse) -> io::Result<()> {
    let payload = serde_json::to_string(response)
        .map_err(|err| io::Error::other(format!("failed to serialize response: {err}")))?;
    writer.write_all(payload.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn to_error_response(error: GraphDbError) -> ServerResponse {
    ServerResponse::Error {
        code: error.code.as_str().to_string(),
        message: error.message,
        details: error.details,
    }
}

fn lock_poisoned_error() -> ServerResponse {
    ServerResponse::Error {
        code: "UNSUPPORTED_FEATURE".to_string(),
        message: "runtime lock poisoned".to_string(),
        details: None,
    }
}

fn audit_on_error_with_guard(guard: &ServerRuntime, err: &GraphDbError, tx_id: Option<&str>) {
    match err.code {
        crate::errors::ErrorCode::AuthFailed => {
            let _ = guard.tx_manager.record_audit_event(
                "AUTH_FAILED",
                Some(err.code.as_str()),
                &err.message,
                tx_id,
                std::collections::HashMap::new(),
            );
        }
        crate::errors::ErrorCode::AuthRequired => {
            let _ = guard.tx_manager.record_audit_event(
                "AUTH_REQUIRED_REJECTED",
                Some(err.code.as_str()),
                &err.message,
                tx_id,
                std::collections::HashMap::new(),
            );
        }
        crate::errors::ErrorCode::ReferentialIntegrityViolation => {
            let _ = guard.tx_manager.record_audit_event(
                "REFERENTIAL_INTEGRITY_VIOLATION",
                Some(err.code.as_str()),
                &err.message,
                tx_id,
                std::collections::HashMap::new(),
            );
        }
        crate::errors::ErrorCode::RetryableConflict => {
            let _ = guard.tx_manager.record_audit_event(
                "DETERMINISTIC_CONFLICT",
                Some(err.code.as_str()),
                &err.message,
                tx_id,
                std::collections::HashMap::new(),
            );
        }
        _ => {}
    }
}

fn record_audit(
    runtime: &Arc<Mutex<ServerRuntime>>,
    event_type: &str,
    code: Option<&str>,
    message: &str,
    tx_id: Option<&str>,
    payload: std::collections::HashMap<String, String>,
) {
    if let Ok(guard) = runtime.lock() {
        let _ = guard
            .tx_manager
            .record_audit_event(event_type, code, message, tx_id, payload);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::Value;

    use super::*;
    use crate::auth::AuthConfig;
    use crate::runtime::{DeploymentMode, RuntimeConfig};
    use crate::storage::StorageEngine;

    fn temp_db() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("agdb-server-{nanos}.db"))
    }

    fn auth_config() -> AuthConfig {
        AuthConfig {
            allowed_algorithms: vec!["RS256".to_string()],
            expected_issuer: "https://issuer.example".to_string(),
            expected_audience: "aira-graphdb".to_string(),
            known_kids: vec!["k1".to_string()],
        }
    }

    fn make_token() -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs();
        let header = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&HashMap::from([
                ("alg", Value::String("RS256".to_string())),
                ("kid", Value::String("k1".to_string())),
            ]))
            .expect("json"),
        );
        let claims = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&HashMap::from([
                ("iss", Value::String("https://issuer.example".to_string())),
                ("aud", Value::String("aira-graphdb".to_string())),
                ("exp", Value::Number((now + 300).into())),
                ("nbf", Value::Number((now.saturating_sub(1)).into())),
            ]))
            .expect("json"),
        );
        format!("{header}.{claims}.sig")
    }

    fn send(
        writer: &mut TcpStream,
        reader: &mut BufReader<TcpStream>,
        req: &ServerRequest,
    ) -> ServerResponse {
        let line = serde_json::to_string(req).expect("serialize");
        writer
            .write_all(format!("{line}\n").as_bytes())
            .expect("write");
        writer.flush().expect("flush");
        let mut out = String::new();
        reader.read_line(&mut out).expect("read response");
        serde_json::from_str(out.trim()).expect("response parse")
    }

    #[test]
    fn serves_handshake_auth_and_query() {
        let db = temp_db();
        let runtime = ServerRuntime::start(
            RuntimeConfig {
                mode: DeploymentMode::Server,
                db_file: db.clone(),
                port: None,
                concurrency_profile: Some("P0-SERVER-CONCURRENCY".to_string()),
            },
            auth_config(),
        )
        .expect("runtime");
        let transport = ServerTransport::bind("127.0.0.1:0", runtime).expect("bind");
        let addr = transport.local_addr().expect("addr");
        let handle = thread::spawn(move || transport.run(Some(1)));

        let mut stream = TcpStream::connect(addr).expect("connect");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));

        let handshake = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Handshake {
                protocol_version: "protocol-p0@1.0.0".to_string(),
                canonical_type_system_version: "canonical-types@1.0.0".to_string(),
            },
        );
        assert!(matches!(handshake, ServerResponse::HandshakeOk { .. }));

        let auth = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Auth {
                bearer_token: make_token(),
            },
        );
        assert!(matches!(auth, ServerResponse::AuthOk));

        let tx_begun = send(&mut stream, &mut reader, &ServerRequest::BeginTx);
        let tx_id = match tx_begun {
            ServerResponse::TxBegun { tx_id } => tx_id,
            _ => panic!("expected begin tx"),
        };

        let create = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Query {
                tx_id: tx_id.clone(),
                query: "CREATE (n:Paper)".to_string(),
            },
        );
        assert!(matches!(create, ServerResponse::QueryResult { .. }));

        let commit = send(
            &mut stream,
            &mut reader,
            &ServerRequest::CommitTx {
                tx_id: tx_id.clone(),
            },
        );
        assert!(matches!(commit, ServerResponse::TxCommitted { .. }));

        drop(reader);
        drop(stream);
        handle.join().expect("join").expect("serve ok");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(db.with_extension("wal"));
        let _ = std::fs::remove_file(db.with_extension("write.lock"));
    }

    #[test]
    fn rejects_tx_commands_without_auth() {
        let db = temp_db();
        let runtime = ServerRuntime::start(
            RuntimeConfig {
                mode: DeploymentMode::Server,
                db_file: db.clone(),
                port: None,
                concurrency_profile: Some("P0-SERVER-CONCURRENCY".to_string()),
            },
            auth_config(),
        )
        .expect("runtime");
        let transport = ServerTransport::bind("127.0.0.1:0", runtime).expect("bind");
        let addr = transport.local_addr().expect("addr");
        let handle = thread::spawn(move || transport.run(Some(1)));

        let mut stream = TcpStream::connect(addr).expect("connect");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let response = send(&mut stream, &mut reader, &ServerRequest::BeginTx);
        match response {
            ServerResponse::Error { code, .. } => assert_eq!(code, "AUTH_REQUIRED"),
            _ => panic!("expected auth error"),
        }

        drop(reader);
        drop(stream);
        handle.join().expect("join").expect("serve ok");
        let events = StorageEngine::new(&db).load_audit_events().expect("audit");
        assert!(events.iter().any(|e| e.event_type == "AUTH_REQUIRED_REJECTED"));
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(db.with_extension("wal"));
        let _ = std::fs::remove_file(db.with_extension("audit.log"));
        let _ = std::fs::remove_file(db.with_extension("write.lock"));
    }

    #[test]
    fn rejects_auth_before_handshake() {
        let db = temp_db();
        let runtime = ServerRuntime::start(
            RuntimeConfig {
                mode: DeploymentMode::Server,
                db_file: db.clone(),
                port: None,
                concurrency_profile: Some("P0-SERVER-CONCURRENCY".to_string()),
            },
            auth_config(),
        )
        .expect("runtime");
        let transport = ServerTransport::bind("127.0.0.1:0", runtime).expect("bind");
        let addr = transport.local_addr().expect("addr");
        let handle = thread::spawn(move || transport.run(Some(1)));

        let mut stream = TcpStream::connect(addr).expect("connect");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let response = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Auth {
                bearer_token: make_token(),
            },
        );
        match response {
            ServerResponse::Error { code, message, .. } => {
                assert_eq!(code, "AUTH_REQUIRED");
                assert!(message.contains("handshake required"));
            }
            _ => panic!("expected auth-required before handshake"),
        }

        drop(reader);
        drop(stream);
        handle.join().expect("join").expect("serve ok");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(db.with_extension("wal"));
        let _ = std::fs::remove_file(db.with_extension("audit.log"));
        let _ = std::fs::remove_file(db.with_extension("write.lock"));
    }

    #[test]
    fn records_auth_failed_event() {
        let db = temp_db();
        let runtime = ServerRuntime::start(
            RuntimeConfig {
                mode: DeploymentMode::Server,
                db_file: db.clone(),
                port: None,
                concurrency_profile: Some("P0-SERVER-CONCURRENCY".to_string()),
            },
            auth_config(),
        )
        .expect("runtime");
        let transport = ServerTransport::bind("127.0.0.1:0", runtime).expect("bind");
        let addr = transport.local_addr().expect("addr");
        let handle = thread::spawn(move || transport.run(Some(1)));

        let mut stream = TcpStream::connect(addr).expect("connect");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));

        let _ = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Handshake {
                protocol_version: "protocol-p0@1.0.0".to_string(),
                canonical_type_system_version: "canonical-types@1.0.0".to_string(),
            },
        );
        let auth = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Auth {
                bearer_token: "bad.token".to_string(),
            },
        );
        match auth {
            ServerResponse::Error { code, .. } => assert_eq!(code, "AUTH_FAILED"),
            _ => panic!("expected AUTH_FAILED"),
        }

        drop(reader);
        drop(stream);
        handle.join().expect("join").expect("serve ok");
        let events = StorageEngine::new(&db).load_audit_events().expect("audit");
        assert!(events.iter().any(|e| e.event_type == "AUTH_FAILED"));
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(db.with_extension("wal"));
        let _ = std::fs::remove_file(db.with_extension("audit.log"));
        let _ = std::fs::remove_file(db.with_extension("write.lock"));
    }

    #[test]
    fn records_referential_integrity_violation_event() {
        let db = temp_db();
        let runtime = ServerRuntime::start(
            RuntimeConfig {
                mode: DeploymentMode::Server,
                db_file: db.clone(),
                port: None,
                concurrency_profile: Some("P0-SERVER-CONCURRENCY".to_string()),
            },
            auth_config(),
        )
        .expect("runtime");
        let transport = ServerTransport::bind("127.0.0.1:0", runtime).expect("bind");
        let addr = transport.local_addr().expect("addr");
        let handle = thread::spawn(move || transport.run(Some(1)));

        let mut stream = TcpStream::connect(addr).expect("connect");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));

        let _ = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Handshake {
                protocol_version: "protocol-p0@1.0.0".to_string(),
                canonical_type_system_version: "canonical-types@1.0.0".to_string(),
            },
        );
        let _ = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Auth {
                bearer_token: make_token(),
            },
        );
        let tx_begun = send(&mut stream, &mut reader, &ServerRequest::BeginTx);
        let tx_id = match tx_begun {
            ServerResponse::TxBegun { tx_id } => tx_id,
            _ => panic!("expected begin tx"),
        };
        let response = send(
            &mut stream,
            &mut reader,
            &ServerRequest::Query {
                tx_id,
                query: "SET NODE n999 title='x'".to_string(),
            },
        );
        match response {
            ServerResponse::Error { code, .. } => assert_eq!(code, "REFERENTIAL_INTEGRITY_VIOLATION"),
            _ => panic!("expected referential integrity violation"),
        }

        drop(reader);
        drop(stream);
        handle.join().expect("join").expect("serve ok");
        let events = StorageEngine::new(&db).load_audit_events().expect("audit");
        assert!(events
            .iter()
            .any(|e| e.event_type == "REFERENTIAL_INTEGRITY_VIOLATION"));
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(db.with_extension("wal"));
        let _ = std::fs::remove_file(db.with_extension("audit.log"));
        let _ = std::fs::remove_file(db.with_extension("write.lock"));
    }
}
