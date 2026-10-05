#!/usr/bin/env python3
"""Independent reference values for tests/transforms_reference_tests.rs.

Nothing here shares code with the Rust implementation:
  * Elements: modified equinoctial set built straight from the r,v vectors via the
    angular-momentum / eccentricity vectors and the Walker/Betts equinoctial frame
    (f^, g^, w^), then Cartesian rebuilt with the Betts closed form. Kepler's
    equation is solved by bisection.
  * Frames: astropy/ERFA (GMST 1982, WGS-84 gd2gc/gc2gd, east/north/up basis) with UT1-UTC
    forced to 0 and polar motion off, which is what the API documents (spec 12.7).
Run: python3 tests/reference/gen_transform_reference.py   (needs numpy, astropy)
"""
import json, math
import numpy as np
import erfa
from astropy.time import Time

MU = 398600.4418  # km^3/s^2, same constant as the spec (WGS-84 / EGM)


def kep_to_rv(a, e, i, raan, argp, nu):
    i, raan, argp, nu = map(math.radians, (i, raan, argp, nu))
    p = a * (1 - e * e)
    r = p / (1 + e * math.cos(nu))
    r_pqw = np.array([r * math.cos(nu), r * math.sin(nu), 0.0])
    v_pqw = np.array([-math.sqrt(MU / p) * math.sin(nu), math.sqrt(MU / p) * (e + math.cos(nu)), 0.0])
    def R3(t): c, s = math.cos(t), math.sin(t); return np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])
    def R1(t): c, s = math.cos(t), math.sin(t); return np.array([[1, 0, 0], [0, c, -s], [0, s, c]])
    Q = R3(raan) @ R1(i) @ R3(argp)
    return Q @ r_pqw, Q @ v_pqw


def mee_from_rv(r, v):
    """Modified equinoctial (Walker/Betts) straight from vectors. Returns dict."""
    rn = np.linalg.norm(r)
    hvec = np.cross(r, v)
    hn = np.linalg.norm(hvec)
    w = hvec / hn
    I = 1 if w[2] >= 0 else -1  # retrograde factor: i > 90 deg  <=> w_z < 0
    # tan^I(i/2)(cos,sin)(raan): I=+1 -> (-w_y, w_x)/(1+w_z); I=-1 -> (-w_y, w_x)/(1-w_z)
    if I == 1:
        h, k = -w[1] / (1 + w[2]), w[0] / (1 + w[2])
    else:
        h, k = -w[1] / (1 - w[2]), w[0] / (1 - w[2])
    s2 = 1 + h * h + k * k
    fh = np.array([1 - k * k + h * h, 2 * k * h, -2 * I * k]) / s2
    gh = np.array([2 * k * h * I, (1 + k * k - h * h) * I, 2 * h]) / s2
    evec = np.cross(v, hvec) / MU - r / rn
    f, g = float(evec @ fh), float(evec @ gh)
    L = math.atan2(float(r @ gh), float(r @ fh)) % (2 * math.pi)
    p = hn * hn / MU
    e = math.hypot(f, g)
    a = p / (1 - e * e)
    varpi = math.atan2(g, f)
    nu = (L - varpi) % (2 * math.pi)
    # mean anomaly via E
    E = 2 * math.atan2(math.sqrt(1 - e) * math.sin(nu / 2), math.sqrt(1 + e) * math.cos(nu / 2))
    M = (E - e * math.sin(E)) % (2 * math.pi)
    lam = (varpi + M) % (2 * math.pi)
    return dict(p=p, f=f, g=g, h=h, k=k, L=math.degrees(L), lam=math.degrees(lam), I=I, a=a, e=e,
                inc=math.degrees(math.acos(w[2])))


