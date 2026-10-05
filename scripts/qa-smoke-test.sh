#!/usr/bin/env bash
# ==============================================================================
# Astrea SDA API - Automated QA Smoke Test Runner
# ==============================================================================
set -e

API_URL="${API_URL:-http://localhost:8080}"
echo "=========================================================="
echo "🛰️  Astrea SDA API Smoke Test Suite"
echo "Target URL: $API_URL"
echo "=========================================================="

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

pass() { echo -e "${GREEN}✔ PASS:${NC} $1"; }
fail() { echo -e "${RED}✖ FAIL:${NC} $1"; exit 1; }

# 1. OpenAPI Docs
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$API_URL/swagger-ui/")
[ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "303" ] || [ "$HTTP_CODE" = "307" ] && pass "Swagger UI Accessible (/swagger-ui/)" || fail "Swagger UI failed ($HTTP_CODE)"

# 2. Auth: Signup Editor
EMAIL="qa-run-$(date +%s)@astrealabs.com"
LOGIN_RES=$(curl -s -X POST "$API_URL/v1/auth/signup" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$EMAIL\",\"password\":\"P@ssword12345!\",\"role\":\"editor\"}")
TOKEN=$(echo "$LOGIN_RES" | jq -r .token)
[ "$TOKEN" != "null" ] && [ -n "$TOKEN" ] && pass "User Signup & JWT Issuance" || fail "Signup failed: $LOGIN_RES"

# 3. Auth: Profile
ME_ROLE=$(curl -s "$API_URL/v1/auth/me" -H "Authorization: Bearer $TOKEN" | jq -r .role)
[ "$ME_ROLE" = "editor" ] && pass "Profile Verified (/v1/auth/me - Role: $ME_ROLE)" || fail "Profile mismatch ($ME_ROLE)"

# 4. Satellites: Create
CREATE_RES=$(curl -s -X POST "$API_URL/v1/satellites" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "name": "ATLAS CENTAUR 2",
    "lineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
    "lineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
  }')
SAT_ID=$(echo "$CREATE_RES" | jq -r .id)
[ "$SAT_ID" != "null" ] && [ -n "$SAT_ID" ] && pass "Create Satellite ($SAT_ID)" || fail "Create failed: $CREATE_RES"

# 5. Satellites: List (Protected: Requires Bearer Auth)
COUNT=$(curl -s -H "Authorization: Bearer $TOKEN" "$API_URL/v1/satellites?limit=1" | jq '.data | length')
[ "$COUNT" -ge 1 ] && pass "List Satellites Paginated (Count: $COUNT)" || fail "List failed"

# 6. Satellites: Overhead (Protected: Requires Bearer Auth; tests canonical /v1/overhead-satellites)
OVERHEAD_ELEV=$(curl -s -H "Authorization: Bearer $TOKEN" "$API_URL/v1/overhead-satellites?lat=13.923&lon=177.315&time=2021-08-27T16:00:00Z" | jq .elevation)
[ "$OVERHEAD_ELEV" != "null" ] && [ -n "$OVERHEAD_ELEV" ] && pass "Overhead Search (/v1/overhead-satellites - Elevation: ${OVERHEAD_ELEV}°)" || fail "Overhead query failed"

# 7. Satellites: Groundtrack GeoJSON (Protected: Requires Bearer Auth)
FORMAT=$(curl -s -H "Authorization: Bearer $TOKEN" "$API_URL/v1/satellites/$SAT_ID/groundtrack?start_time=2021-08-27T16:00:00Z&duration_minutes=30&step_seconds=60&format=geojson" | jq -r '.geojson.type // .type')
([ "$FORMAT" = "Feature" ] || [ "$FORMAT" = "FeatureCollection" ]) && pass "Ground Track GeoJSON Validated (Type: $FORMAT)" || fail "Groundtrack invalid format: $FORMAT"

# 8. Satellites: Illumination (Protected: Requires Bearer Auth)
LIGHTING=$(curl -s -H "Authorization: Bearer $TOKEN" "$API_URL/v1/satellites/$SAT_ID/illumination?lat=13.923&lon=177.315&time=2021-08-27T16:00:00Z" | jq -r .lightingState)
[ -n "$LIGHTING" ] && [ "$LIGHTING" != "null" ] && pass "Illumination State ($LIGHTING)" || fail "Illumination failed"

# 9. Satellites: Doppler (Protected: Requires Bearer Auth)
DOPPLER=$(curl -s -H "Authorization: Bearer $TOKEN" "$API_URL/v1/satellites/$SAT_ID/doppler?center_freq_hz=437500000&lat=13.923&lon=177.315&time=2021-08-27T16:00:00Z" | jq .dopplerShiftHz)
[ -n "$DOPPLER" ] && [ "$DOPPLER" != "null" ] && pass "RF Doppler Shift Calculated (${DOPPLER} Hz)" || fail "Doppler failed"

# 10. Astrodynamics: Solar Transits (Protected: Requires Bearer Auth)
TRANSIT_TARGET=$(curl -s -H "Authorization: Bearer $TOKEN" "$API_URL/v1/transits/solar?lat=28.57&lon=-80.64&durationDays=1" | jq -r .target)
[ "$TRANSIT_TARGET" = "Sun" ] && pass "Solar Transit Predictor (/v1/transits/solar)" || fail "Transit target mismatch: $TRANSIT_TARGET"

echo "=========================================================="
echo -e "${GREEN}🎉 All 10 Smoke Test Assertions Passed Successfully!${NC}"
echo "=========================================================="
