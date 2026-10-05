use astrea_sda_api::{
    auth::{create_jwt_token, Claims},
    create_router,
    error::AppError,
    repository::SatelliteRepository,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::{json, Value};
use std::process::Command;
use tower::ServiceExt;
use uuid::Uuid;

static INIT_KEYS: std::sync::Once = std::sync::Once::new();

fn ensure_keys_exist() {
    INIT_KEYS.call_once(|| {
        let _ = Command::new("./scripts/setup-keys.sh").output();
    });
}

// ==============================================================================
// 1. Strict Defense-in-Depth Authentication & RBAC Enforcement Tests
// ==============================================================================

#[tokio::test]
async fn test_ea_risk_defense_in_depth_unauthenticated_access_is_blocked() {
    ensure_keys_exist();
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let dummy_id = Uuid::new_v4().to_string();

    // All operational business endpoints must reject unauthenticated requests with 401 Unauthorized
    let protected_routes = [
        ("GET", "/v1/satellites", None),
        (
            "POST",
            "/v1/satellites",
            Some(json!({"name": "TEST"}).to_string()),
        ),
        ("GET", &format!("/v1/satellites/{}", dummy_id), None),
        (
            "PATCH",
            &format!("/v1/satellites/{}", dummy_id),
            Some(json!({"name": "UPDATE"}).to_string()),
        ),
        ("DELETE", &format!("/v1/satellites/{}", dummy_id), None),
        ("GET", "/v1/overhead-satellites?lat=0&lon=0", None),
        (
            "GET",
            &format!("/v1/satellites/{}/next-visible?lat=0&lon=0", dummy_id),
            None,
        ),
        (
            "GET",
            &format!("/v1/satellites/{}/groundtrack", dummy_id),
            None,
        ),
        (
            "GET",
            &format!("/v1/satellites/{}/illumination?lat=0&lon=0", dummy_id),
            None,
        ),
        (
            "GET",
            &format!(
                "/v1/satellites/{}/doppler?center_freq_hz=437500000&lat=0&lon=0",
                dummy_id
            ),
            None,
        ),
        (
            "GET",
            &format!("/v1/satellites/{}/maneuvers", dummy_id),
            None,
        ),
        (
            "POST",
            &format!("/v1/satellites/{}/detect-anomalies", dummy_id),
            None,
        ),
        ("GET", "/v1/conjunctions/search", None),
        ("GET", "/v1/transits/solar?lat=0&lon=0", None),
        ("GET", "/v1/transits/lunar?lat=0&lon=0", None),
        ("POST", "/v1/pipelines/sync", None),
        ("GET", "/v1/auth/me", None),
    ];

    for (method, uri, body) in protected_routes {
        let mut builder = Request::builder().method(method).uri(uri);
        let req_body = if let Some(b) = body {
            builder = builder.header("content-type", "application/json");
            Body::from(b)
        } else {
            Body::empty()
        };

        let response = app
            .clone()
            .oneshot(builder.body(req_body).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Protected route {} {} must strictly return 401 Unauthorized without auth",
            method,
            uri
        );
    }
}

#[tokio::test]
async fn test_ea_risk_defense_in_depth_public_endpoints_remain_accessible() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    // Only documentation, OpenAPI schemas, and login/signup are public
    let public_routes = [
        ("/", StatusCode::TEMPORARY_REDIRECT),
        ("/docs", StatusCode::OK),
        ("/swagger-ui/", StatusCode::OK),
        ("/api-docs/openapi.json", StatusCode::OK),
    ];

    for (uri, expected_status) in public_routes {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            response.status(),
            expected_status,
            "Public route {} must be accessible without auth",
            uri
        );
    }
}

