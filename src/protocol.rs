use crate::contracts::load_typemap_spec;
use crate::errors::{ErrorCode, GraphDbError};

#[derive(Debug, Clone)]
pub struct HandshakeRequest {
    pub protocol_version: String,
    pub canonical_type_system_version: String,
}

#[derive(Debug, Clone)]
pub struct HandshakeResponse {
    pub accepted: bool,
    pub protocol_version: Option<String>,
    pub canonical_type_system_version: Option<String>,
}

pub fn negotiate(request: &HandshakeRequest) -> Result<HandshakeResponse, GraphDbError> {
    let spec = load_typemap_spec();
    let protocol_ok = spec
        .protocol_versions
        .iter()
        .any(|v| v == &request.protocol_version);
    let canonical_ok = spec
        .canonical_type_system_versions
        .iter()
        .any(|v| v == &request.canonical_type_system_version);

    if !protocol_ok || !canonical_ok {
        return Err(GraphDbError::new(
            ErrorCode::ProtocolVersionMismatch,
            "protocol or canonical type system version mismatch",
        ));
    }

    Ok(HandshakeResponse {
        accepted: true,
        protocol_version: Some(request.protocol_version.clone()),
        canonical_type_system_version: Some(request.canonical_type_system_version.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_versions() {
        let request = HandshakeRequest {
            protocol_version: "protocol-p0@1.0.0".to_string(),
            canonical_type_system_version: "canonical-types@1.0.0".to_string(),
        };
        let response = negotiate(&request).expect("supported versions");
        assert!(response.accepted);
    }

    #[test]
    fn rejects_unknown_versions() {
        let request = HandshakeRequest {
            protocol_version: "protocol-p0@2.0.0".to_string(),
            canonical_type_system_version: "canonical-types@1.0.0".to_string(),
        };
        let err = negotiate(&request).expect_err("must fail");
        assert_eq!(err.code, ErrorCode::ProtocolVersionMismatch);
    }
}
