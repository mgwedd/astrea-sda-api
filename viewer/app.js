// The JWT lives only in this closure: never written to browser storage, cookies, or the URL.
(() => {
  const $ = (id) => document.getElementById(id);
  if (typeof Cesium === "undefined") { $("no-cesium").hidden = false; return; }

  let token = null;
  let viewer = null;

  const api = async (path, opts = {}) => {
    const res = await fetch(path, {
      ...opts,
      headers: { ...(opts.headers || {}), ...(token ? { Authorization: `Bearer ${token}` } : {}) },
    });
    if (res.status === 401) { signOut(); throw new Error("Session expired"); }
    if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error || `HTTP ${res.status}`);
    return res.json();
  };

  const show = (loggedIn) => { $("login").hidden = loggedIn; $("app").hidden = !loggedIn; };

  function signOut() {
    token = null;
    if (viewer) { viewer.destroy(); viewer = null; }
    show(false);
  }

  async function start() {
    show(true);
    // Bundled NaturalEarthII imagery: no Ion token, no external origin (CSP stays 'self').
    viewer = new Cesium.Viewer("globe", {
      baseLayer: Cesium.ImageryLayer.fromProviderAsync(
        Cesium.TileMapServiceImageryProvider.fromUrl(
          Cesium.buildModuleUrl("Assets/Textures/NaturalEarthII"))),
      baseLayerPicker: false, geocoder: false, timeline: true, animation: true,
    });
    const list = await api("/v1/satellites?limit=100");
    $("sat").replaceChildren(...list.data.map((s) => new Option(s.name, s.id)));

    viewer.selectedEntityChanged.addEventListener(async (entity) => {
      const panel = $("inspector");
      if (!entity) { panel.hidden = true; return; }
      panel.hidden = false;
      const title = entity.name || entity.id || "Entity";
      $("inspector-title").textContent = title;
      const satId = $("sat").value;
      $("inspector-body").innerHTML = `<p>ID: <code>${entity.id}</code></p><p>Loading state telemetry…</p>`;
      try {
        const state = await api(`/v1/satellites/${satId}/state`);
        $("inspector-body").innerHTML = `
          <p><strong>${state.satelliteName}</strong></p>
          <p>Altitude: ${state.altitudeKm.toFixed(1)} km</p>
          <p>Speed: ${state.speedKmS.toFixed(2)} km/s</p>
          <p>Period: ${state.orbitalPeriodMinutes.toFixed(1)} min</p>
          <p>Lat/Lon: ${state.latitudeDeg.toFixed(2)}°, ${state.longitudeDeg.toFixed(2)}°</p>
        `;
      } catch (e) {
        $("inspector-body").innerHTML = `<p>ID: <code>${entity.id}</code></p><p>State: unavailable</p>`;
      }
    });
  }

  async function loadTrack() {
    $("status").textContent = "Loading…";
    try {
      const minutes = Math.min(1440, Math.max(1, Number($("minutes").value) || 90));
      const r = await api(`/v1/satellites/${$("sat").value}/groundtrack?format=czml&duration_minutes=${minutes}`);
      viewer.dataSources.removeAll();
      await viewer.dataSources.add(Cesium.CzmlDataSource.load(r.czml));
      $("status").textContent = r.droppedSamples ? `${r.droppedSamples} samples dropped` : "";
    } catch (e) { $("status").textContent = e.message; }
  }

  async function loadScene() {
    $("status").textContent = "Composing scene…";
    try {
      const satId = $("sat").value;
      const hours = Math.min(24, Math.max(0.5, (Number($("minutes").value) || 90) / 60));
      const r = await api("/v1/scenes", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          durationHours: hours,
          stepSeconds: 60,
          layers: [
            { type: "groundtrack", satelliteId: satId, label: $("sat").selectedOptions[0]?.text },
            { type: "passes", satelliteId: satId, observer: { lat: 40.0, lon: -105.0, altM: 1600 } }
          ]
        })
      });
      viewer.dataSources.removeAll();
      await viewer.dataSources.add(Cesium.CzmlDataSource.load(r.czml));
      $("status").textContent = `Scene composed (${r.layerCount} layers, ${r.droppedSamples} dropped)`;
    } catch (e) { $("status").textContent = e.message; }
  }

  async function enter(t) {
    token = t;
    try { await start(); } catch (e) { signOut(); $("login-error").textContent = e.message; }
  }

  $("login").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const f = new FormData(ev.target);
    $("login-error").textContent = "";
    try {
      const r = await fetch("/v1/auth/login", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: f.get("email"), password: f.get("password") }),
      });
      if (!r.ok) throw new Error("Sign-in failed");
      await enter((await r.json()).token);
    } catch (e) { $("login-error").textContent = e.message; }
    ev.target.reset();
  });
  $("use-token").addEventListener("click", () => {
    const f = $("login").elements.token;
    const t = f.value.trim(); f.value = "";
    if (t) enter(t);
  });
  $("load").addEventListener("click", loadTrack);
  $("load-scene").addEventListener("click", loadScene);
  $("inspector-close").addEventListener("click", () => { $("inspector").hidden = true; });
  $("logout").addEventListener("click", signOut);
  show(false);
})();
