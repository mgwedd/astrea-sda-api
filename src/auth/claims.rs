//! JWT Claims & Role-Based Access Control (RBAC) Module

use crate::auth::mtls::{verify_mtls_cert_binding, CnfClaim};
use crate::error::AppError;
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Modern OAuth 2.0 / OIDC Role Hierarchy for Satellite API RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    /// Read-only access to satellite data and astrodynamics calculations.
    Viewer,
    /// Read and write access to create/update satellites and trigger sync pipelines.
    Editor,
    /// Full administrative access including destructive deletion.
    Admin,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Viewer => "viewer",
            UserRole::Editor => "editor",
            UserRole::Admin => "admin",
        }
    }
}

/// JWT Claims payload structure containing standard OIDC and OAuth 2.0 claim fields.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    /// Subject (User ID, username, or client service ID)
    pub sub: String,
    /// Token issuer (OIDC issuer URI or auth server ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Token audience (target API audience)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration timestamp in seconds since Unix epoch
    pub exp: usize,
    /// Issued-at timestamp in seconds since Unix epoch
    pub iat: usize,
    /// User role ("viewer", "editor", or "admin")
    #[serde(default)]
    pub role: String,
    /// Optional list of secondary roles
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    /// Space-separated OAuth 2.0 scopes string (e.g. "read:satellites write:satellites admin:satellites")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// RFC 8705 Confirmation claim binding token to client X.509 certificate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cnf: Option<CnfClaim>,
}

impl Claims {
    /// Evaluates role hierarchy level (Admin: 3 > Editor/Operator: 2 > Viewer/Reader: 1).
    pub fn role_level(&self) -> u8 {
        match self.role.to_lowercase().as_str() {
            "admin" | "superuser" => 3,
            "editor" | "operator" | "writer" => 2,
            "viewer" | "reader" | "user" => 1,
            _ => 0,
        }
    }

    /// Role normalised to the canonical name for its level (`superuser` -> `admin`, `reader` -> `viewer`...),
    /// so per-role configuration matches the same aliases `role_level` accepts. Unknown roles pass through lowercased.
    pub fn canonical_role(&self) -> String {
        match self.role_level() {
            3 => "admin".to_string(),
            2 => "editor".to_string(),
            1 => "viewer".to_string(),
            _ => self.role.to_lowercase(),
        }
    }

    /// Checks whether caller has at least the minimum required role in the hierarchy.
    pub fn has_role(&self, minimum_role: UserRole) -> bool {
        let req_level = match minimum_role {
            UserRole::Admin => 3,
            UserRole::Editor => 2,
            UserRole::Viewer => 1,
        };
        self.role_level() >= req_level
    }

    /// Enforces minimum role requirement. Returns `AppError::Forbidden` if caller has insufficient privileges.
    pub fn require_role(&self, minimum_role: UserRole) -> Result<(), AppError> {
        if self.has_role(minimum_role) {
            Ok(())
        } else {
            Err(AppError::Forbidden(format!(
                "Action requires '{:?}' role or higher (caller has '{}')",
                minimum_role, self.role
            )))
        }
    }

    /// Checks if token contains a specific OAuth 2.0 scope.
    pub fn has_scope(&self, required_scope: &str) -> bool {
        if self.role_level() >= 3 {
            return true; // Admin bypasses scope check
        }
        if let Some(ref scope_str) = self.scope {
            scope_str.split_whitespace().any(|s| s == required_scope)
        } else {
            false
        }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".into()))?;

        let auth_header_trim = auth_header.trim();
        if auth_header_trim.len() < 7 || !auth_header_trim[..6].eq_ignore_ascii_case("bearer") {
            return Err(AppError::Unauthorized(
                "Invalid Authorization header scheme, expected 'Bearer <token>'".into(),
            ));
        }

        let token = auth_header_trim[6..].trim();
        if token.is_empty() {
            return Err(AppError::Unauthorized("Bearer token is empty".into()));
        }

        let claims = crate::auth::jwt::decode_jwt_token(token)?;

        // Enforce RFC 8705 mTLS Certificate Binding if `cnf` claim is present in token
        if let Some(ref cnf) = claims.cnf {
            verify_mtls_cert_binding(parts, cnf)?;
        }

        Ok(claims)
    }
}