def rv_from_mee(p, f, g, h, k, L_deg, I):
    """Betts closed form."""
    L = math.radians(L_deg)
    s2 = 1 + h * h + k * k
    al2 = h * h - k * k
    ww = 1 + f * math.cos(L) + g * math.sin(L)
    r = p / ww
    sq = math.sqrt(MU / p)
    r_vec = r / s2 * np.array([math.cos(L) + al2 * math.cos(L) + 2 * h * k * math.sin(L),
                               math.sin(L) - al2 * math.sin(L) + 2 * h * k * math.cos(L),
                               2 * (h * math.sin(L) - k * math.cos(L))])
    v_vec = -sq / s2 * np.array([math.sin(L) + al2 * math.sin(L) - 2 * h * k * math.cos(L) + g - 2 * f * h * k + al2 * g,
                                 -math.cos(L) + al2 * math.cos(L) + 2 * h * k * math.sin(L) - f + 2 * g * h * k + al2 * f,
                                 -2 * (h * math.cos(L) + k * math.sin(L) + f * h + g * k)])
    if I == -1:
        # Betts closed form is the I=+1 expression; retrograde reflects via the I-aware frame
        s2_ = s2
        fh = np.array([1 - k * k + h * h, 2 * k * h, -2 * I * k]) / s2_
        gh = np.array([2 * k * h * I, (1 + k * k - h * h) * I, 2 * h]) / s2_
        x, y = r * math.cos(L), r * math.sin(L)
        r_vec = x * fh + y * gh
        xd = -sq * (g + math.sin(L))
        yd = sq * (f + math.cos(L))
        v_vec = xd * fh + yd * gh
    return r_vec, v_vec


def solve_kepler_bisect(M, e):
    lo, hi = 0.0, 2 * math.pi
    for _ in range(200):
        mid = (lo + hi) / 2
        if mid - e * math.sin(mid) < M: lo = mid
        else: hi = mid
    return (lo + hi) / 2


ORBITS = {
    # name: (a, e, i, raan, argp, nu)
    "iss_like":        (6786.0, 0.0005, 51.64, 247.46, 130.54, 325.0),
    "molniya":         (26554.0, 0.7, 63.4, 40.0, 270.0, 120.0),
    "geo_near_equat":  (42164.0, 0.0002, 0.05, 80.0, 45.0, 200.0),
    "polar_leo":       (7200.0, 0.01, 90.0, 120.0, 10.0, 75.0),
    "sun_sync_retro":  (7078.0, 0.001, 98.2, 310.0, 95.0, 250.0),
    "retro_135":       (9000.0, 0.2, 135.0, 200.0, 60.0, 30.0),
    "equatorial_elliptic": (12000.0, 0.3, 0.0, 0.0, 0.0, 100.0),  # raan/argp degenerate, spec 7.6
}

out = {"elements": [], "kepler": [], "frames": []}
for name, (a, e, i, raan, argp, nu) in ORBITS.items():
    r, v = kep_to_rv(a, e, i, raan, argp, nu)
    m = mee_from_rv(r, v)
    r2, v2 = rv_from_mee(m["p"], m["f"], m["g"], m["h"], m["k"], m["L"], m["I"])
    # internal consistency of this independent implementation
    assert np.allclose(r, r2, atol=1e-7) and np.allclose(v, v2, atol=1e-10), (name, r - r2, v - v2)
    out["elements"].append(dict(name=name, kep=[a, e, i, raan, argp, nu], r=r.tolist(), v=v.tolist(), **m))

for M_deg, e in [(235.4, 0.4), (10.0, 0.0), (179.9, 0.95), (359.0, 0.7), (45.0, 0.001)]:
    E = solve_kepler_bisect(math.radians(M_deg), e)
    nu = 2 * math.atan2(math.sqrt(1 + e) * math.sin(E / 2), math.sqrt(1 - e) * math.cos(E / 2))
    out["kepler"].append(dict(M_deg=M_deg, e=e, E_deg=math.degrees(E), nu_deg=math.degrees(nu) % 360))

