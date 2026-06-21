use std::path::PathBuf;

use crate::auth::{AuthConfig, validate_bearer_token};
use crate::errors::{ErrorCode, GraphDbError};
use crate::lock::{WriteLockGuard, acquire_write_lock};
use crate::protocol::{HandshakeRequest, negotiate};
use crate::storage::StorageEngine;
use crate::tx::TransactionManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentMode {
    Embedded,
    Server,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub mode: DeploymentMode,
    pub db_file: PathBuf,
    pub port: Option<u16>,
    pub concurrency_profile: Option<String>,
}

pub struct EmbeddedRuntime {
    _lock: WriteLockGuard,
    pub tx_manager: TransactionManager,
}

pub struct ServerRuntime {
    _lock: WriteLockGuard,
    pub tx_manager: TransactionManager,
    pub config: RuntimeConfig,
    pub auth: AuthConfig,
}

impl EmbeddedRuntime {
    pub fn open(db_file: impl Into<PathBuf>) -> Result<Self, GraphDbError> {
        let db_file = db_file.into();
        let lock = acquire_write_lock(&db_file)?;
        let storage = StorageEngine::new(db_file);
        let tx_manager = TransactionManager::new(storage)?;
        Ok(Self {
            _lock: lock,
            tx_manager,
        })
    }
}

impl ServerRuntime {
    pub fn start(config: RuntimeConfig, auth: AuthConfig) -> Result<Self, GraphDbError> {
        if config.mode != DeploymentMode::Server {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                "server runtime requires SERVER mode",
            ));
        }
        validate_server_profile(config.concurrency_profile.as_deref(), 32)?;
        let lock = acquire_write_lock(&config.db_file)?;
        let storage = StorageEngine::new(config.db_file.clone());
        let tx_manager = TransactionManager::new(storage)?;
        Ok(Self {
            _lock: lock,
            tx_manager,
            config,
            auth,
        })
    }

    pub fn handshake(&self, request: HandshakeRequest) -> Result<(), GraphDbError> {
        let _ = negotiate(&request)?;
        Ok(())
    }

    pub fn authorize(&self, bearer_token: Option<&str>) -> Result<(), GraphDbError> {
        let token = bearer_token.ok_or_else(|| {
            GraphDbError::new(
                ErrorCode::AuthRequired,
                "authentication required before query execution",
            )
        })?;
        validate_bearer_token(&self.auth, token)
    }
}

fn validate_server_profile(profile: Option<&str>, active_connections: usize) -> Result<(), GraphDbError> {
    if profile == Some("P0-SERVER-CONCURRENCY") && active_connections < 32 {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "P0-SERVER-CONCURRENCY requires at least 32 concurrent connections",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::auth::AuthConfig;

    fn temp_db() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("agdb-runtime-{nanos}.db"))
    }

    fn auth_config() -> AuthConfig {
        AuthConfig {
            allowed_algorithms: vec!["RS256".to_string()],
            expected_issuer: "https://issuer.example".to_string(),
            expected_audience: "aira-graphdb".to_string(),
            known_kids: vec!["k1".to_string()],
        }
    }

    #[test]
    fn embedded_runtime_opens() {
        let db = temp_db();
        let runtime = EmbeddedRuntime::open(&db).expect("open embedded");
        assert_eq!(runtime.tx_manager.current_graph().node_count(), 0);
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(db.with_extension("wal"));
    }

    #[test]
    fn server_runtime_enforces_profile_minimum() {
        let db = temp_db();
        let config = RuntimeConfig {
            mode: DeploymentMode::Server,
            db_file: db.clone(),
            port: Some(7687),
            concurrency_profile: Some("P0-SERVER-CONCURRENCY".to_string()),
        };
        let err = validate_server_profile(config.concurrency_profile.as_deref(), 16)
            .expect_err("profile min should fail");
        assert_eq!(err.code, ErrorCode::UnsupportedFeature);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn server_runtime_requires_auth() {
        let db = temp_db();
        let config = RuntimeConfig {
            mode: DeploymentMode::Server,
            db_file: db.clone(),
            port: Some(7687),
            concurrency_profile: None,
        };
        let runtime = ServerRuntime::start(config, auth_config()).expect("start");
        let err = runtime.authorize(None).expect_err("must require auth");
        assert_eq!(err.code, ErrorCode::AuthRequired);
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(db.with_extension("wal"));
    }
}