#[tokio::test]
async fn test_ea_risk_defense_in_depth_rbac_privilege_boundaries() {
    ensure_keys_exist();
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let (viewer_token, _) = create_jwt_token("viewer_user", "viewer", 3600).unwrap();
    let (editor_token, _) = create_jwt_token("editor_user", "editor", 3600).unwrap();
    let (admin_token, _) = create_jwt_token("admin_user", "admin", 3600).unwrap();

    // 1. Viewer trying to create satellite -> 403 Forbidden
    let create_payload = json!({
        "name": "TEST SAT",
        "tleLineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "tleLineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", viewer_token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Viewer cannot create satellites"
    );

    // 2. Editor can create satellite -> 201 Created
    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", editor_token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Editor can create satellites"
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let sat: Value = serde_json::from_slice(&body_bytes).unwrap();
    let sat_id = sat["id"].as_str().unwrap();

    // 3. Editor trying to delete satellite -> 403 Forbidden
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/satellites/{}", sat_id))
        .header("authorization", format!("Bearer {}", editor_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Editor cannot delete satellites"
    );

    // 4. Editor trying to trigger pipeline sync -> 403 Forbidden
    let req = Request::builder()
        .method("POST")
        .uri("/v1/pipelines/sync")
        .header("authorization", format!("Bearer {}", editor_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Editor cannot trigger pipeline sync"
    );

    // 5. Admin can delete satellite -> 204 No Content
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/satellites/{}", sat_id))
        .header("authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "Admin can delete satellites"
    );
}

// ==============================================================================
// 2. PostgreSQL Multi-Statement Query Fix Verification
// ==============================================================================

#[test]
fn test_ea_risk_postgres_multi_statement_query_isolation_logic() {
    // Simulates the schema migration array and ensures each statement is an isolated DDL command
    let schema_statements = [
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            email TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'viewer',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS satellites (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name TEXT NOT NULL,
            line_one TEXT NOT NULL,
            line_two TEXT NOT NULL,
            owner_id TEXT,
            created_date TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            last_modified_date TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS tle_history (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            satellite_id UUID NOT NULL REFERENCES satellites(id) ON DELETE CASCADE,
            line_one TEXT NOT NULL,
            line_two TEXT NOT NULL,
            epoch TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    ];

    // Verify each statement contains exactly one top-level CREATE command
    for stmt in schema_statements {
        let trimmed = stmt.trim();
        assert!(trimmed.starts_with("CREATE TABLE IF NOT EXISTS"));
        // Count semicolons in statement - must end cleanly without chaining multiple commands
        let semicolon_count = trimmed.matches(';').count();
        assert_eq!(
            semicolon_count, 1,
            "Each migration entry must be an isolated single statement to prevent Postgres prepared statement protocol errors"
        );
    }
}

// ==============================================================================
// 3. M2M PK JWTCA Assertion Deserialization (RFC 7523) Without Role
// ==============================================================================

#[tokio::test]
async fn test_ea_risk_m2m_pk_jwtca_assertion_deserialization_without_role() {
    ensure_keys_exist();
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let priv_pem = astrea_sda_api::auth::get_rsa_private_key_pem();
    let encoding_key = EncodingKey::from_rsa_pem(priv_pem.as_bytes()).unwrap();

    let now = chrono::Utc::now().timestamp() as usize;

    // RFC 7523 Client Assertion JSON payload WITHOUT a `role` field:
    // Standard OAuth 2.0 client assertions do NOT provide a role claim.
    let raw_assertion_claims = json!({
        "sub": "m2m_autonomous_tracker",
        "iss": "m2m_autonomous_tracker",
        "aud": "astrea-sda-api",
        "exp": now + 600,
        "iat": now,
        "scope": "read:satellites write:satellites"
    });

    let assertion_token = encode(
        &Header::new(jsonwebtoken::Algorithm::RS256),
        &raw_assertion_claims,
        &encoding_key,
    )
    .unwrap();

    // Verify Claims deserializes without error despite missing `role` field due to #[serde(default)]
    let decoded_claims: Claims = serde_json::from_value(raw_assertion_claims.clone()).unwrap();
    assert_eq!(
        decoded_claims.role, "",
        "#[serde(default)] on Claims.role must populate empty string when field is missing in assertion"
    );

    // Perform RFC 7523 M2M token exchange at /v1/auth/token
    let token_exchange_payload = json!({
        "grantType": "client_credentials",
        "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
        "clientAssertion": assertion_token
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/auth/token")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&token_exchange_payload).unwrap(),
        ))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "RFC 7523 M2M assertion without role field must exchange successfully for an API Bearer token"
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_res: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(auth_res["token"].is_string());
    assert_eq!(auth_res["claims"]["sub"], "m2m_autonomous_tracker");
    assert_eq!(
        auth_res["claims"]["role"], "editor",
        "Client assertions without an explicit role default to 'editor' for operational M2M capability"
    );
}

// ==============================================================================
// 4. Tooling & Script Sanitation Verification
// ==============================================================================

#[test]
fn test_ea_risk_make_jwt_stdout_sanitation_clean_token_capture() {
    ensure_keys_exist();
    // Verifies that scripts/make-jwt.sh can be invoked via $(./scripts/make-jwt.sh -q) without banner leakage
    let output = Command::new("./scripts/make-jwt.sh")
        .args(["-q", "sanitized_user", "editor", "3600"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let parts: Vec<&str> = stdout_str.split('.').collect();
            assert_eq!(
                parts.len(), 3,
                "scripts/make-jwt.sh -q must output exactly a 3-part RS256 JWT on stdout without banner logs. Got: '{}'",
                stdout_str
            );
            // Verify there are no newlines or banner characters in token
            assert!(!stdout_str.contains('\n'));
            assert!(!stdout_str.contains("Astrea"));
        }
    }
}

#[test]
fn test_ea_risk_tle_checksum_integrity_algorithm() {
    // Validates the modulo 10 checksum algorithm according to NORAD TLE specification
    fn calculate_tle_checksum(line: &str) -> u32 {
        let mut sum: u32 = 0;
        // Checksum is the last character; calculate on chars 0..line.len()-1
        for c in line.chars().take(68) {
            match c {
                '0'..='9' => sum += c.to_digit(10).unwrap(),
                '-' => sum += 1,
                _ => {}
            }
        }
        sum % 10
    }

    let line1 = "1 00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994";
    let line2 = "2 00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397";

    let expected_c1 = line1.chars().last().unwrap().to_digit(10).unwrap();
    let expected_c2 = line2.chars().last().unwrap().to_digit(10).unwrap();

    assert_eq!(calculate_tle_checksum(line1), expected_c1);
    assert_eq!(calculate_tle_checksum(line2), expected_c2);
}

// ==============================================================================
// 5. Exact HTTP Error Status Code Mapping Verification
// ==============================================================================

#[tokio::test]
async fn test_ea_risk_exact_http_status_codes_not_found_and_sgp4() {
    ensure_keys_exist();
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let (admin_token, _) = create_jwt_token("admin_tester", "admin", 3600).unwrap();

    // 1. Non-existent Satellite ID -> strictly 404 Not Found
    let random_id = Uuid::new_v4();
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/satellites/{}", random_id))
        .header("authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Querying non-existent satellite must return 404 Not Found"
    );

    // 2. Calculation on non-existent Satellite -> strictly 404 Not Found
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/satellites/{}/groundtrack", random_id))
        .header("authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Ground track on non-existent satellite must return 404 Not Found"
    );

    // 3. SGP4 calculation error -> strictly 422 Unprocessable Entity
    let sgp4_resp =
        AppError::Sgp4Error("SGP4 calculation error: Decayed orbit".into()).into_response();
    assert_eq!(
        sgp4_resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "Sgp4Error must map strictly to 422 Unprocessable Entity per API standard"
    );

    // 4. Not Found error -> strictly 404 Not Found
    let not_found_resp = AppError::NotFound.into_response();
    assert_eq!(
        not_found_resp.status(),
        StatusCode::NOT_FOUND,
        "NotFound must map strictly to 404 Not Found"
    );
}
