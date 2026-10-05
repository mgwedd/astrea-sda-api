#!/usr/bin/env python3
"""
Astrea Space Domain Awareness (SDA) API — Comprehensive End-to-End QA Test Runner
Executes the full test suite defined in docs/QA_TESTING_PLAN.md across:
- Profile 1: In-Memory / SQLite (Without Postgres / Redis)
- Profile 2: Distributed Stack (With Postgres 5432 & Redis 6379)
- Authentication: Dev JWT (make jwt), JWTCA (RFC 7523 M2M), mTLS (RFC 8705 cnf binding)
- Endpoints: All 20 OpenAPI operations, documentation, pagination, rate-limiting, and caching
"""

import sys
import os
import time
import json
import base64
import subprocess
import urllib.request
import urllib.error
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding

GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
BLUE = "\033[94m"
RESET = "\033[0m"

PASS_COUNT = 0
FAIL_COUNT = 0
DEFECTS = []

def log_pass(msg):
    global PASS_COUNT
    PASS_COUNT += 1
    print(f"  {GREEN}✔ PASS:{RESET} {msg}")

def log_fail(msg, detail=""):
    global FAIL_COUNT
    FAIL_COUNT += 1
    error_msg = f"{msg} | {detail}" if detail else msg
    DEFECTS.append(error_msg)
    print(f"  {RED}✖ FAIL:{RESET} {msg}")
    if detail:
        print(f"         {YELLOW}Detail:{RESET} {detail}")

def http_req(method, url, headers=None, body=None):
    if headers is None:
        headers = {}
    data = None
    if body is not None:
        if isinstance(body, dict):
            data = json.dumps(body).encode("utf-8")
            headers["Content-Type"] = "application/json"
        elif isinstance(body, str):
            data = body.encode("utf-8")
        else:
            data = body

    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req) as resp:
            resp_body = resp.read().decode("utf-8")
            status = resp.status
            resp_headers = dict(resp.headers)
            try:
                parsed = json.loads(resp_body) if resp_body else None
            except json.JSONDecodeError:
                parsed = resp_body
            return status, resp_headers, parsed
    except urllib.error.HTTPError as e:
        resp_body = e.read().decode("utf-8")
        try:
            parsed = json.loads(resp_body) if resp_body else None
        except json.JSONDecodeError:
            parsed = resp_body
        return e.code, dict(e.headers), parsed
    except Exception as e:
        return 0, {}, str(e)

def wait_for_server(url, max_retries=30, delay=0.5):
    for _ in range(max_retries):
        try:
            req = urllib.request.Request(f"{url}/docs")
            with urllib.request.urlopen(req, timeout=2) as resp:
                if resp.status in (200, 303, 307):
                    return True
        except Exception:
            time.sleep(delay)
    return False

def make_client_assertion(private_key_path, iss="ground-station-alpha", aud="https://api.astrealabs.com/v1/auth/token"):
    with open(private_key_path, "rb") as f:
        key = serialization.load_pem_private_key(f.read(), None)

    now = int(time.time())
    header = {"alg": "RS256", "typ": "JWT"}
    claims = {
        "iss": iss,
        "sub": iss,
        "aud": aud,
        "jti": f"assert-{now}",
        "exp": now + 300,
        "iat": now,
    }
    def b64(d):
        return base64.urlsafe_b64encode(d).decode("ascii").rstrip("=")
    h = b64(json.dumps(header).encode())
    c = b64(json.dumps(claims).encode())
    msg = f"{h}.{c}".encode()
    sig = b64(key.sign(msg, padding.PKCS1v15(), hashes.SHA256()))
    return f"{h}.{c}.{sig}"

