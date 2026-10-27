#!/usr/bin/env bash
# ==============================================================================
# Astrea SDA API - Seed Demonstration Satellites for Cesium 3D Viewer
# ==============================================================================
set -e

API_URL="${API_URL:-http://localhost:8080}"
echo "🛰️  Seeding Astrea SDA API ($API_URL) with demonstration satellites..."

# 1. Login to get token
LOGIN_RES=$(curl -s -X POST "$API_URL/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@astrea.local","password":"password123"}')

TOKEN=$(echo "$LOGIN_RES" | jq -r '.token // empty')

if [ -z "$TOKEN" ]; then
  echo "❌ Error: Could not authenticate with $API_URL."
  echo "   Make sure the server is running with Cesium assets configured:"
  echo "   $ VIEWER_ASSETS_DIR=.viewer-assets/cesium cargo run"
  exit 1
fi
echo "✔ Authenticated as admin@astrea.local"

# 2. Seed ISS (ZARYA)
ISS=$(curl -s -X POST "$API_URL/v1/satellites" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "name": "ISS (ZARYA)",
    "lineOne": "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996",
    "lineTwo": "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012"
  }')
ISS_ID=$(echo "$ISS" | jq -r '.id // empty')
echo "✔ Seeded Satellite A: ISS (ZARYA) -> ID: $ISS_ID"

# 3. Seed TIANGONG (CSS)
CSS=$(curl -s -X POST "$API_URL/v1/satellites" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "name": "TIANGONG (CSS)",
    "lineOne": "1 48274U 21035A   24083.91666667  .00021500  00000+0  31200-3 0  9991",
    "lineTwo": "2 48274  41.4725 152.3412 0003500 245.1234 114.8765 15.58912345165008"
  }')
CSS_ID=$(echo "$CSS" | jq -r '.id // empty')
echo "✔ Seeded Satellite B: TIANGONG (CSS) -> ID: $CSS_ID"

# 4. Seed STARLINK-1007
STARLINK=$(curl -s -X POST "$API_URL/v1/satellites" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "name": "STARLINK-1007",
    "lineOne": "1 44713U 19074A   24083.89583333  .00001200  00000+0  78000-4 0  9992",
    "lineTwo": "2 44713  53.0542 196.1200 0001450  85.2341 274.8912 15.06412345241005"
  }')
STARLINK_ID=$(echo "$STARLINK" | jq -r '.id // empty')
echo "✔ Seeded Satellite C: STARLINK-1007  -> ID: $STARLINK_ID"

echo ""
echo "=========================================================="
echo "✨ Visualization Ready!"
echo "=========================================================="
echo "1. Viewer URL: http://localhost:8080/viewer"
echo "2. Credentials: admin@astrea.local / password123"
echo "   (Or paste bearer token into 'Use an existing token')"
echo ""
echo "Features to verify in the viewer:"
echo " • Ground Track (Cyan trajectory + 3D orbit ribbon + real-time clock)"
echo " • Illumination (Shadow state: sunlight vs penumbra vs umbra eclipse)"
echo " • Passes (Topocentric horizon visibility passes over ground observer)"
echo " • Compare Mode (Toggle 'Compare Mode' checkbox to overlay ISS [Cyan] vs TIANGONG [Amber])"
echo " • Conjunction / Relative Motion (Layer options in Compare Mode)"
echo "=========================================================="
