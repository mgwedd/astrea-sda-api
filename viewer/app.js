// The JWT lives only in this closure: never written to browser storage, cookies, or the URL.
(() => {
  const $ = (id) => document.getElementById(id);
  if (typeof Cesium === "undefined") { $("no-cesium").hidden = false; return; }

  let token = null;
  let viewer = null;
  const satelliteMap = new Map();

  // In-memory default observer coordinates for pass predictions.
  // Security rule: NEVER serialized into the URL hash or persisted in client storage.
  const DEFAULT_OBSERVER = { lat: 40.0, lon: -105.0, altM: 1600 };

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

  // --- Strict Sanitization for Deep Linking ---
  // Security rules:
  // 1. NEVER encode auth tokens, passwords, or credentials into URL hash.
  // 2. NEVER encode observer physical coordinates (lat, lon, altM) into URL hash.
  function sanitizeSceneConfig(cfg) {
    if (!cfg || typeof cfg !== "object") return {};
    const clean = Array.isArray(cfg) ? [] : {};
    for (const [k, v] of Object.entries(cfg)) {
      const lower = k.toLowerCase();
      if (lower.includes("token") || lower.includes("auth") || lower.includes("jwt") ||
          lower.includes("password") || lower.includes("secret") || lower.includes("bearer") ||
          lower.includes("credential")) {
        continue;
      }
      if (lower === "observer" || lower === "lat" || lower === "lon" || lower === "altm" || lower === "alt_m" || lower === "alt") {
        continue;
      }
      if (v && typeof v === "object") {
        clean[k] = sanitizeSceneConfig(v);
      } else {
        clean[k] = v;
      }
    }
    return clean;
  }

  function encodeSceneHash(config) {
    const clean = sanitizeSceneConfig(config);
    try {
      return "#scene=" + encodeURIComponent(btoa(JSON.stringify(clean)));
    } catch (_) {
      return "#scene=" + encodeURIComponent(JSON.stringify(clean));
    }
  }

  function decodeSceneHash(hash) {
    if (!hash || !hash.startsWith("#scene=")) return null;
    const raw = hash.slice(7);
    if (!raw) return null;
    try {
      let jsonStr;
      if (raw.startsWith("%7B") || raw.startsWith("{")) {
        jsonStr = decodeURIComponent(raw);
      } else {
        jsonStr = atob(decodeURIComponent(raw));
      }
      const parsed = JSON.parse(jsonStr);
      return sanitizeSceneConfig(parsed); // Defense-in-depth: sanitize decoded state
    } catch (e) {
      console.warn("Failed to decode scene hash:", e);
      return null;
    }
  }

  function getFormConfig() {
    const isCompare = $("compare-mode").checked;
    const cfg = {
      sat: $("sat").value,
      minutes: Math.min(1440, Math.max(1, Number($("minutes").value) || 90)),
      layer: $("layer-type").value || "groundtrack",
      compare: isCompare,
    };
    if (isCompare && $("sat-b").value) {
      cfg.satB = $("sat-b").value;
    }
    return cfg;
  }

  function syncUrlHash() {
    const cfg = getFormConfig();
    const hash = encodeSceneHash(cfg);
    if (window.location.hash !== hash) {
      window.location.hash = hash;
    }
  }

  function updateCompareUI(isCompare) {
    $("compare-sat-group").hidden = !isCompare;
    $("compare-legend").hidden = !isCompare;
    $("load-scene").textContent = isCompare ? "Compare scenes" : "Compose scene";
  }

  function applySceneConfig(cfg) {
    if (!cfg) return;
    if (cfg.sat && satelliteMap.has(cfg.sat)) {
      $("sat").value = cfg.sat;
    }
    if (cfg.minutes) {
      $("minutes").value = cfg.minutes;
    }
    if (cfg.layer && $("layer-type")) {
      $("layer-type").value = cfg.layer;
    }
    const isCompare = Boolean(cfg.compare);
    $("compare-mode").checked = isCompare;
    updateCompareUI(isCompare);
    if (isCompare && cfg.satB && satelliteMap.has(cfg.satB)) {
      $("sat-b").value = cfg.satB;
    }
  }

  // Namespaces packet IDs and recolors comparison entities to Neon Amber/Orange [255, 145, 0, 255]
  function transformCompareCzml(czml) {
    if (!Array.isArray(czml)) return czml;
    return czml.map((pkt) => {
      if (pkt.id === "document") return pkt;
      const copy = JSON.parse(JSON.stringify(pkt));
      copy.id = "compare_" + copy.id;
      if (copy.name) copy.name = "[B] " + copy.name;
      if (copy.label && copy.label.text) {
        if (typeof copy.label.text === "string") {
          copy.label.text = "[B] " + copy.label.text;
        }
        copy.label.fillColor = { rgba: [255, 145, 0, 255] };
      }
      if (copy.polyline) {
        copy.polyline.material = { solidColor: { color: { rgba: [255, 145, 0, 255] } } };
      }
      if (copy.point) {
        copy.point.color = { rgba: [255, 145, 0, 255] };
      }
      return copy;
    });
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
    satelliteMap.clear();
    for (const s of list.data) {
      satelliteMap.set(s.id, s.name);
    }
    const optionsA = list.data.map((s) => new Option(s.name, s.id));
    const optionsB = list.data.map((s) => new Option(s.name, s.id));
    $("sat").replaceChildren(...optionsA);
    $("sat-b").replaceChildren(...optionsB);
    if (list.data.length > 1) {
      $("sat-b").selectedIndex = 1;
    }

    // Restore deep-linked scene configuration from URL hash if present
    const initialHashConfig = decodeSceneHash(window.location.hash);
    if (initialHashConfig) {
      applySceneConfig(initialHashConfig);
      await loadScene();
    }

    viewer.selectedEntityChanged.addEventListener(async (entity) => {
      const panel = $("inspector");
      if (!entity) { panel.hidden = true; return; }
      panel.hidden = false;
      const title = entity.name || entity.id || "Entity";
      $("inspector-title").textContent = title;

      // Determine which satellite was clicked (A or B)
      const isCompareEntity = (entity.id && entity.id.startsWith("compare_")) || (title && title.startsWith("[B]"));
      const satId = isCompareEntity ? $("sat-b").value : $("sat").value;
      const sceneLabel = isCompareEntity ? "Scene B (Comparison)" : "Scene A (Primary)";

      $("inspector-body").innerHTML = `<p><small>${sceneLabel}</small></p><p>ID: <code>${entity.id}</code></p><p>Loading state telemetry…</p>`;
      try {
        const state = await api(`/v1/satellites/${satId}/state`);
        $("inspector-body").innerHTML = `
          <p><strong>${state.satelliteName}</strong> <small>(${sceneLabel})</small></p>
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
      const satIdA = $("sat").value;
      const isCompare = $("compare-mode").checked;
      const satIdB = isCompare ? $("sat-b").value : null;

      viewer.dataSources.removeAll();
      const rA = await api(`/v1/satellites/${satIdA}/groundtrack?format=czml&duration_minutes=${minutes}`);
      await viewer.dataSources.add(await Cesium.CzmlDataSource.load(rA.czml));

      let extraMsg = "";
      if (isCompare && satIdB) {
        const rB = await api(`/v1/satellites/${satIdB}/groundtrack?format=czml&duration_minutes=${minutes}`);
        const styledB = transformCompareCzml(rB.czml);
        await viewer.dataSources.add(await Cesium.CzmlDataSource.load(styledB));
        extraMsg = " (comparing A & B)";
      }
      syncUrlHash();
      $("status").textContent = (rA.droppedSamples ? `${rA.droppedSamples} samples dropped` : "Loaded") + extraMsg;
    } catch (e) { $("status").textContent = e.message; }
  }

  async function loadScene() {
    $("status").textContent = "Composing scene…";
    try {
      const satIdA = $("sat").value;
      const nameA = satelliteMap.get(satIdA) || $("sat").selectedOptions[0]?.text || "Sat A";
      const hours = Math.min(24, Math.max(0.5, (Number($("minutes").value) || 90) / 60));
      const isCompare = $("compare-mode").checked;
      const satIdB = isCompare ? $("sat-b").value : null;
      const nameB = satIdB ? (satelliteMap.get(satIdB) || $("sat-b").selectedOptions[0]?.text || "Sat B") : "Sat B";
      const layerType = $("layer-type").value || "groundtrack";

      const layers = [];
      if (!isCompare) {
        if (layerType === "illumination") {
          layers.push({ type: "illumination", satelliteId: satIdA, label: nameA });
        } else if (layerType === "passes") {
          layers.push({ type: "groundtrack", satelliteId: satIdA, label: nameA, rgba: [0, 229, 255, 255] });
          layers.push({ type: "passes", satelliteId: satIdA, observer: DEFAULT_OBSERVER });
        } else {
          layers.push({ type: "groundtrack", satelliteId: satIdA, label: nameA, rgba: [0, 229, 255, 255] });
        }
      } else {
        // Compare Mode: compose scenes with distinctive Cyan (Scene A) vs Amber/Orange (Scene B)
        if (layerType === "conjunction") {
          layers.push({ type: "groundtrack", satelliteId: satIdA, label: `[A] ${nameA}`, rgba: [0, 229, 255, 255] });
          layers.push({ type: "groundtrack", satelliteId: satIdB, label: `[B] ${nameB}`, rgba: [255, 145, 0, 255] });
          layers.push({ type: "conjunction", primaryId: satIdA, secondaryId: satIdB, label: `Conjunction ${nameA} - ${nameB}` });
        } else if (layerType === "relativeMotion") {
          layers.push({ type: "groundtrack", satelliteId: satIdA, label: `[A] ${nameA}`, rgba: [0, 229, 255, 255] });
          layers.push({ type: "relativeMotion", primaryId: satIdA, targetId: satIdB, label: `LVLH ${nameB} rel to ${nameA}` });
        } else if (layerType === "illumination") {
          layers.push({ type: "illumination", satelliteId: satIdA, label: `[A] ${nameA}` });
          layers.push({ type: "illumination", satelliteId: satIdB, label: `[B] ${nameB}` });
        } else if (layerType === "passes") {
          layers.push({ type: "passes", satelliteId: satIdA, label: `[A] ${nameA}`, observer: DEFAULT_OBSERVER });
          layers.push({ type: "passes", satelliteId: satIdB, label: `[B] ${nameB}`, observer: DEFAULT_OBSERVER });
        } else {
          // Default: dual ground track overlay with Cyan (A) and Amber/Orange (B)
          layers.push({ type: "groundtrack", satelliteId: satIdA, label: `[A] ${nameA}`, rgba: [0, 229, 255, 255] });
          layers.push({ type: "groundtrack", satelliteId: satIdB, label: `[B] ${nameB}`, rgba: [255, 145, 0, 255] });
        }
      }

      const r = await api("/v1/scenes", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          durationHours: hours,
          stepSeconds: 60,
          layers: layers
        })
      });

      viewer.dataSources.removeAll();
      await viewer.dataSources.add(await Cesium.CzmlDataSource.load(r.czml));
      syncUrlHash();
      $("status").textContent = `Scene composed (${r.layerCount} layers, ${r.droppedSamples} dropped)${isCompare ? " [Compare Mode]" : ""}`;
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
  $("compare-mode").addEventListener("change", (ev) => {
    updateCompareUI(ev.target.checked);
    syncUrlHash();
  });
  $("sat").addEventListener("change", syncUrlHash);
  $("sat-b").addEventListener("change", syncUrlHash);
  $("minutes").addEventListener("change", syncUrlHash);
  $("layer-type").addEventListener("change", syncUrlHash);

  $("share-link").addEventListener("click", async () => {
    syncUrlHash();
    const url = window.location.href;
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(url);
        $("status").textContent = "Scene link copied to clipboard!";
      } else {
        $("status").textContent = "URL updated with scene link";
      }
    } catch (_) {
      $("status").textContent = "URL updated with scene link";
    }
  });

  window.addEventListener("hashchange", () => {
    const cfg = decodeSceneHash(window.location.hash);
    if (cfg && token && viewer) {
      applySceneConfig(cfg);
      loadScene();
    }
  });

  $("inspector-close").addEventListener("click", () => { $("inspector").hidden = true; });
  $("logout").addEventListener("click", signOut);
  show(false);
})();
