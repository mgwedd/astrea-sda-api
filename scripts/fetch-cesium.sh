#!/bin/sh
# Download the pinned CesiumJS release and copy its prebuilt bundle to $1 (default .viewer-assets/cesium).
set -eu
VERSION="${CESIUM_VERSION:-1.146.0}"
DEST="${1:-.viewer-assets/cesium}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
(cd "$TMP" && npm pack "cesium@$VERSION" --silent >/dev/null && tar xzf cesium-*.tgz)
rm -rf "$DEST" && mkdir -p "$DEST"
cp -R "$TMP/package/Build/Cesium/." "$DEST/"
echo "Cesium $VERSION -> $DEST"
