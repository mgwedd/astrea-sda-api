use astrea_sda_api::{
    auth::{create_jwt_token, get_rsa_private_key_pem, Claims},
    create_router,
    repository::SatelliteRepository,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn test_auth_security_rejects_hs256_algorithm_confusion() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: "attacker".to_string(),
        iss: None,
        aud: None,
        exp: now + 3600,
        iat: now,
        role: "admin".to_string(),
        roles: Some(vec!["admin".to_string()]),
        scope: Some("read:satellites write:satellites admin:satellites".to_string()),
        cnf: None,
    };

    // Attacker crafts token using HS256 algorithm with a secret key
    let hs256_token = encode(
        &Header::default(), // Defaults to HS256
        &claims,
        &EncodingKey::from_secret(b"attacker_hmac_secret_key"),
    )
    .unwrap();

    let create_payload = json!({
        "name": "EXPLOIT SATELLITE",
        "tleLineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "tleLineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", hs256_token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "API must reject HS256 tokens when RS256 algorithm is strictly required"
    );
}

#[tokio::test]
async fn test_auth_security_rejects_expired_and_tampered_rs256_tokens() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let now = chrono::Utc::now().timestamp() as usize;
    let expired_claims = Claims {
        sub: "expired_user".to_string(),
        iss: None,
        aud: None,
        exp: now - 300, // Expired 5 minutes ago
        iat: now - 3600,
        role: "admin".to_string(),
        roles: Some(vec!["admin".to_string()]),
        scope: Some("read:satellites write:satellites admin:satellites".to_string()),
        cnf: None,
    };

    let private_pem = get_rsa_private_key_pem();
    let encoding_key = EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap();
    let expired_token = encode(
        &Header::new(jsonwebtoken::Algorithm::RS256),
        &expired_claims,
        &encoding_key,
    )
    .unwrap();

    let create_payload = json!({
        "name": "EXPIRED SATELLITE",
        "tleLineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "tleLineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
    });

    // 1. Expired token request -> 401 Unauthorized
    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", expired_token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 2. Tampered signature token -> 401 Unauthorized
    let (valid_token, _) = create_jwt_token("user", "admin", 3600).unwrap();
    let tampered_token = format!("{}tampered", valid_token);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", tampered_token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_security_viewer_role_permissions() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let (viewer_token, _) = create_jwt_token("viewer_user", "viewer", 3600).unwrap();

    // 1. Viewer can access GET /v1/satellites -> 200 OK
    let req = Request::builder()
        .method("GET")
        .uri("/v1/satellites")
        .header("authorization", format!("Bearer {}", viewer_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 2. Viewer attempts POST /v1/satellites -> 403 Forbidden
    let create_payload = json!({
        "name": "VIEWER SAT",
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
        "Viewer role must be forbidden from creating satellites"
    );

    // 3. Viewer attempts POST /v1/pipelines/sync -> 403 Forbidden
    let req = Request::builder()
        .method("POST")
        .uri("/v1/pipelines/sync?group=stations")
        .header("authorization", format!("Bearer {}", viewer_token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Viewer role must be forbidden from triggering pipeline sync"
    );
}

#[tokio::test]
async fn test_auth_security_editor_and_admin_role_permissions() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let (editor_token, _) = create_jwt_token("editor_user", "editor", 3600).unwrap();

    // 1. Create a satellite as Editor -> 201 Created
    let create_payload = json!({
        "name": "EDITOR TEST SAT",
        "tleLineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "tleLineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", editor_token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let sat_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let sat_id = sat_json["id"].as_str().unwrap();

    // 2. Update satellite as Editor -> 200 OK
    let update_payload = json!({
        "name": "UPDATED BY EDITOR"
    });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/v1/satellites/{}", sat_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", editor_token))
        .body(Body::from(serde_json::to_vec(&update_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 3. Editor attempts DELETE /v1/satellites/:id -> 403 Forbidden
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
        "Editor role must be forbidden from deleting satellites"
    );

    // 4. Admin attempts DELETE /v1/satellites/:id -> 204 No Content
    let (admin_token, _) = create_jwt_token("admin_user", "admin", 3600).unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/satellites/{}", sat_id))
        .header("authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_auth_security_case_insensitive_bearer_header() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let (admin_token, _) = create_jwt_token("admin_user", "admin", 3600).unwrap();

    let create_payload = json!({
        "name": "CASE INSENSITIVE SAT",
        "tleLineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "tleLineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
    });

    // Test lowercase "bearer <token>" scheme header
    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("bearer {}", admin_token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Case-insensitive 'bearer' prefix must be accepted per RFC 6750"
    );
}

#[tokio::test]
async fn test_auth_security_m2m_client_assertion_token_exchange() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let (assertion_token, _) = create_jwt_token("m2m_telemetry_worker", "editor", 300).unwrap();

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
        "RFC 7523 M2M Client Assertion Token Exchange must succeed and issue Bearer token"
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_res: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(auth_res["token"].is_string());
    assert_eq!(auth_res["claims"]["sub"], "m2m_telemetry_worker");
}

#[tokio::test]
async fn test_auth_security_mtls_certificate_bound_token_validation() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    let (assertion_token, _) = create_jwt_token("mtls_m2m_daemon", "editor", 300).unwrap();
    let client_cert_fingerprint =
        "8f3c7e91a02b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d";

    // 1. Exchange client assertion with X-Client-Cert-Fingerprint header -> Issues bound token
    let token_exchange_payload = json!({
        "grantType": "client_credentials",
        "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
        "clientAssertion": assertion_token
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/auth/token")
        .header("content-type", "application/json")
        .header("x-client-cert-fingerprint", client_cert_fingerprint)
        .body(Body::from(
            serde_json::to_vec(&token_exchange_payload).unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_res: Value = serde_json::from_slice(&body_bytes).unwrap();
    let bound_token = auth_res["token"].as_str().unwrap();

    // Verify cnf claim was attached
    assert_eq!(
        auth_res["claims"]["cnf"]["x5t#S256"],
        client_cert_fingerprint
    );

    // 2. Request using bound token with MATCHING client cert fingerprint -> 200 OK
    let req = Request::builder()
        .method("GET")
        .uri("/v1/auth/me")
        .header("authorization", format!("Bearer {}", bound_token))
        .header("x-client-cert-fingerprint", client_cert_fingerprint)
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "mTLS request with matching client cert fingerprint must succeed"
    );

    // 3. Request using bound token with MISMATCHED client cert fingerprint -> 401 Unauthorized
    let req = Request::builder()
        .method("GET")
        .uri("/v1/auth/me")
        .header("authorization", format!("Bearer {}", bound_token))
        .header("x-client-cert-fingerprint", "ATTACKER_CERT_FINGERPRINT")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "mTLS request with mismatched client cert fingerprint must be rejected"
    );

    // 4. Request using bound token without any client cert header -> 401 Unauthorized
    let req = Request::builder()
        .method("GET")
        .uri("/v1/auth/me")
        .header("authorization", format!("Bearer {}", bound_token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "mTLS request missing client cert fingerprint header must be rejected"
    );
}

#[tokio::test]
async fn test_unauthenticated_requests_are_rejected_except_login_and_docs() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    // 1. Documentation & Login Endpoints MUST be accessible without authentication
    let allowed_unauthed_endpoints = [
        ("GET", "/", None),
        ("GET", "/docs", None),
        ("GET", "/swagger-ui/", None),
        ("GET", "/api-docs/openapi.json", None),
        (
            "POST",
            "/v1/auth/login",
            Some(
                json!({
                    "email": "admin@astrea.local",
                    "password": "password123"
                })
                .to_string(),
            ),
        ),
    ];

    for (method, uri, body) in allowed_unauthed_endpoints {
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
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Endpoint {} {} must not return 401 Unauthorized without auth",
            method,
            uri
        );
    }

    // 2. All Protected Endpoints MUST strictly return 401 Unauthorized without authentication
    let dummy_id = "00000000-0000-0000-0000-000000000000";
    let protected_endpoints = [
        ("GET", "/v1/auth/me", None),
        ("GET", "/v1/satellites", None),
        (
            "POST",
            "/v1/satellites",
            Some(json!({"name": "Test"}).to_string()),
        ),
        ("GET", &format!("/v1/satellites/{}", dummy_id), None),
        (
            "PATCH",
            &format!("/v1/satellites/{}", dummy_id),
            Some(json!({"name": "Test"}).to_string()),
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
    ];

    for (method, uri, body) in protected_endpoints {
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
            "Protected endpoint {} {} MUST return 401 Unauthorized when missing Bearer token",
            method,
            uri
        );
    }
}
