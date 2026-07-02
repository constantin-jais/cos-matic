//! Orchestration authorization model using Biscuit tokens (ADR: orchestration-authorization-model).
//!
//! Validates one-shot, depth-1 Biscuit delegation chains before accepting a
//! handoff. The verification happens in layers:
//! 1. Extract biscuit_authorization_references from the handoff.
//! 2. Load public key(s) that signed the delegation root.
//! 3. Validate the delegation chain and token expiration.
//! 4. Return a typed principal or a structured error.
//!
//! Verification is gated behind a rollout flag to avoid breaking existing
//! handoff flows during brownfield operation.

use serde::Serialize;
use std::fmt;
use thiserror::Error;

/// Authorization principal extracted from a validated Biscuit token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorizedPrincipal {
    pub user_id: String,
    pub tenant_id: String,
    pub delegated_to: String, // agent ID
    pub expires_at: String,   // ISO 8601
}

/// Errors that occur during authorization verification.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize)]
#[serde(tag = "kind")]
pub enum AuthorizationError {
    #[error("token signature invalid: {reason}")]
    InvalidSignature { reason: String },

    #[error("token expired at {expired_at}")]
    TokenExpired { expired_at: String },

    #[error("token missing required fact: {fact}")]
    MissingFact { fact: String },

    #[error("token delegation depth exceeds limit: depth={depth}")]
    DepthExceeded { depth: u32 },

    #[error("no authorization references found in handoff")]
    NoAuthorizationReferences,

    #[error("authorization verification disabled")]
    VerificationDisabled,

    #[error("unknown authorization error: {reason}")]
    Unknown { reason: String },
}

/// Rollout mode for authorization verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolloutMode {
    /// Skip verification, log a warning. Allows brownfield operation.
    Disabled,
    /// Run verification, log errors as warnings, accept handoff regardless.
    /// Permits gradual rollout.
    WarnOnly,
    /// Run verification, reject handoff on any error. Hard gate.
    Enforce,
}

impl fmt::Display for RolloutMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::WarnOnly => write!(f, "warn-only"),
            Self::Enforce => write!(f, "enforce"),
        }
    }
}

impl RolloutMode {
    /// Parse a rollout mode from a string. Defaults to WarnOnly if unrecognized.
    pub fn from_env() -> Self {
        match std::env::var("BOLT_COSMATIC_REQUIRE_DELEGATION")
            .as_deref()
            .unwrap_or("warn-only")
        {
            "enforce" => Self::Enforce,
            "disabled" => Self::Disabled,
            _ => Self::WarnOnly,
        }
    }
}

/// Trait for Biscuit token verification.
///
/// Abstracts the verification logic to allow real implementations using
/// biscuit-auth crate and testing stubs.
pub trait BiscuitAuthorizer: Send + Sync {
    /// Verify a Biscuit delegation chain and extract the authorized principal.
    ///
    /// # Errors
    /// Returns an AuthorizationError if the token is invalid, expired, or
    /// missing required facts.
    fn verify_delegation_chain(
        &self,
        token: &str,
        public_keys: &[Vec<u8>],
    ) -> Result<AuthorizedPrincipal, AuthorizationError>;
}

/// Stub implementation for testing.
///
/// Returns predictable results without requiring live token minting or key
/// material. Use this for development and test fixtures.
pub struct StubAuthorizer {
    /// If set, all verifications fail with this error.
    pub fail_with: Option<AuthorizationError>,
    /// If set, return this principal on successful verification.
    pub return_principal: Option<AuthorizedPrincipal>,
}

impl Default for StubAuthorizer {
    fn default() -> Self {
        Self {
            fail_with: None,
            return_principal: Some(AuthorizedPrincipal {
                user_id: "test-user".to_string(),
                tenant_id: "test-tenant".to_string(),
                delegated_to: "test-agent".to_string(),
                expires_at: "2026-12-31T23:59:59Z".to_string(),
            }),
        }
    }
}

impl BiscuitAuthorizer for StubAuthorizer {
    fn verify_delegation_chain(
        &self,
        _token: &str,
        _public_keys: &[Vec<u8>],
    ) -> Result<AuthorizedPrincipal, AuthorizationError> {
        if let Some(err) = &self.fail_with {
            return Err(err.clone());
        }

        self.return_principal
            .clone()
            .ok_or(AuthorizationError::Unknown {
                reason: "stub authorizer has no principal configured".to_string(),
            })
    }
}

/// Real Biscuit-based authorizer using biscuit-auth crate.
///
/// Validates depth-1 delegation chains: root authority → agent → this request.
/// Requires public key(s) to validate the signature.
pub struct BiscuitTokenAuthorizer;

impl BiscuitAuthorizer for BiscuitTokenAuthorizer {
    fn verify_delegation_chain(
        &self,
        _token: &str,
        _public_keys: &[Vec<u8>],
    ) -> Result<AuthorizedPrincipal, AuthorizationError> {
        // TODO(ADR: orchestration-authorization-model): Implement real Biscuit verification.
        //
        // This is a stub that documents the expected contract.
        // Full implementation requires:
        // 1. Decoding token from base64 or hex.
        // 2. Parsing it as a Biscuit token.
        // 3. Validating the signature against provided public keys.
        // 4. Extracting facts and checking delegation depth ≤ 1.
        //
        // For now, return an error to avoid false positives.
        Err(AuthorizationError::Unknown {
            reason: "Biscuit token verification not yet implemented; awaiting integration with rumble-lm and key provisioning"
                .to_string(),
        })
    }
}

