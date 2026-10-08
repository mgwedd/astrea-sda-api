# Viewer (`/viewer`)

A static CesiumJS client served by the API. It holds no server state and calls the same `/v1` JSON API as any other client.

## Run it

```bash
./scripts/fetch-cesium.sh                      # pinned CesiumJS -> .viewer-assets/cesium (~22 MB)
VIEWER_ASSETS_DIR=.viewer-assets/cesium cargo run
# open http://localhost:8080/viewer
```

The Docker image bundles Cesium and sets `VIEWER_ASSETS_DIR=/app/viewer/cesium`. Without `VIEWER_ASSETS_DIR` the page loads and says Cesium is missing; `/viewer/cesium/*` returns 404.

## Sign-in and token handling

Sign in with email and password (`POST /v1/auth/login`) or paste an existing bearer token. The JWT is held in a JavaScript closure only: never in `localStorage`, `sessionStorage`, a cookie, or the URL. Reloading the page signs you out. `tests/viewer_tests.rs` greps `app.js` for those APIs.

## Security headers

Every `/viewer` response carries `Content-Security-Policy`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store` and `X-Content-Type-Options: nosniff`. The CSP allows only same-origin sources plus the relaxations Cesium 1.146 needs (measured, see R10 in `CZML_VIEWER_LOG.md`):

| Directive | Why |
|---|---|
| `script-src 'unsafe-eval'` | Cesium.js bundles Knockout 3.5.1, whose `eval` runs at load; without it the whole bundle fails |
| `script-src 'wasm-unsafe-eval'` | Cesium's WebAssembly modules |
| `script-src blob:` | Cesium workers `importScripts` from blob URLs |
| `style-src 'unsafe-inline'` | Cesium injects `<style>` elements and sets `style` attributes |

`default-src 'self'`, `frame-ancestors 'none'`, `object-src 'none'` and no third-party origins stay. The globe uses Cesium's bundled NaturalEarthII imagery, so no Cesium Ion token or external tile host is involved.

Because `unsafe-eval` is allowed, the CSP does not stop an injected string from being evaluated. It still blocks inline and third-party scripts. Dropping `unsafe-eval` needs an engine-only Cesium build without Knockout (tracked in the log as R23).
