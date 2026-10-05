use astrea_sda_api::{create_router, repository::SatelliteRepository};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt; // for `oneshot`

#[tokio::test]
async fn test_full_satellite_crud_and_overhead() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    // 0. Login to obtain JWT Bearer Token
    let login_payload = json!({
        "email": "admin@astrea.local",
        "password": "password123"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&login_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let token = auth_json["token"].as_str().unwrap();

    // 1a. Attempt to create satellite without Authorization header -> Expect 401 Unauthorized
    let create_payload = json!({
        "name": "ATLAS CENTAUR 2",
        "tleLineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "tleLineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 1b. Create satellite with valid JWT Authorization header -> Expect 201 Created
    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let sat_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let sat_id = sat_json["id"].as_str().unwrap().to_string();

    assert_eq!(sat_json["name"], "ATLAS CENTAUR 2");
    assert_eq!(
        sat_json["tle"]["lineOne"],
        "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994"
    );

    // 2a. Attempt to get paginated satellites without Authorization -> Expect 401 Unauthorized
    let req = Request::builder()
        .method("GET")
        .uri("/v1/satellites")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 2b. Get paginated list of satellites with Bearer auth
    let req = Request::builder()
        .method("GET")
        .uri("/v1/satellites")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(list_json["data"].as_array().unwrap().len(), 1);
    assert_eq!(list_json["pagination"]["limit"], 20);

    // 3a. Attempt to get satellite by ID without Authorization -> Expect 401 Unauthorized
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/satellites/{}", sat_id))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 3b. Get satellite by ID with Bearer auth
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/satellites/{}", sat_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 4a. Attempt to test overhead calculation without Authorization -> Expect 401 Unauthorized
    let req = Request::builder()
        .method("GET")
        .uri("/v1/overhead-satellites?lat=13.923&lon=177.315&time=2021-08-27T16:00:00Z")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 4b. Test canonical overhead satellite endpoint /v1/overhead-satellites with Bearer auth
    let req = Request::builder()
        .method("GET")
        .uri("/v1/overhead-satellites?lat=13.923&lon=177.315&time=2021-08-27T16:00:00Z")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let overhead_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(overhead_json["elevation"].is_number());
    assert_eq!(overhead_json["satellite"]["name"], "ATLAS CENTAUR 2");

    // 5a. Delete satellite without authorization -> Expect 401 Unauthorized
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/satellites/{}", sat_id))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 5b. Delete satellite with valid JWT Authorization header -> Expect 204 No Content
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/satellites/{}", sat_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_openapi_and_swagger_ui_endpoints() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    // 1. Verify OpenAPI JSON endpoint
    let req = Request::builder()
        .method("GET")
        .uri("/api-docs/openapi.json")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let openapi_json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(openapi_json["openapi"].as_str().unwrap().starts_with("3."));
    assert!(openapi_json["paths"]["/v1/satellites"].is_object());
    assert!(openapi_json["paths"]["/v1/satellites/{id}"].is_object());
    assert!(openapi_json["paths"]["/v1/overhead-satellites"].is_object());
    assert!(openapi_json["paths"]["/v1/pipelines/sync"].is_object());
    assert!(openapi_json["components"]["schemas"]["Satellite"].is_object());
    assert!(openapi_json["components"]["schemas"]["Tle"].is_object());

    // 2. Verify Swagger UI endpoint
    let req = Request::builder()
        .method("GET")
        .uri("/swagger-ui/")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert!(response.status().is_success() || response.status().is_redirection());

    // 3. Verify Root GET / redirects to Swagger UI
    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap(),
        "/swagger-ui/"
    );

    // 4. Verify GET /docs returns Redoc HTML page
    let req = Request::builder()
        .method("GET")
        .uri("/docs")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("redoc"));
}

#[tokio::test]
async fn test_api_v1_route_alias() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    // Verify /api/v1/auth/login route alias
    let login_payload = json!({
        "email": "admin@astrea.local",
        "password": "password123"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&login_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let token = auth_json["token"].as_str().unwrap();

    // Verify /api/v1/satellites route alias without auth -> Expect 401 Unauthorized
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/satellites")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Verify /api/v1/satellites route alias with auth -> Expect 200 OK
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/satellites")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