/// Extract the authorized principal from a verified Biscuit token.
///
/// The token is passed as raw bytes here to avoid taking a hard dependency on
/// the `biscuit-auth` crate before real verification is wired. When rumble-lm
/// key provisioning lands, this signature becomes `&biscuit_auth::Biscuit` and
/// the crate is added back (with its advisory then fixed upstream or waived on
/// a real functional basis, not to hold a dead-code type).
#[allow(dead_code)]
fn extract_principal(_token_bytes: &[u8]) -> Result<AuthorizedPrincipal, AuthorizationError> {
    // In a full implementation, we would:
    // 1. Decode the Biscuit and verify its signature against the public keys.
    // 2. Query the token's facts for user_id, tenant_id, delegated_to.
    // 3. Extract expiration from the token checks.
    // 4. Validate delegation depth ≤ 1.
    //
    // For now, return a stub that documents the expected interface.
    Err(AuthorizationError::Unknown {
        reason: "fact extraction not yet implemented; awaiting integration with rumble-lm"
            .to_string(),
    })
}

/// Verify a handoff's authorization references using the provided authorizer.
///
/// Returns the extracted principal if verification succeeds (or is disabled).
/// If the rollout mode is Enforce and verification fails, returns an error.
/// If the rollout mode is WarnOnly, logs a warning but returns Ok.
pub fn verify_handoff_authorization(
    biscuit_references: Option<&str>,
    public_keys: &[Vec<u8>],
    authorizer: &dyn BiscuitAuthorizer,
    rollout_mode: RolloutMode,
) -> Result<Option<AuthorizedPrincipal>, AuthorizationError> {
    match rollout_mode {
        RolloutMode::Disabled => {
            // Skip verification. In production, log a warning here.
            Ok(None)
        }
        RolloutMode::WarnOnly => {
            match biscuit_references {
                Some(token) => {
                    // Attempt verification but don't fail.
                    match authorizer.verify_delegation_chain(token, public_keys) {
                        Ok(principal) => Ok(Some(principal)),
                        Err(_e) => {
                            // Log warning but accept handoff. In production:
                            // tracing::warn!("authorization verification failed: {}", e);
                            Ok(None)
                        }
                    }
                }
                None => {
                    // No authorization references; this is acceptable in warn-only mode.
                    Ok(None)
                }
            }
        }
        RolloutMode::Enforce => match biscuit_references {
            Some(token) => authorizer
                .verify_delegation_chain(token, public_keys)
                .map(Some),
            None => Err(AuthorizationError::NoAuthorizationReferences),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_authorizer_returns_default_principal() {
        let stub = StubAuthorizer::default();
        let result = stub.verify_delegation_chain("any-token", &[]);

        assert!(result.is_ok());
        let principal = result.unwrap();
        assert_eq!(principal.user_id, "test-user");
        assert_eq!(principal.tenant_id, "test-tenant");
    }

    #[test]
    fn stub_authorizer_returns_configured_error() {
        let stub = StubAuthorizer {
            fail_with: Some(AuthorizationError::TokenExpired {
                expired_at: "2026-01-01T00:00:00Z".to_string(),
            }),
            return_principal: None,
        };

        let result = stub.verify_delegation_chain("any-token", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn verify_handoff_authorization_disabled_mode() {
        let stub = StubAuthorizer::default();
        let result = verify_handoff_authorization(Some("token"), &[], &stub, RolloutMode::Disabled);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn verify_handoff_authorization_warn_only_mode_succeeds() {
        let stub = StubAuthorizer::default();
        let result = verify_handoff_authorization(Some("token"), &[], &stub, RolloutMode::WarnOnly);

        assert!(result.is_ok());
        let principal = result.unwrap();
        assert!(principal.is_some());
    }

    #[test]
    fn verify_handoff_authorization_warn_only_mode_ignores_errors() {
        let stub = StubAuthorizer {
            fail_with: Some(AuthorizationError::TokenExpired {
                expired_at: "2026-01-01T00:00:00Z".to_string(),
            }),
            return_principal: None,
        };

        let result = verify_handoff_authorization(Some("token"), &[], &stub, RolloutMode::WarnOnly);

        // In warn-only mode, errors are suppressed.
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn verify_handoff_authorization_enforce_mode_requires_token() {
        let stub = StubAuthorizer::default();
        let result = verify_handoff_authorization(None, &[], &stub, RolloutMode::Enforce);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err, AuthorizationError::NoAuthorizationReferences);
    }

    #[test]
    fn verify_handoff_authorization_enforce_mode_fails_on_error() {
        let stub = StubAuthorizer {
            fail_with: Some(AuthorizationError::TokenExpired {
                expired_at: "2026-01-01T00:00:00Z".to_string(),
            }),
            return_principal: None,
        };

        let result = verify_handoff_authorization(Some("token"), &[], &stub, RolloutMode::Enforce);

        assert!(result.is_err());
    }

    #[test]
    fn rollout_mode_from_env_defaults_to_warn_only() {
        // In test environment without BOLT_COSMATIC_REQUIRE_DELEGATION set,
        // expect WarnOnly.
        let mode = RolloutMode::from_env();
        assert_eq!(mode, RolloutMode::WarnOnly);
    }
}