# ---- frames -------------------------------------------------------------
OBS = dict(lat=30.2672, lon=-97.7431, alt_m=150.0)
CASES = [
    ("2024-03-23T21:31:00", [4000.0, 5000.0, 3000.0], [-3.0, 4.0, 5.0]),
    ("2000-01-01T12:00:00", [-6045.0, -3490.0, 2500.0], [-3.457, 6.618, 2.533]),  # Vallado 3-? style LEO
    ("2025-12-31T23:59:59", [-20000.0, 14000.0, -9000.0], [1.1, 2.2, -0.7]),
    ("2024-06-21T03:07:09", [7000.0, -100.0, 50.0], [0.0, 7.5, 0.1]),
]


def teme_to_ecef(pos, epoch):
    """astropy/ERFA GMST-1982 rotation, UT1=UTC, no polar motion (API contract)."""
    t = Time(epoch, scale="utc")
    gmst = erfa.gmst82(t.jd1, t.jd2)  # ut1 = utc here
    c, s = math.cos(gmst), math.sin(gmst)
    R = np.array([[c, s, 0], [-s, c, 0], [0, 0, 1]])
    return R @ np.array(pos)


for epoch, pos, vel in CASES:
    t = Time(epoch, scale="utc")
    ecef = teme_to_ecef(pos, epoch)
    # velocity: central difference of an inertial straight-line track (independent of the analytic omega x r term)
    dt = 0.01
    pp = np.array(pos) + np.array(vel) * dt
    pm = np.array(pos) - np.array(vel) * dt
    tp = Time(t.jd1, t.jd2 + dt / 86400, format="jd", scale="utc")  # offset the small part: jd1 is too coarse for 10 ms
    tm = Time(t.jd1, t.jd2 - dt / 86400, format="jd", scale="utc")
    gp = erfa.gmst82(tp.jd1, tp.jd2); gm = erfa.gmst82(tm.jd1, tm.jd2)
    def rot(g): c, s = math.cos(g), math.sin(g); return np.array([[c, s, 0], [-s, c, 0], [0, 0, 1]])
    vecef = (rot(gp) @ pp - rot(gm) @ pm) / (2 * dt)
    # geodetic + topocentric: ERFA WGS-84 (gd2gc/gc2gd) and the textbook east/north/up basis.
    # (astropy's ITRS->AltAz path was NOT used: it differs from pure geometry by ~0.15 km here.)
    lon_r, lat_r, h_m = erfa.gc2gd(1, ecef * 1000.0)
    obs = erfa.gd2gc(1, math.radians(OBS["lon"]), math.radians(OBS["lat"]), OBS["alt_m"]) / 1000.0
    phi, lam = math.radians(OBS["lat"]), math.radians(OBS["lon"])
    east = np.array([-math.sin(lam), math.cos(lam), 0.0])
    north = np.array([-math.sin(phi) * math.cos(lam), -math.sin(phi) * math.sin(lam), math.cos(phi)])
    up = np.array([math.cos(phi) * math.cos(lam), math.cos(phi) * math.sin(lam), math.sin(phi)])
    rho = ecef - obs
    e_, n_, u_ = float(rho @ east), float(rho @ north), float(rho @ up)
    rng = float(np.linalg.norm(rho))
    az = math.degrees(math.atan2(e_, n_)) % 360.0
    el = math.degrees(math.asin(u_ / rng))
    ned = [n_, e_, -u_]
    sez = [-n_, e_, u_]
    lat, lon, h = math.degrees(lat_r), math.degrees(lon_r), h_m / 1000.0
    out["frames"].append(dict(epoch=epoch + "Z", teme_pos=pos, teme_vel=vel, ecef_pos=ecef.tolist(),
                              ecef_vel=vecef.tolist(),
                              geodetic=dict(lat=float(lat), lon=float(lon), alt_km=float(h)),
                              observer=OBS, az=float(az), el=float(el), range_km=float(rng),
                              sez=sez, ned=ned))

print(json.dumps(out, indent=1))  # redirect into tests/reference/transform_reference.json
