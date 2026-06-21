use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;

use crate::errors::{ErrorCode, GraphDbError};

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub allowed_algorithms: Vec<String>,
    pub expected_issuer: String,
    pub expected_audience: String,
    pub known_kids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct JwtHeader {
    alg: String,
    kid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JwtClaims {
    iss: String,
    aud: String,
    exp: u64,
    nbf: Option<u64>,
}

pub fn validate_bearer_token(config: &AuthConfig, token: &str) -> Result<(), GraphDbError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(
            GraphDbError::new(ErrorCode::AuthFailed, "token must have 3 segments")
                .with_detail("auth_reason", "token_segments"),
        );
    }
    if parts[2].is_empty() {
        return Err(GraphDbError::new(ErrorCode::AuthFailed, "empty signature")
            .with_detail("auth_reason", "empty_signature"));
    }

    let header: JwtHeader = decode_segment(parts[0])?;
    let claims: JwtClaims = decode_segment(parts[1])?;

    if header.alg == "none"
        || !config
            .allowed_algorithms
            .iter()
            .any(|alg| alg == &header.alg)
    {
        return Err(
            GraphDbError::new(ErrorCode::AuthFailed, "token algorithm not allowed")
                .with_detail("auth_reason", "algorithm_not_allowed"),
        );
    }

    let kid = header
        .kid
        .ok_or_else(|| GraphDbError::new(ErrorCode::AuthFailed, "missing kid"))?;
    let kid_count = config
        .known_kids
        .iter()
        .filter(|known| *known == &kid)
        .count();
    if kid_count != 1 {
        return Err(
            GraphDbError::new(ErrorCode::AuthFailed, "kid unresolved or non-unique")
                .with_detail("auth_reason", "kid_unresolved"),
        );
    }

    if claims.iss != config.expected_issuer {
        return Err(GraphDbError::new(ErrorCode::AuthFailed, "issuer mismatch")
            .with_detail("auth_reason", "issuer_mismatch"));
    }
    if claims.aud != config.expected_audience {
        return Err(
            GraphDbError::new(ErrorCode::AuthFailed, "audience mismatch")
                .with_detail("auth_reason", "audience_mismatch"),
        );
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_secs();
    if now >= claims.exp {
        return Err(GraphDbError::new(ErrorCode::AuthFailed, "token expired")
            .with_detail("auth_reason", "token_expired"));
    }
    if let Some(nbf) = claims.nbf
        && now < nbf
    {
        return Err(
            GraphDbError::new(ErrorCode::AuthFailed, "token not yet valid")
                .with_detail("auth_reason", "token_not_yet_valid"),
        );
    }

    Ok(())
}

fn decode_segment<T: for<'de> Deserialize<'de>>(segment: &str) -> Result<T, GraphDbError> {
    let bytes = URL_SAFE_NO_PAD.decode(segment).map_err(|_| {
        GraphDbError::new(ErrorCode::AuthFailed, "base64 decode failed")
            .with_detail("auth_reason", "base64_decode_failed")
    })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        GraphDbError::new(ErrorCode::AuthFailed, "json decode failed")
            .with_detail("auth_reason", "json_decode_failed")
    })
}

#[cfg(test)]
fn encode_segment(map: std::collections::HashMap<&str, serde_json::Value>) -> String {
    let json = serde_json::to_vec(&map).expect("json");
    URL_SAFE_NO_PAD.encode(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config() -> AuthConfig {
        AuthConfig {
            allowed_algorithms: vec!["RS256".to_string()],
            expected_issuer: "https://issuer.example".to_string(),
            expected_audience: "aira-graphdb".to_string(),
            known_kids: vec!["k1".to_string()],
        }
    }

    fn token(alg: &str, kid: &str, exp_offset: i64) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs();
        let header = encode_segment(HashMap::from([
            ("alg", serde_json::Value::String(alg.to_string())),
            ("kid", serde_json::Value::String(kid.to_string())),
        ]));
        let claims = encode_segment(HashMap::from([
            (
                "iss",
                serde_json::Value::String("https://issuer.example".to_string()),
            ),
            ("aud", serde_json::Value::String("aira-graphdb".to_string())),
            (
                "exp",
                serde_json::Value::Number(serde_json::Number::from(
                    (now as i64 + exp_offset) as u64,
                )),
            ),
            (
                "nbf",
                serde_json::Value::Number(serde_json::Number::from(now.saturating_sub(1))),
            ),
        ]));
        format!("{header}.{claims}.signature")
    }

    #[test]
    fn accepts_valid_token() {
        let cfg = config();
        let jwt = token("RS256", "k1", 300);
        validate_bearer_token(&cfg, &jwt).expect("valid token");
    }

    #[test]
    fn rejects_alg_none() {
        let cfg = config();
        let jwt = token("none", "k1", 300);
        let err = validate_bearer_token(&cfg, &jwt).expect_err("alg none must fail");
        assert_eq!(err.code, ErrorCode::AuthFailed);
    }

    #[test]
    fn rejects_unknown_kid() {
        let cfg = config();
        let jwt = token("RS256", "k2", 300);
        let err = validate_bearer_token(&cfg, &jwt).expect_err("unknown kid");
        assert_eq!(err.code, ErrorCode::AuthFailed);
    }
}