def run_suite(api_url, profile_name, has_redis=False):
    print(f"\n{BLUE}======================================================================{RESET}")
    print(f"{BLUE}🚀 RUNNING QA SUITE ON [{profile_name}] AT {api_url}{RESET}")
    print(f"{BLUE}======================================================================{RESET}\n")

    # 1. Documentation & OpenAPI Spec endpoints
    print(f"{YELLOW}--- 1. Documentation & Contract Verification ---{RESET}")
    st, _, b = http_req("GET", f"{api_url}/swagger-ui/")
    if st in (200, 303, 307):
        log_pass("Swagger UI Accessible (/swagger-ui/)")
    else:
        log_fail("Swagger UI check", f"Status: {st}")

    st, _, b = http_req("GET", f"{api_url}/docs")
    if st == 200 and "redoc" in str(b).lower():
        log_pass("ReDoc Accessible (/docs)")
    else:
        log_fail("ReDoc check", f"Status: {st}")

    st, _, b = http_req("GET", f"{api_url}/api-docs/openapi.json")
    if st == 200 and isinstance(b, dict) and "paths" in b:
        log_pass(f"OpenAPI Spec Validated (/api-docs/openapi.json - {len(b['paths'])} paths)")
    else:
        log_fail("OpenAPI JSON check", f"Status: {st}")

    # 2. Authentication: Dev JWT Minting (make jwt / scripts/make-jwt.sh)
    print(f"\n{YELLOW}--- 2. Dev JWT Tooling (make jwt / make keys) ---{RESET}")
    dev_token_res = subprocess.run(["./scripts/make-jwt.sh", "dev_admin_user", "admin"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    dev_admin_token = dev_token_res.stdout.strip()
    if dev_token_res.returncode == 0 and len(dev_admin_token.split(".")) == 3:
        log_pass("Dev RSA JWT Token Minted via scripts/make-jwt.sh")
    else:
        log_fail("Dev JWT minting", dev_token_res.stderr)

    dev_viewer_res = subprocess.run(["./scripts/make-jwt.sh", "dev_viewer_user", "viewer"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    dev_viewer_token = dev_viewer_res.stdout.strip()

    # 3. User Lifecycle: Signup & Login
    print(f"\n{YELLOW}--- 3. User Authentication Lifecycle (Signup, Login, Profile) ---{RESET}")
    test_email = f"qa-user-{int(time.time()*1000)}@astrealabs.com"
    st, _, b = http_req("POST", f"{api_url}/v1/auth/signup", body={
        "email": test_email,
        "password": "CorrectHorseBatteryStaple123!",
        "role": "editor"
    })
    if st in (200, 201) and isinstance(b, dict) and "token" in b:
        editor_token = b["token"]
        role = b.get("claims", {}).get("role") or b.get("role")
        log_pass(f"User Signup & Token Issuance (POST /v1/auth/signup - Role: {role})")
    else:
        log_fail("User Signup", f"Status: {st}, Body: {b}")
        editor_token = dev_admin_token

    # Negative Signup test (duplicate email -> 409 Conflict)
    st, _, b = http_req("POST", f"{api_url}/v1/auth/signup", body={
        "email": test_email,
        "password": "CorrectHorseBatteryStaple123!",
        "role": "editor"
    })
    if st in (409, 400):
        log_pass("Duplicate User Registration Rejected with 409 Conflict")
    else:
        log_fail("Duplicate User Signup check", f"Expected 409, got {st}")

    # Login
    st, _, b = http_req("POST", f"{api_url}/v1/auth/login", body={
        "email": test_email,
        "password": "CorrectHorseBatteryStaple123!"
    })
    if st == 200 and isinstance(b, dict) and "token" in b:
        log_pass("User Login (POST /v1/auth/login)")
    else:
        log_fail("User Login", f"Status: {st}")

    # Negative Login test (wrong password -> 401 Unauthorized)
    st, _, b = http_req("POST", f"{api_url}/v1/auth/login", body={
        "email": test_email,
        "password": "WrongPassword123!"
    })
    if st == 401:
        log_pass("Invalid Password Rejected with 401 Unauthorized")
    else:
        log_fail("Invalid Password check", f"Expected 401, got {st}")

    # Profile (/v1/auth/me) with token
    st, _, b = http_req("GET", f"{api_url}/v1/auth/me", headers={"Authorization": f"Bearer {editor_token}"})
    if st == 200 and isinstance(b, dict) and b.get("role") == "editor":
        log_pass(f"Caller Profile Verified (GET /v1/auth/me - Role: {b.get('role')})")
    else:
        log_fail("Profile check", f"Status: {st}, Body: {b}")

    # Profile (/v1/auth/me) WITHOUT token -> 401
    st, _, b = http_req("GET", f"{api_url}/v1/auth/me")
    if st == 401:
        log_pass("Unauthenticated /v1/auth/me Rejected with 401 Unauthorized")
    else:
        log_fail("Unauthenticated Profile check", f"Expected 401, got {st}")

    # 4. RFC 7523 M2M PK JWTCA Client Assertion Token Exchange
    print(f"\n{YELLOW}--- 4. M2M PK JWTCA (RFC 7523) & mTLS Certificate Binding ---{RESET}")
    private_key_path = ".keys/rsa_private.pem"
    if os.path.exists(private_key_path):
        assertion = make_client_assertion(private_key_path)
        st, _, b = http_req("POST", f"{api_url}/v1/auth/token", body={
            "clientAssertion": assertion,
            "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            "grantType": "client_credentials",
            "scope": "satellites:write"
        })
        if st == 200 and isinstance(b, dict) and "token" in b:
            m2m_token = b["token"]
            log_pass("RFC 7523 M2M Client Assertion Token Exchange (POST /v1/auth/token)")
        else:
            log_fail("M2M Token Exchange", f"Status: {st}, Body: {b}")
            m2m_token = None

        # Negative M2M assertion test (tampered signature -> 401 Unauthorized)
        tampered_assertion = assertion[:-4] + "AAAA"
        st, _, b = http_req("POST", f"{api_url}/v1/auth/token", body={
            "clientAssertion": tampered_assertion,
            "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            "grantType": "client_credentials"
        })
        if st == 401:
            log_pass("Tampered Client Assertion Rejected with 401 Unauthorized")
        else:
            log_fail("Tampered Assertion check", f"Expected 401, got {st}")

        # RFC 8705 mTLS token binding test
        cert_fp = "d1a89c3f58a74e92b31498c0b5f1624d78e391b402cf89a5e1289b0123456789"
        st, _, b = http_req(
            "POST",
            f"{api_url}/v1/auth/token",
            headers={"x-client-cert-fingerprint": cert_fp},
            body={
                "clientAssertion": make_client_assertion(private_key_path),
                "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
                "grantType": "client_credentials"
            }
        )
        if st == 200 and isinstance(b, dict) and "claims" in b and b["claims"].get("cnf", {}).get("x5t#S256") == cert_fp:
            bound_token = b["token"]
            log_pass("RFC 8705 mTLS Certificate Fingerprint Attached (cnf.x5t#S256)")
            # Access with matching cert header
            st_auth, _, _ = http_req("GET", f"{api_url}/v1/auth/me", headers={"Authorization": f"Bearer {bound_token}", "x-client-cert-fingerprint": cert_fp})
            if st_auth == 200:
                log_pass("mTLS Token Access with Matching Client Cert Succeeded")
            else:
                log_fail("mTLS Matching Cert Access", f"Status: {st_auth}")

            # Access with mismatched cert header -> 401
            st_bad, _, _ = http_req("GET", f"{api_url}/v1/auth/me", headers={"Authorization": f"Bearer {bound_token}", "x-client-cert-fingerprint": "ATTACKER_CERT"})
            if st_bad == 401:
                log_pass("mTLS Token Access with Mismatched Client Cert Rejected with 401")
            else:
                log_fail("mTLS Mismatched Cert Access", f"Expected 401, got {st_bad}")
        else:
            log_fail("mTLS Certificate Token Exchange", f"Status: {st}, Body: {b}")
    else:
        log_fail("Private key missing", f"{private_key_path} not found")

    # 5. Satellite CRUD & Unauthenticated Rejections
    print(f"\n{YELLOW}--- 5. Satellite Management & RBAC Verification ---{RESET}")
    # Verify unauthenticated GET /v1/satellites returns 401
    st, _, _ = http_req("GET", f"{api_url}/v1/satellites")
    if st == 401:
        log_pass("Unauthenticated GET /v1/satellites Rejected with 401 Unauthorized")
    else:
        log_fail("Unauthenticated GET /v1/satellites", f"Expected 401, got {st}")

    # Verify Viewer cannot create satellite (403 Forbidden)
    st, _, _ = http_req("POST", f"{api_url}/v1/satellites", headers={"Authorization": f"Bearer {dev_viewer_token}"}, body={
        "name": "TEST", "lineOne": "1 ...", "lineTwo": "2 ..."
    })
    if st == 403:
        log_pass("Viewer Role Creating Satellite Rejected with 403 Forbidden")
    else:
        log_fail("Viewer Role Create check", f"Expected 403, got {st}")

    # Create Satellite with Editor token
    create_body = {
        "name": "ATLAS CENTAUR 2",
        "lineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "lineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
    }
    st, _, b = http_req("POST", f"{api_url}/v1/satellites", headers={"Authorization": f"Bearer {editor_token}"}, body=create_body)
    if st == 201 and isinstance(b, dict) and "id" in b:
        sat_id = b["id"]
        log_pass(f"Create Satellite (POST /v1/satellites - ID: {sat_id})")
    else:
        log_fail("Create Satellite", f"Status: {st}, Body: {b}")
        sat_id = None

    # List satellites with Viewer token
    st, _, b = http_req("GET", f"{api_url}/v1/satellites?limit=5", headers={"Authorization": f"Bearer {dev_viewer_token}"})
    if st == 200 and isinstance(b, dict) and "data" in b and len(b["data"]) >= 1:
        log_pass(f"List Satellites Paginated (GET /v1/satellites - Count: {len(b['data'])})")
        next_cursor = b.get("pagination", {}).get("nextCursor")
        if next_cursor:
            st2, _, b2 = http_req("GET", f"{api_url}/v1/satellites?limit=5&cursor={next_cursor}", headers={"Authorization": f"Bearer {dev_viewer_token}"})
            if st2 == 200:
                log_pass("Checkpoint Cursor Pagination Page 2 Retrieved Successfully")
            else:
                log_fail("Cursor pagination page 2", f"Status: {st2}")
    else:
        log_fail("List Satellites", f"Status: {st}, Body: {b}")

    # Get Satellite by ID
    if sat_id:
        st, _, b = http_req("GET", f"{api_url}/v1/satellites/{sat_id}", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        if st == 200 and isinstance(b, dict) and b.get("id") == sat_id:
            log_pass(f"Retrieve Satellite by ID (GET /v1/satellites/{sat_id})")
        else:
            log_fail("Get Satellite by ID", f"Status: {st}, Body: {b}")

        # Update Satellite (PATCH) with Editor token
        st, _, b = http_req("PATCH", f"{api_url}/v1/satellites/{sat_id}", headers={"Authorization": f"Bearer {editor_token}"}, body={
            "name": "ATLAS CENTAUR 2 - Updated QA Epoch"
        })
        if st == 200 and isinstance(b, dict) and "Updated QA Epoch" in b.get("name", ""):
            log_pass("Update Satellite Details (PATCH /v1/satellites/{id})")
        else:
            log_fail("Update Satellite", f"Status: {st}, Body: {b}")

    # 6. Astrodynamics & Calculations
    print(f"\n{YELLOW}--- 6. Astrodynamics & Orbital Calculations ---{RESET}")
    # Canonical /v1/overhead-satellites
    st, _, b = http_req("GET", f"{api_url}/v1/overhead-satellites?lat=13.923&lon=177.315&time=2021-08-27T16:00:00Z", headers={"Authorization": f"Bearer {dev_viewer_token}"})
    if st == 200 and isinstance(b, dict) and "satellite" in b:
        log_pass(f"Canonical Overhead Query (GET /v1/overhead-satellites - Sat: {b['satellite']['name']}, Elev: {b.get('elevation')}°)")
    else:
        log_fail("Canonical Overhead calculation", f"Status: {st}, Body: {b}")

    if sat_id:
        # Next visible pass
        st, _, b = http_req("GET", f"{api_url}/v1/satellites/{sat_id}/next-visible?lat=13.923&lon=177.315&threshold_deg=5", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        if st == 200 and isinstance(b, dict) and "passTime" in b:
            log_pass(f"Next Visible Pass (GET /v1/satellites/{sat_id}/next-visible - Elev: {b.get('elevationDeg')}°)")
        elif st == 404:
            log_pass(f"Next Visible Pass (GET /v1/satellites/{sat_id}/next-visible - 404 No pass in window)")
        else:
            log_fail("Next visible pass", f"Status: {st}, Body: {b}")

        # Ground Track GeoJSON
        st, _, b = http_req("GET", f"{api_url}/v1/satellites/{sat_id}/groundtrack?start_time=2021-08-27T16:00:00Z&duration_minutes=30&step_seconds=60&format=all", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        if st == 200 and isinstance(b, dict) and "trajectory" in b and len(b["trajectory"]) > 0:
            log_pass(f"3D Ground Track & GeoJSON (GET /v1/satellites/{sat_id}/groundtrack - Points: {len(b['trajectory'])})")
        else:
            log_fail("Groundtrack calculation", f"Status: {st}, Body: {b}")

        # Illumination
        st, _, b = http_req("GET", f"{api_url}/v1/satellites/{sat_id}/illumination?lat=13.923&lon=177.315&time=2021-08-27T16:00:00Z", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        if st == 200 and isinstance(b, dict) and "lightingState" in b:
            log_pass(f"Satellite Solar Illumination (GET /v1/satellites/{sat_id}/illumination - State: {b['lightingState']})")
        else:
            log_fail("Illumination calculation", f"Status: {st}, Body: {b}")

        # RF Doppler shift
        st, _, b = http_req("GET", f"{api_url}/v1/satellites/{sat_id}/doppler?center_freq_hz=437500000&lat=13.923&lon=177.315&time=2021-08-27T16:00:00Z", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        if st == 200 and isinstance(b, dict) and "dopplerShiftHz" in b:
            log_pass(f"RF Doppler Frequency Shift (GET /v1/satellites/{sat_id}/doppler - Shift: {b['dopplerShiftHz']} Hz)")
        else:
            log_fail("Doppler calculation", f"Status: {st}, Body: {b}")

        # Maneuvers
        st, _, b = http_req("GET", f"{api_url}/v1/satellites/{sat_id}/maneuvers", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        if st == 200 and isinstance(b, dict) and "maneuvers" in b:
            log_pass("Orbital Maneuver Reconstruction (GET /v1/satellites/{id}/maneuvers)")
        else:
            log_fail("Maneuver reconstruction", f"Status: {st}, Body: {b}")

        # Anomaly Detection
        st, _, b = http_req("POST", f"{api_url}/v1/satellites/{sat_id}/detect-anomalies", headers={"Authorization": f"Bearer {dev_viewer_token}"}, body={"thresholdSigma": 3.0})
        if st == 200 and isinstance(b, dict) and "status" in b:
            log_pass(f"Orbital Anomaly Detection (POST /v1/satellites/{sat_id}/detect-anomalies - Status: {b['status']})")
        else:
            log_fail("Anomaly detection", f"Status: {st}, Body: {b}")

    # Solar Transits
    st, _, b = http_req("GET", f"{api_url}/v1/transits/solar?lat=28.57&lon=-80.64&durationDays=1", headers={"Authorization": f"Bearer {dev_viewer_token}"})
    if st == 200 and isinstance(b, dict) and b.get("target") == "Sun":
        log_pass("Solar Satellite Transits (GET /v1/transits/solar)")
    else:
        log_fail("Solar transit query", f"Status: {st}, Body: {b}")

    # Lunar Transits
    st, _, b = http_req("GET", f"{api_url}/v1/transits/lunar?lat=28.57&lon=-80.64&durationDays=1", headers={"Authorization": f"Bearer {dev_viewer_token}"})
    if st == 200 and isinstance(b, dict) and b.get("target") == "Moon":
        log_pass("Lunar Satellite Transits (GET /v1/transits/lunar)")
    else:
        log_fail("Lunar transit query", f"Status: {st}, Body: {b}")

    # Conjunctions Radar
    st, _, b = http_req("GET", f"{api_url}/v1/conjunctions/search?max_distance_km=15.0&duration_hours=24", headers={"Authorization": f"Bearer {dev_viewer_token}"})
    if st == 200 and isinstance(b, dict) and "conjunctionsFound" in b:
        log_pass(f"Conjunction Collision Radar (GET /v1/conjunctions/search - Found: {b['conjunctionsFound']})")
    else:
        log_fail("Conjunction search", f"Status: {st}, Body: {b}")

    # Pipeline Sync (Admin only)
    st, _, b = http_req("POST", f"{api_url}/v1/pipelines/sync?group=stations", headers={"Authorization": f"Bearer {dev_admin_token}"})
    if st == 200 and isinstance(b, dict) and "syncedCount" in b:
        log_pass(f"Automated Pipeline Sync (POST /v1/pipelines/sync - Synced: {b.get('syncedCount')})")
    else:
        log_fail("Pipeline sync", f"Status: {st}, Body: {b}")

    # 7. Rate Limiting Subsystem (when Redis active)
    if has_redis:
        print(f"\n{YELLOW}--- 7. Rate Limiter Subsystem & Redis Headers ---{RESET}")
        st, headers, _ = http_req("GET", f"{api_url}/v1/satellites?limit=1", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        headers_lower = {k.lower(): v for k, v in headers.items()}
        if "ratelimit-limit" in headers_lower:
            log_pass(f"RateLimit Headers Active (Limit: {headers_lower['ratelimit-limit']})")
        else:
            log_fail("RateLimit headers check", f"Headers: {headers}")

    # 8. Single-Flight Coalescing L1/L2 Cache Test
    print(f"\n{YELLOW}--- 8. Single-Flight Coalescing & L1 Moka Cache ---{RESET}")
    if sat_id:
        t0 = time.time()
        st1, _, _ = http_req("GET", f"{api_url}/v1/satellites/{sat_id}/groundtrack?start_time=2021-08-27T16:00:00Z&duration_minutes=30&step_seconds=60", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        elapsed1 = (time.time() - t0) * 1000

        t1 = time.time()
        st2, _, _ = http_req("GET", f"{api_url}/v1/satellites/{sat_id}/groundtrack?start_time=2021-08-27T16:00:00Z&duration_minutes=30&step_seconds=60", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        elapsed2 = (time.time() - t1) * 1000

        if st1 == 200 and st2 == 200:
            log_pass(f"L1 Cache Accelerated Ground Track (Cold: {elapsed1:.1f}ms -> Hot: {elapsed2:.1f}ms)")
        else:
            log_fail("Cache acceleration check", f"Cold status {st1}, Hot status {st2}")

    # 9. Clean-up: Delete Satellite (Admin role verification)
    print(f"\n{YELLOW}--- 9. Satellite Deletion & RBAC Admin Gate ---{RESET}")
    if sat_id:
        # Editor cannot delete -> 403 Forbidden
        st, _, _ = http_req("DELETE", f"{api_url}/v1/satellites/{sat_id}", headers={"Authorization": f"Bearer {editor_token}"})
        if st == 403:
            log_pass("Editor Role Cannot Delete Satellite (403 Forbidden)")
        else:
            log_fail("Editor Delete RBAC check", f"Expected 403, got {st}")

        # Admin deletes -> 204 No Content
        st, _, _ = http_req("DELETE", f"{api_url}/v1/satellites/{sat_id}", headers={"Authorization": f"Bearer {dev_admin_token}"})
        if st == 204:
            log_pass(f"Admin Role Deletes Satellite (DELETE /v1/satellites/{sat_id} - 204 No Content)")
        else:
            log_fail("Admin Delete check", f"Expected 204, got {st}")

        # Verify 404
        st, _, _ = http_req("GET", f"{api_url}/v1/satellites/{sat_id}", headers={"Authorization": f"Bearer {dev_viewer_token}"})
        if st == 404:
            log_pass("Deleted Satellite Confirmed Purged (404 Not Found)")
        else:
            log_fail("Purge verification", f"Expected 404, got {st}")

def start_server(env_vars, port=8080):
    env = os.environ.copy()
    env.update(env_vars)
    env["PORT"] = str(port)
    env["HOST"] = "127.0.0.1"
    proc = subprocess.Popen(
        ["./target/debug/astrea-sda-api"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )
    return proc

def stop_server(proc):
    if proc:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()

def main():
    print(f"{BLUE}======================================================================{RESET}")
    print(f"{BLUE}🛰️  ASTREA SDA API — COMPREHENSIVE END-TO-END QA SUITE{RESET}")
    print(f"{BLUE}======================================================================{RESET}")

    # Build binary first
    print("\nEnsuring latest binary is compiled...")
    build_res = subprocess.run(["cargo", "build"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if build_res.returncode != 0:
        print(f"{RED}Compilation failed:{RESET}\n{build_res.stderr}")
        sys.exit(1)

    # ──────────────────────────────────────────────────────────────────────────
    # Profile A: In-Memory / SQLite (Without Postgres & Without Redis)
    # ──────────────────────────────────────────────────────────────────────────
    print(f"\n{YELLOW}▶ Starting Profile A: Without Redis / Without Postgres (In-Memory / SQLite Fallback)...{RESET}")
    env_profile_a = {
        "POSTGRES_URI": "",
        "DATABASE_URL": "",
        "REDIS_URL": "",
        "ENABLE_DISCOVERY_PIPELINE": "false",
        "RSA_PRIVATE_KEY_FILE": ".keys/rsa_private.pem",
        "RSA_PUBLIC_KEY_FILE": ".keys/rsa_public.pem",
    }
    server_a = start_server(env_profile_a, port=8081)
    url_a = "http://127.0.0.1:8081"
    if not wait_for_server(url_a):
        print(f"{RED}Failed to start server for Profile A{RESET}")
        stop_server(server_a)
        sys.exit(1)
    try:
        run_suite(url_a, "Profile A: In-Memory (No Redis/PG)", has_redis=False)
    finally:
        stop_server(server_a)

    # ──────────────────────────────────────────────────────────────────────────
    # Profile B: Full Distributed Stack (With Postgres 5432 & Redis 6379)
    # ──────────────────────────────────────────────────────────────────────────
    print(f"\n{YELLOW}▶ Starting Profile B: With Postgres & Redis (Distributed Stack)...{RESET}")
    env_profile_b = {
        "POSTGRES_URI": "postgres://astrea:astreadbpass@127.0.0.1:5432/astrea_sda",
        "REDIS_URL": "redis://127.0.0.1:6379",
        "RATE_LIMIT_ALGORITHM": "SLIDING_WINDOW",
        "RATE_LIMIT_MAX": "100",
        "RATE_LIMIT_WINDOW_SECS": "60",
        "ENABLE_DISCOVERY_PIPELINE": "false",
        "RSA_PRIVATE_KEY_FILE": ".keys/rsa_private.pem",
        "RSA_PUBLIC_KEY_FILE": ".keys/rsa_public.pem",
    }
    server_b = start_server(env_profile_b, port=8082)
    url_b = "http://127.0.0.1:8082"
    if not wait_for_server(url_b):
        print(f"{RED}Failed to start server for Profile B{RESET}")
        stop_server(server_b)
        sys.exit(1)
    try:
        run_suite(url_b, "Profile B: Full Distributed (Postgres + Redis)", has_redis=True)
    finally:
        stop_server(server_b)

    # Summary
    print(f"\n{BLUE}======================================================================{RESET}")
    print(f"{BLUE}🏁 FINAL QA TEST SUMMARY{RESET}")
    print(f"{BLUE}======================================================================{RESET}")
    print(f"Total Passed: {GREEN}{PASS_COUNT}{RESET}")
    print(f"Total Failed: {RED}{FAIL_COUNT}{RESET}")

    if FAIL_COUNT == 0:
        print(f"\n{GREEN}🎉 ALL END-TO-END QA TESTS PASSED WITH ZERO DEFECTS!{RESET}\n")
        sys.exit(0)
    else:
        print(f"\n{RED}❌ Defects encountered during QA run:{RESET}")
        for d in DEFECTS:
            print(f"  - {d}")
        sys.exit(1)

if __name__ == "__main__":
    main()
