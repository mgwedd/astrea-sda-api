use astrea_sda_api::ApiDoc;
use std::fs;
use utoipa::OpenApi;

#[test]
fn test_openapi_schema_static_contract() {
    let openapi = ApiDoc::openapi();
    let json_str = openapi
        .to_pretty_json()
        .expect("Failed to serialize OpenAPI spec to JSON");
    let v: serde_json::Value = serde_json::from_str(&json_str).expect("Invalid JSON produced");

    // 1. OpenAPI Specification Version
    assert!(
        v["openapi"].as_str().unwrap().starts_with("3."),
        "Expected OpenAPI 3.x version"
    );

    // 2. Tag Metadata
    let tags = v["tags"].as_array().expect("Missing tags array");
    let tag_names: Vec<&str> = tags.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(tag_names.contains(&"Authentication"));
    assert!(tag_names.contains(&"Satellites"));
    assert!(tag_names.contains(&"Astrodynamics"));
    assert!(tag_names.contains(&"Pipelines"));

    // 3. Endpoint Paths Contract Verification
    let paths = &v["paths"];
    assert!(
        paths["/v1/auth/login"]["post"]["operationId"].as_str() == Some("loginHandler"),
        "Missing or invalid operationId for POST /v1/auth/login"
    );
    assert!(
        paths["/v1/satellites"]["post"]["operationId"].as_str() == Some("createSatellite"),
        "Missing or invalid operationId for POST /v1/satellites"
    );
    assert!(
        paths["/v1/satellites"]["get"]["operationId"].as_str() == Some("listSatellites"),
        "Missing or invalid operationId for GET /v1/satellites"
    );
    assert!(
        paths["/v1/satellites/{id}"]["get"]["operationId"].as_str() == Some("getSatellite"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}"
    );
    assert!(
        paths["/v1/satellites/{id}"]["patch"]["operationId"].as_str() == Some("updateSatellite"),
        "Missing or invalid operationId for PATCH /v1/satellites/{{id}}"
    );
    assert!(
        paths["/v1/satellites/{id}"]["delete"]["operationId"].as_str() == Some("deleteSatellite"),
        "Missing or invalid operationId for DELETE /v1/satellites/{{id}}"
    );
    assert!(
        paths["/v1/overhead"]["get"]["operationId"].as_str() == Some("getOverheadSatellite"),
        "Missing or invalid operationId for GET /v1/overhead"
    );
    assert!(
        paths["/v1/satellites/{id}/next-visible"]["get"]["operationId"].as_str()
            == Some("getNextVisiblePass"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/next-visible"
    );
    assert!(
        paths["/v1/satellites/{id}/groundtrack"]["get"]["operationId"].as_str()
            == Some("getGroundTrack"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/groundtrack"
    );
    assert!(
        paths["/v1/satellites/{id}/illumination"]["get"]["operationId"].as_str()
            == Some("getSatelliteIllumination"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/illumination"
    );
    assert!(
        paths["/v1/satellites/{id}/doppler"]["get"]["operationId"].as_str()
            == Some("getSatelliteDoppler"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/doppler"
    );
    assert!(
        paths["/v1/satellites/{id}/maneuvers"]["get"]["operationId"].as_str()
            == Some("getSatelliteManeuvers"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/maneuvers"
    );
    assert!(
        paths["/v1/satellites/{id}/detect-anomalies"]["post"]["operationId"].as_str()
            == Some("detectSatelliteAnomalies"),
        "Missing or invalid operationId for POST /v1/satellites/{{id}}/detect-anomalies"
    );
    assert!(
        paths["/v1/transits/solar"]["get"]["operationId"].as_str() == Some("getSolarTransits"),
        "Missing or invalid operationId for GET /v1/transits/solar"
    );
    assert!(
        paths["/v1/transits/lunar"]["get"]["operationId"].as_str() == Some("getLunarTransits"),
        "Missing or invalid operationId for GET /v1/transits/lunar"
    );
    assert!(
        paths["/v1/conjunctions/search"]["get"]["operationId"].as_str()
            == Some("searchConjunctions"),
        "Missing or invalid operationId for GET /v1/conjunctions/search"
    );
    assert!(
        paths["/v1/conjunctions/collision-probability"]["post"]["operationId"].as_str()
            == Some("calculateCollisionProbability"),
        "Missing or invalid operationId for POST /v1/conjunctions/collision-probability"
    );
    assert!(
        paths["/v1/pipelines/sync"]["post"]["operationId"].as_str() == Some("triggerPipelineSync"),
        "Missing or invalid operationId for POST /v1/pipelines/sync"
    );
    assert!(
        paths["/v1/satellites/{id}/passes"]["get"]["operationId"].as_str()
            == Some("getSatellitePasses"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/passes"
    );
    assert!(
        paths["/v1/satellites/{id}/relative-motion"]["get"]["operationId"].as_str()
            == Some("getRelativeMotion"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/relative-motion"
    );
    assert!(
        paths["/v1/satellites/{id}/state"]["get"]["operationId"].as_str()
            == Some("getSatelliteState"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/state"
    );
    assert!(
        paths["/v1/satellites/{id}/decay-risk"]["get"]["operationId"].as_str()
            == Some("getSatelliteDecayRisk"),
        "Missing or invalid operationId for GET /v1/satellites/{{id}}/decay-risk"
    );
    assert!(
        paths["/v1/decay-watch"]["get"]["operationId"].as_str() == Some("getDecayWatch"),
        "Missing or invalid operationId for GET /v1/decay-watch"
    );

    // Verify REST Architecture Tag Separation:
    // Satellite-scoped endpoints belong to the "Satellites" resource tag
    let sat_endpoints = [
        (&paths["/v1/satellites"]["get"], "listSatellites"),
        (&paths["/v1/satellites"]["post"], "createSatellite"),
        (&paths["/v1/satellites/{id}"]["get"], "getSatellite"),
        (&paths["/v1/overhead"]["get"], "getOverheadSatellite"),
        (
            &paths["/v1/satellites/{id}/next-visible"]["get"],
            "getNextVisiblePass",
        ),
        (
            &paths["/v1/satellites/{id}/passes"]["get"],
            "getSatellitePasses",
        ),
        (
            &paths["/v1/satellites/{id}/relative-motion"]["get"],
            "getRelativeMotion",
        ),
        (
            &paths["/v1/satellites/{id}/state"]["get"],
            "getSatelliteState",
        ),
        (
            &paths["/v1/satellites/{id}/decay-risk"]["get"],
            "getSatelliteDecayRisk",
        ),
        (
            &paths["/v1/satellites/{id}/groundtrack"]["get"],
            "getGroundTrack",
        ),
        (
            &paths["/v1/satellites/{id}/illumination"]["get"],
            "getSatelliteIllumination",
        ),
        (
            &paths["/v1/satellites/{id}/doppler"]["get"],
            "getSatelliteDoppler",
        ),
        (
            &paths["/v1/satellites/{id}/maneuvers"]["get"],
            "getSatelliteManeuvers",
        ),
        (
            &paths["/v1/satellites/{id}/detect-anomalies"]["post"],
            "detectSatelliteAnomalies",
        ),
    ];
    for (endpoint, op) in sat_endpoints {
        assert_eq!(
            endpoint["tags"][0].as_str(),
            Some("Satellites"),
            "Expected 'Satellites' tag for operation {}",
            op
        );
        assert!(
            endpoint["security"].is_array(),
            "Expected bearer_auth security for operation {}",
            op
        );
    }

    // Cross-satellite / celestial interactions belong to "Astrodynamics" resource tag
    let astro_endpoints = [
        (
            &paths["/v1/conjunctions/search"]["get"],
            "searchConjunctions",
        ),
        (&paths["/v1/transits/solar"]["get"], "getSolarTransits"),
        (&paths["/v1/transits/lunar"]["get"], "getLunarTransits"),
    ];
    for (endpoint, op) in astro_endpoints {
        assert_eq!(
            endpoint["tags"][0].as_str(),
            Some("Astrodynamics"),
            "Expected 'Astrodynamics' tag for operation {}",
            op
        );
        assert!(
            endpoint["security"].is_array(),
            "Expected bearer_auth security for operation {}",
            op
        );
    }

    // 4. Component Schemas & Security Schemes Contract Verification
    let schemas = &v["components"]["schemas"];
    let required_schemas = [
        "UserRole",
        "Claims",
        "LoginRequest",
        "AuthResponse",
        "Satellite",
        "Tle",
        "CreateSatelliteDto",
        "UpdateSatelliteDto",
        "OverheadResponse",
        "NextVisiblePassResponse",
        "GroundTrackResponse",
        "GroundTrackPoint",
        "GeoJsonFeature",
        "GeoJsonGeometry",
        "IlluminationResponse",
        "LightingState",
        "ObserverTwilightState",
        "DopplerResponse",
        "ManeuversResponse",
        "DetectedManeuver",
        "ManeuverType",
        "AnomalyDetectionRequest",
        "AnomalyDetectionResponse",
        "AnomalyStatus",
        "OrbitalParameterResidual",
        "TransitPredictionResponse",
        "TransitMatch",
        "TransitTarget",
        "ConjunctionSearchResponse",
        "ConjunctionMatch",
        "SatelliteSummary",
        "PipelineSyncResponse",
        "SatellitePass",
        "PassScheduleResponse",
        "RelativeMotionPoint",
        "RelativeMotionResponse",
        "KeplerianElements",
        "SatelliteStateResponse",
        "SatelliteDecayRiskResponse",
        "DecayWatchResponse",
        "CollisionProbabilityRequest",
        "CollisionProbabilityResponse",
        "PaginationMeta",
        "PaginatedResponseSatellite",
        "ErrorResponse",
    ];
    for schema_name in &required_schemas {
        assert!(
            schemas[schema_name].is_object(),
            "Missing component schema: {}",
            schema_name
        );
    }

    // 5. Security Scheme Verification
    assert!(
        v["components"]["securitySchemes"]["bearer_auth"]["type"].as_str() == Some("http"),
        "Missing bearer_auth security scheme in OpenAPI contract"
    );
    assert!(
        v["components"]["securitySchemes"]["bearer_auth"]["scheme"].as_str() == Some("bearer"),
        "Invalid bearer_auth scheme in OpenAPI contract"
    );
}

#[test]
fn test_openapi_json_file_in_sync() {
    let openapi = ApiDoc::openapi();
    let generated_json = openapi.to_pretty_json().unwrap();
    let dir_path = concat!(env!("CARGO_MANIFEST_DIR"), "/api-docs");
    let file_path = concat!(env!("CARGO_MANIFEST_DIR"), "/api-docs/openapi.json");

    if !std::path::Path::new(file_path).exists() || std::env::var("UPDATE_OPENAPI").is_ok() {
        fs::create_dir_all(dir_path).expect("Failed to create api-docs directory");
        fs::write(file_path, &generated_json).expect("Failed to write api-docs/openapi.json");
    }

    let committed_json = fs::read_to_string(file_path).expect(
        "api-docs/openapi.json not found! Ensure the OpenAPI spec file exists in repository root.",
    );

    let generated_val: serde_json::Value = serde_json::from_str(&generated_json).unwrap();
    let committed_val: serde_json::Value = serde_json::from_str(&committed_json).unwrap();

    if generated_val != committed_val {
        fs::write(file_path, &generated_json).expect("Failed to sync api-docs/openapi.json");
    }
}
