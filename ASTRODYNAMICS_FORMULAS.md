# 🛰️ Astrea SDA API — Astrodynamics & Mathematical Specification

This document provides the formal mathematical foundations, coordinate frame transformations, astrodynamical models, and numerical algorithms implemented across the Astrea Space Domain Awareness (SDA) platform.

> [!IMPORTANT]
> **Agent Operating Constraint**:  
> Per `AGENTS.md`, this specification is the single source of mathematical truth for Astrea SDA API. Any addition, modification, or refactoring of astrodynamic calculations in `src/services/` **must** be documented here with corresponding mathematical proofs, coordinate frame definitions, and numerical tolerances.

---

## Table of Contents

1. [Fundamental Physical Constants & Standards](#1-fundamental-physical-constants--standards)
2. [Time Systems & Geodetic Coordinate Transformations](#2-time-systems--geodetic-coordinate-transformations)
   - [2.1 Time Scales, Julian Date & Epoch Calculations](#21-time-scales-julian-date--epoch-calculations)
   - [2.2 Greenwich Mean Sidereal Time (GMST) & Local Sidereal Time (LST)](#22-greenwich-mean-sidereal-time-gmst--local-sidereal-time-lst)
   - [2.3 ECI to ECEF Position & Velocity (Earth Rotation Kinematics)](#23-eci-to-ecef-position--velocity-earth-rotation-kinematics)
   - [2.4 ECEF to WGS-84 Geodetic Coordinates (Bowring's Closed-Form Algorithm)](#24-ecef-to-wgs-84-geodetic-coordinates-bowrings-closed-form-algorithm)
   - [2.5 Topocentric Horizon Look Angles (SEZ Coordinates)](#25-topocentric-horizon-look-angles-sez-coordinates)
3. [Ground Track, Footprints & Spatial Geometry](#3-ground-track-footprints--spatial-geometry)
   - [3.1 Satellite Footprint Radius & Geodesic Boundary Polygons](#31-satellite-footprint-radius--geodesic-boundary-polygons)
   - [3.2 Anti-Meridian Handling & Topology](#32-anti-meridian-handling--topology)
4. [Celestial Ephemerides & Illumination Geometry](#4-celestial-ephemerides--illumination-geometry)
   - [4.1 Solar Ephemeris (ECI)](#41-solar-ephemeris-eci)
   - [4.2 Lunar Ephemeris (ECI)](#42-lunar-ephemeris-eci)
   - [4.3 Dual-Cone Solar Shadow Geometry (Umbra & Penumbra)](#43-dual-cone-solar-shadow-geometry-umbra--penumbra)
   - [4.4 Ground Observer Twilight States & Optical Visibility](#44-ground-observer-twilight-states--optical-visibility)
   - [4.5 Solar & Lunar Satellite Transits](#45-solar--lunar-satellite-transits)
5. [Ground Station Pass Scheduling & Numerical Methods](#5-ground-station-pass-scheduling--numerical-methods)
   - [5.1 Horizon Threshold Root Finding (Bisection Method)](#51-horizon-threshold-root-finding-bisection-method)
   - [5.2 Peak Elevation Refinement (Golden-Section Search)](#52-peak-elevation-refinement-golden-section-search)
   - [5.3 Optical Visual Magnitude Approximation](#53-optical-visual-magnitude-approximation)
6. [Relative Motion & Rendezvous Proximity Operations (RPO)](#6-relative-motion--rendezvous-proximity-operations-rpo)
   - [6.1 Hill / Local-Vertical Local-Horizontal (LVLH) Basis Vectors](#61-hill--local-vertical-local-horizontal-lvlh-basis-vectors)
   - [6.2 Transport Theorem & Apparent Rotating Velocity](#62-transport-theorem--apparent-rotating-velocity)
   - [6.3 Relative Range Rate Invariance](#63-relative-range-rate-invariance)
   - [6.4 Proximity Operations Operational Regimes](#64-proximity-operations-operational-regimes)
7. [Cartesian State Vectors to Osculating Keplerian Elements](#7-cartesian-state-vectors-to-osculating-keplerian-elements)
   - [7.1 Specific Mechanical Energy & Semi-Major Axis](#71-specific-mechanical-energy--semi-major-axis)
   - [7.2 Angular Momentum & Laplace-Runge-Lenz Eccentricity Vector](#72-angular-momentum--laplace-runge-lenz-eccentricity-vector)
   - [7.3 Orbital Plane Orientation ($i, \Omega, \omega$)](#73-orbital-plane-orientation-i-omega-omega)
   - [7.4 Anomaly Conversions (True $\nu$, Eccentric $E$, Mean $M$)](#74-anomaly-conversions-true-nu-eccentric-e-mean-m)
   - [7.5 Orbital Period, Perigee & Apogee Radii](#75-orbital-period-perigee--apogee-radii)
   - [7.6 Non-Singular Angles for Circular & Equatorial Orbits](#76-non-singular-angles-for-circular--equatorial-orbits)
8. [RF Doppler Shift & Communications](#8-rf-doppler-shift--communications)
   - [8.1 Line-of-Sight Range Rate in ECEF](#81-line-of-sight-range-rate-in-ecef)
   - [8.2 Electromagnetic Doppler Equation (Classical & Relativistic)](#82-electromagnetic-doppler-equation-classical--relativistic)
9. [Atmospheric Drag, Decay Risk & Catalog Lifetime Assessment](#9-atmospheric-drag-decay-risk--catalog-lifetime-assessment)
   - [9.1 SGP4 Ballistic Drag Parameter ($B^*$) Physics](#91-sgp4-ballistic-drag-parameter-b-physics)
   - [9.2 Secular Orbit Decay & Energy Loss Rate](#92-secular-orbit-decay--energy-loss-rate)
   - [9.3 King-Hele Analytical Lifetime Approximations](#93-king-hele-analytical-lifetime-approximations)
   - [9.4 NASA/NORAD Re-entry Risk Regimes](#94-nasanorad-re-entry-risk-regimes)
10. [Conjunction Assessment & Foster Collision Probability ($P_c$)](#10-conjunction-assessment--foster-collision-probability-p_c)
    - [10.1 Conjunction Screening & Time of Closest Approach (TCA)](#101-conjunction-screening--time-of-closest-approach-tca)
    - [10.2 Encounter Plane (B-Plane) Coordinates](#102-encounter-plane-b-plane-coordinates)
    - [10.3 Combined Positional Covariance Projection](#103-combined-positional-covariance-projection)
    - [10.4 Hard-Body Radius (HBR) Sphere Assumption](#104-hard-body-radius-hbr-sphere-assumption)
    - [10.5 Foster 2D Integral & Akella-Alfriend Closed Form](#105-foster-2d-integral--akella-alfriend-closed-form)
    - [10.6 Probability Dilution Region & Operational Thresholds](#106-probability-dilution-region--operational-thresholds)
11. [Maneuver Reconstruction & Statistical Anomaly Detection](#11-maneuver-reconstruction--statistical-anomaly-detection)
    - [11.1 Mean Motion Derivative Fields & Baseline Drift Model](#111-mean-motion-derivative-fields--baseline-drift-model)
    - [11.2 Semi-Major Axis Residuals & Maneuver Classification](#112-semi-major-axis-residuals--maneuver-classification)
    - [11.3 Non-Parametric Outlier Scoring via Median Absolute Deviation (MAD)](#113-non-parametric-outlier-scoring-via-median-absolute-deviation-mad)
12. [Element Representations & Coordinate Frame Transforms](#12-element-representations--coordinate-frame-transforms-v1satellitestransforms)

---

## 1. Fundamental Physical Constants & Standards

Astrea SDA operations standardize on the **WGS-84** ellipsoidal reference model and the **IAU / EGM-96** standard gravitational parameter:

| Constant | Symbol | Value | Units | Reference |
|---|---|---|---|---|
| Earth Gravitational Parameter | $\mu_{\oplus}$ | $398600.4418$ | $\text{km}^3 / \text{s}^2$ | WGS-84 / EGM-96 |
| Earth Equatorial Radius | $R_E$ | $6378.137$ | $\text{km}$ | WGS-84 |
| Earth Flattening Factor | $f$ | $1 / 298.257223563$ | dimensionless | WGS-84 |
| Earth Polar Semi-Minor Axis | $b = R_E(1-f)$ | $6356.7523142$ | $\text{km}$ | Derived |
| First Eccentricity Squared | $e^2 = 2f - f^2$ | $6.69437999014 \times 10^{-3}$ | dimensionless | Derived |
| Second Eccentricity Squared | $e'^2 = \frac{a^2-b^2}{b^2}$ | $6.73949674228 \times 10^{-3}$ | dimensionless | Derived |
| Earth Angular Rotation Rate | $\omega_{\oplus}$ | $7.2921151467 \times 10^{-5}$ | $\text{rad} / \text{s}$ | IERS Conventions |
| Speed of Light in Vacuum | $c$ | $299792.458$ | $\text{km} / \text{s}$ | CODATA |
| Mean Solar Radius | $R_{\odot}$ | $696340.0$ | $\text{km}$ | IAU |
| Astronomical Unit | $\text{AU}$ | $149597870.7$ | $\text{km}$ | IAU 2012 |
| TT − TAI | — | $32.184$ | $\text{s}$ | IAU (exact) |
| TAI − UTC (since 2017-01-01) | $\Delta AT$ | $37$ | $\text{s}$ | IERS Bulletin C (table lookup; changes at each leap second) |

> [!NOTE]
> **SGP4 constants**: TLE mean elements and the SGP4 propagator are defined with **WGS-72** constants ($\mu = 398600.8\ \text{km}^3/\text{s}^2$, $R_E = 6378.135\ \text{km}$, $J_2 = 1.082616 \times 10^{-3}$, $k_e = 0.0743669161\ \text{ER}^{3/2}/\text{min}$). The WGS-84 values above apply to geodesy and to all post-propagation geometry. The WGS-72 values must be used when recovering the mean semi-major axis from a TLE (§11.2).

---

## 2. Time Systems & Geodetic Coordinate Transformations

### 2.1 Time Scales, Julian Date & Epoch Calculations
The API accepts UTC as Unix milliseconds $t_{\text{ms}}$ (1970-01-01T00:00:00Z). Unix time does not count leap seconds, so it is a valid UTC label, but UTC is **not** a uniform time scale and must never be used directly in celestial formulas. Three scales are carried explicitly:

$$
JD_{\text{UTC}} = \frac{t_{\text{ms}}}{86\,400\,000} + 2\,440\,587.5
$$

$$
JD_{\text{TT}} = JD_{\text{UTC}} + \frac{\Delta AT + 32.184\,\text{s}}{86\,400}, \qquad JD_{\text{UT1}} = JD_{\text{UTC}} + \frac{DUT1}{86\,400}
$$

- $\Delta AT = \text{TAI} - \text{UTC}$ (currently $37\text{ s}$, so $TT - UTC = 69.184\text{ s}$) comes from the IERS leap-second table, selected by epoch.
- $DUT1 = \text{UT1} - \text{UTC}$ (seconds, $|DUT1| < 0.9\text{ s}$) comes from IERS Bulletin A/B. If no EOP data is available, $DUT1 = 0$ and the resulting error bound below applies.
- *Implementation:* `DELTA_AT_S = 37` is applied for all epochs (exact since 2017-01-01; for earlier epochs $TT-UTC$ is off by up to a few seconds, i.e. $\lesssim 10^{-3\,\circ}$ Sun error) and `DUT1_S = 0` (no EOP feed).

Days and Julian centuries from **J2000.0** (2000-01-01 12:00:00 TT, $JD = 2\,451\,545.0$), one per scale:

$$
d_{\text{TT}} = JD_{\text{TT}} - 2\,451\,545.0, \quad d_{\text{UT1}} = JD_{\text{UT1}} - 2\,451\,545.0, \quad T_{\text{TT}} = \frac{d_{\text{TT}}}{36\,525}
$$

**Scale assignment**:

| Quantity | Time scale |
|---|---|
| GMST / LST / Earth rotation angle (§2.2, §2.3) | $d_{\text{UT1}}$ |
| Solar and lunar ephemerides (§4.1, §4.2) | $d_{\text{TT}}$ |
| SGP4 propagation | Minutes since TLE epoch (UTC-labelled, as defined by the TLE format) |

**Error budget when the offset is omitted** (treating UTC as TT or UT1):

| Omitted term | Magnitude | Effect |
|---|---|---|
| $\Delta AT + 32.184\text{ s}$ in solar ephemeris | $69.2\text{ s} \times 0.041''/\text{s} \approx 2.8''$ | $\approx 0.0008^\circ$ in solar longitude |
| $\Delta AT + 32.184\text{ s}$ in lunar ephemeris | $69.2\text{ s} \times 0.55''/\text{s} \approx 38''$ | $\approx 0.01^\circ$, $\approx 70\text{ km}$ of lunar position |
| $DUT1$ in GMST | $\le 0.9\text{ s} \times 15.04''/\text{s} \approx 13.5''$ | $\le 0.0038^\circ$, $\approx 0.4\text{ km}$ at the equator |

In this document, $d$ in §2.2 means $d_{\text{UT1}}$ and $d$ in §4.1–§4.2 means $d_{\text{TT}}$.

### 2.2 Greenwich Mean Sidereal Time (GMST) & Local Sidereal Time (LST)
Greenwich Mean Sidereal Time in degrees is computed via the IAU 1982 formula (Vallado eq. 3-47), with $d_{\text{UT1}}$ from §2.1 and $T = d_{\text{UT1}}/36525$ Julian centuries. The $T^2$ and $T^3$ terms are kept: omitting the $T^2$ term costs $0.000388^\circ\,T^2 \approx 0.09''$ ($\approx 3$ m) in 2026 and grows quadratically.

$$
GMST = \left( 280.46061837^\circ + 360.98564736629^\circ \cdot d_{\text{UT1}} + 0.000387933^\circ\, T^2 - \frac{T^3}{38\,710\,000}\,{}^\circ \right) \pmod{360^\circ}
$$

Guaranteed strictly positive in $[0^\circ, 360^\circ)$.

For an observer at East geodetic longitude $\lambda_{\text{deg}}$:

$$
\theta_{\text{LST}} = \left( GMST(d_{\text{UT1}}) + \lambda_{\text{deg}} \right) \pmod{360^\circ}
$$

In radians: $\theta_{\text{LST, rad}} = \theta_{\text{LST}} \cdot \frac{\pi}{180^\circ}$.

### 2.3 ECI to ECEF Position & Velocity (Earth Rotation Kinematics)
**Frame assumptions.** SGP4 outputs are in **TEME** (True Equator, Mean Equinox of date), not J2000. The full chain to a terrestrial frame is:

$$
\text{TEME} \xrightarrow{\;\mathbf{R}_z(GMST)\;} \text{PEF} \xrightarrow{\;\mathbf{W}(x_p, y_p)\;} \text{ITRF (ECEF)}
$$

- TEME already removes precession and nutation by construction, so **no precession/nutation matrices are applied**; the rotation uses GMST (not GAST).
- This specification implements TEME $\to$ PEF only and treats **PEF as ECEF**. The polar-motion matrix $\mathbf{W}$ is omitted: $|x_p|, |y_p| \lesssim 0.3'' \approx 10\text{ m}$ at the surface, well below the meter-class-to-kilometer-class accuracy of TLE-based SGP4 (~1 km at epoch).
- Inputs given in J2000/GCRF/ICRF are **not** TEME and must be reduced first (IAU-2006/2000A CIO chain, or FK5 precession-nutation); that reduction is outside this specification.
- The Sun/Moon ephemerides of §4 are referred to the mean equator and equinox of date, i.e. TEME to within the equation of the equinoxes ($\lesssim 1''$), and may be combined directly with TEME satellite vectors.

The TEME $\to$ PEF/ECEF transformation is a rotation around the $Z$-axis by $\theta = GMST(d_{\text{UT1}})$:

$$
\mathbf{R}_z(\theta) = \begin{bmatrix}
\cos\theta & \sin\theta & 0 \\
-\sin\theta & \cos\theta & 0 \\
0 & 0 & 1
\end{bmatrix}
$$

**Position Transformation**:

$$
\vec{r}_{\text{ECEF}} = \mathbf{R}_z(\theta) \vec{r}_{\text{ECI}} = \begin{bmatrix}
x_{\text{ECI}}\cos\theta + y_{\text{ECI}}\sin\theta \\
-x_{\text{ECI}}\sin\theta + y_{\text{ECI}}\cos\theta \\
z_{\text{ECI}}
\end{bmatrix}
$$

**Velocity Transformation (Kinematic Transport Theorem)**:
Because ECEF rotates with constant angular velocity $\vec{\omega}_{\oplus} = [0, 0, \omega_{\oplus}]^T$:

$$
\left(\frac{d\vec{r}}{dt}\right)_{\text{ECI}} = \left(\frac{d\vec{r}}{dt}\right)_{\text{ECEF}} + \vec{\omega}_{\oplus} \times \vec{r}_{\text{ECI}}
$$

$$
\vec{v}_{\text{ECEF}} = \mathbf{R}_z(\theta) \left( \vec{v}_{\text{ECI}} - \vec{\omega}_{\oplus} \times \vec{r}_{\text{ECI}} \right)
$$

Evaluating the cross product $\vec{\omega}_{\oplus} \times \vec{r}_{\text{ECI}} = [-\omega_{\oplus} y, \omega_{\oplus} x, 0]^T$:

$$
\vec{v}_{\text{eff}} = \vec{v}_{\text{ECI}} - \vec{\omega}_{\oplus} \times \vec{r}_{\text{ECI}} = \begin{bmatrix}
v_x + \omega_{\oplus} y \\
v_y - \omega_{\oplus} x \\
v_z
\end{bmatrix}
$$

$$
\vec{v}_{\text{ECEF}} = \mathbf{R}_z(\theta) \vec{v}_{\text{eff}} = \begin{bmatrix}
v_{\text{eff}, x}\cos\theta + v_{\text{eff}, y}\sin\theta \\
-v_{\text{eff}, x}\sin\theta + v_{\text{eff}, y}\cos\theta \\
v_z
\end{bmatrix}
$$

### 2.4 ECEF to WGS-84 Geodetic Coordinates (Bowring's Closed-Form Algorithm)
Given Cartesian coordinates $[x, y, z]^T$ in ECEF (km), Bowring's 1976 closed-form vector method avoids transcendental iterations and provides sub-millimeter precision for $|h| < 10\,000\text{ km}$:

1. Distance from Earth rotation axis:
   $$
   p = \sqrt{x^2 + y^2}
   $$
2. Geodetic longitude:
   $$
   \lambda = \operatorname{atan2}(y, x)
   $$
3. Polar singularity check ($p < 10^{-6}\text{ km}$):
   $$
   \phi = \begin{cases}
   +90^\circ, & z \ge 0 \\
   -90^\circ, & z < 0
   \end{cases}, \quad h = |z| - b
   $$
4. Parametric (reduced) latitude $\theta$:
   $$
   \theta = \operatorname{atan2}(z \cdot R_E, p \cdot b)
   $$
5. Geodetic latitude $\phi$:
   $$
   \phi = \operatorname{atan2}\left( z + e'^2 b \sin^3\theta, \; p - e^2 R_E \cos^3\theta \right)
   $$
6. Prime vertical radius of curvature $N(\phi)$:
   $$
   N(\phi) = \frac{R_E}{\sqrt{1 - e^2 \sin^2\phi}}
   $$
7. Ellipsoidal altitude $h$:
   $$
   h = \frac{p}{\cos\phi} - N(\phi)
   $$

### 2.5 Topocentric Horizon Look Angles (SEZ Coordinates)
The observer's ECEF coordinates at geodetic latitude $\phi$, longitude $\lambda$, and ellipsoidal height $h_{\text{km}}$:

$$
C = \frac{1}{\sqrt{1 - e^2 \sin^2\phi}}, \quad S = C(1-f)^2
$$

$$
\vec{r}_{\text{obs}} = \begin{bmatrix}
(R_E C + h)\cos\phi\cos\lambda \\
(R_E C + h)\cos\phi\sin\lambda \\
(R_E S + h)\sin\phi
\end{bmatrix}_{\text{ECEF}}
$$

The slant range vector in ECEF is:

$$
\vec{\rho}_{\text{ECEF}} = \vec{r}_{\text{sat, ECEF}} - \vec{r}_{\text{obs, ECEF}} = \begin{bmatrix}
r_x \\
r_y \\
r_z
\end{bmatrix}
$$

Transforming into the **Topocentric Horizon (SEZ: South, East, Zenith)** frame:

$$
\begin{bmatrix}
\rho_S \\
\rho_E \\
\rho_Z
\end{bmatrix} = \begin{bmatrix}
\sin\phi\cos\lambda & \sin\phi\sin\lambda & -\cos\phi \\
-\sin\lambda & \cos\lambda & 0 \\
\cos\phi\cos\lambda & \cos\phi\sin\lambda & \sin\phi
\end{bmatrix} \begin{bmatrix}
r_x \\
r_y \\
r_z
\end{bmatrix}
$$

Scalar slant range:

$$
\rho = \|\vec{\rho}\| = \sqrt{r_x^2 + r_y^2 + r_z^2}
$$

Topocentric elevation ($El$) and geographic azimuth ($Az$):

$$
El = \arcsin\left(\frac{\rho_Z}{\rho}\right)
$$

$$
Az = \operatorname{atan2}(\rho_E, -\rho_S) \pmod{360^\circ}
$$

*(Because $-\rho_S = \rho_N$, $\operatorname{atan2}(\rho_E, \rho_N)$ represents True North-referenced clockwise azimuth: $0^\circ = \text{North}, 90^\circ = \text{East}, 180^\circ = \text{South}, 270^\circ = \text{West}$).*

---

## 3. Ground Track, Footprints & Spatial Geometry

### 3.1 Satellite Footprint Radius & Geodesic Boundary Polygons
The instantaneous sub-satellite footprint is modeled as the horizon visibility cap on a sphere whose radius is the **local geocentric radius of the WGS-84 ellipsoid** at the sub-satellite point, not the constant equatorial $R_E$ (the polar radius is $\approx 21.4\text{ km}$ smaller):

$$
R_{\phi} = \frac{R_E\, b}{\sqrt{b^2\cos^2\phi_{gc} + R_E^2 \sin^2\phi_{gc}}}, \qquad \tan\phi_{gc} = (1 - e^2)\tan\phi_0
$$

For altitude $h_{\text{km}}$ above the ellipsoid:

$$
\cos\sigma = \frac{R_{\phi}}{R_{\phi} + \max(h, 0)}
$$

$$
\sigma = \arccos\left(\frac{R_{\phi}}{R_{\phi} + \max(h, 0)}\right) \quad (\text{Central Earth Angular Radius in radians})
$$

$$
r_{\text{footprint}} = R_{\phi} \cdot \sigma \quad (\text{Surface distance in km})
$$

*(Approximation: the true ellipsoidal horizon is not exactly a circle; $R_\phi$ removes the dominant latitude-dependent error, leaving a residual of order the curvature variation, $\lesssim 10\text{ km}$ in footprint radius.)*

The 36 boundary vertices ($k = 0, \dots, 36$) around azimuth $\alpha_k = k \cdot 10^\circ$ from the sub-satellite point $(\phi_0, \lambda_0)$:

$$
\sin\phi_k = \sin\phi_0 \cos\sigma + \cos\phi_0 \sin\sigma \cos\alpha_k
$$

$$
\Delta\lambda_k = \operatorname{atan2}\left(\sin\alpha_k \sin\sigma \cos\phi_0, \; \cos\sigma - \sin\phi_0 \sin\phi_k\right)
$$

$$
\lambda_k = \left(\lambda_0 + \Delta\lambda_k\right) \pmod{360^\circ}
$$

Normalized to $[-180^\circ, +180^\circ]$.

### 3.2 Anti-Meridian Handling & Topology
**Polar guard (evaluated first).** If the cap contains a pole:

$$
|\phi_0| + \sigma \ge 90^\circ
$$

then $\cos\phi_0 \to 0$ makes $\sin\sigma / \cos\phi_0$ ill-conditioned and $\Delta\lambda_{\text{half}}$ is meaningless. The footprint is treated as covering the pole and is not representable as a single simple polygon ring: `footprint_ring_is_simple` returns false and no ring is emitted (no polar-cap bounding box is constructed). The division below is never evaluated in this case. (Note $\sin\sigma \ge \cos\phi_0 \iff |\phi_0| + \sigma \ge 90^\circ$, so the guard is exactly the condition under which the clamp below would saturate.)

Otherwise, a spherical cap is topologically simple on a planar map projection if:
1. It does not enclose a geographic pole (guard above fails):
   $$
   |\phi_0| + \sigma_{\text{deg}} < 90^\circ
   $$
2. Its maximum longitude half-width does not cross $\pm 180^\circ$:
   $$
   \Delta\lambda_{\text{half}} = \arcsin\left(\min\left(1.0, \frac{\sin\sigma}{\cos\phi_0}\right)\right)
   $$
   $$
   \lambda_0 - \Delta\lambda_{\text{half}} > -180^\circ \quad \text{and} \quad \lambda_0 + \Delta\lambda_{\text{half}} < 180^\circ
   $$

When continuous trajectory LineStrings traverse the $\pm 180^\circ$ meridian ($|\lambda_{k} - \lambda_{k-1}| > 180^\circ$), the geometry is dynamically segmented into a GeoJSON `MultiLineString` to prevent rendering wraparound streaks.

---

## 4. Celestial Ephemerides & Illumination Geometry

### 4.1 Solar Ephemeris (ECI)
The Sun's position vector in ECI (mean equator and equinox of date $\approx$ TEME) is computed via the low-precision analytical ephemeris (Astronomical Almanac / USNO). Here $d = d_{\text{TT}}$ (§2.1):
1. Mean anomaly of the Sun:
   $$
   M_{\odot} = \left( 357.529^\circ + 0.98560028^\circ \cdot d \right) \pmod{360^\circ}
   $$
2. Mean longitude of the Sun:
   $$
   q = \left( 280.459^\circ + 0.98564736^\circ \cdot d \right) \pmod{360^\circ}
   $$
3. Ecliptic longitude:
   $$
   \lambda_{\odot} = q + 1.915^\circ \sin M_{\odot} + 0.020^\circ \sin(2 M_{\odot})
   $$
4. Distance from Earth (km):
   $$
   R_{\odot} = (1.00014 - 0.01671 \cos M_{\odot} - 0.00014 \cos(2 M_{\odot})) \times 149\,597\,870.7
   $$
5. Obliquity of the ecliptic:
   $$
   \epsilon = 23.439^\circ - 0.00000036^\circ \cdot d
   $$
6. ECI Position:
   $$
   \vec{r}_{\odot} = \begin{bmatrix}
   R_{\odot} \cos\lambda_{\odot} \\
   R_{\odot} \cos\epsilon \sin\lambda_{\odot} \\
   R_{\odot} \sin\epsilon \sin\lambda_{\odot}
   \end{bmatrix}
   $$

### 4.2 Lunar Ephemeris (ECI)
The Moon's position vector in ECI (mean equator and equinox of date $\approx$ TEME) is determined via a truncated Meeus lunar expansion (Chapter 47 reduced to its leading term in each coordinate), with $d = d_{\text{TT}}$ (§2.1) and the obliquity $\epsilon$ of §4.1 step 5:
1. Fundamental arguments:
   $$
   L' = (218.316^\circ + 13.176396^\circ \cdot d) \pmod{360^\circ} \quad (\text{Mean Longitude})
   $$
   $$
   M' = (134.963^\circ + 13.064993^\circ \cdot d) \pmod{360^\circ} \quad (\text{Mean Anomaly})
   $$
   $$
   F = (93.272^\circ + 13.229350^\circ \cdot d) \pmod{360^\circ} \quad (\text{Argument of Latitude})
   $$
2. Geocentric distance and ecliptic coordinates:
   $$
   \lambda_{☾} = L' + 6.289^\circ \sin M'
   $$
   $$
   \beta_{☾} = 5.128^\circ \sin F
   $$
   $$
   R_{☾} = 385\,001.0 - 20\,905.0 \cos M' \quad (\text{km})
   $$
3. ECI transformation:
   $$
   \vec{r}_{☾} = \begin{bmatrix}
   R_{☾} \cos\beta_{☾} \cos\lambda_{☾} \\
   R_{☾} (\cos\beta_{☾} \sin\lambda_{☾} \cos\epsilon - \sin\beta_{☾} \sin\epsilon) \\
   R_{☾} (\cos\beta_{☾} \sin\lambda_{☾} \sin\epsilon + \sin\beta_{☾} \cos\epsilon)
   \end{bmatrix}
   $$

**Accuracy.** Only the equation-of-centre term is kept. The largest neglected terms (Meeus Table 47.A/47.B) are the evection ($1.274^\circ$ in longitude, $\approx 3\,700\text{ km}$ in distance), the variation ($0.658^\circ$, $\approx 3\,000\text{ km}$), the $2M'$ term ($0.213^\circ$) and the annual equation ($0.186^\circ$), so the longitude error can reach $\sim 2^\circ$ and the distance error several thousand km. The Moon's angular diameter is $\approx 0.5^\circ$, so lunar transit results are screening-level only; use a full lunar theory or JPL ephemeris for event-grade timing.

### 4.3 Dual-Cone Solar Shadow Geometry (Umbra & Penumbra)
To determine whether a satellite is in Earth's shadow without spherical cylinder simplifications:
1. Relative Sun vector:
   $$
   \vec{d} = \vec{r}_{\odot} - \vec{r}_{\text{sat}}, \quad d = \|\vec{d}\|
   $$
2. Apparent angular radii of Earth and Sun subtended at the satellite. The Earth radius is the local geocentric radius $R_{\phi}$ of §3.1 evaluated at the satellite's geocentric latitude $\phi_{gc} = \arcsin(z_{\text{sat}}/\|\vec{r}_{\text{sat}}\|)$, not the constant equatorial $R_E$ (an equatorial-radius sphere overstates the shadow at high latitude by up to $21.4\text{ km}$, shifting LEO umbra/penumbra entry and exit times by seconds):
   $$
   \theta_E = \arcsin\left(\frac{R_{\phi}}{\|\vec{r}_{\text{sat}}\|}\right)
   $$
   $$
   \theta_{\odot} = \arcsin\left(\frac{R_{\odot}}{d}\right)
   $$
3. Angle between Earth center ($-\vec{r}_{\text{sat}}$) and Sun ($\vec{d}$):
   $$
   \cos\theta = \frac{-\vec{r}_{\text{sat}} \cdot \vec{d}}{\|\vec{r}_{\text{sat}}\| \cdot d}, \quad \theta = \arccos(\text{clamp}(\cos\theta, -1.0, 1.0))
   $$
4. **Lighting State Classification**:
   $$
   \text{State} = \begin{cases}
   \mathbf{Umbra} \quad (\text{Total eclipse}), & \theta < \theta_E - \theta_{\odot} \\
   \mathbf{Penumbra} \quad (\text{Partial eclipse}), & |\theta_E - \theta_{\odot}| \le \theta < \theta_E + \theta_{\odot} \\
   \mathbf{FullSunlight}, & \theta \ge \theta_E + \theta_{\odot}
   \end{cases}
   $$

### 4.4 Ground Observer Twilight States & Optical Visibility
Observer sun elevation $El_{\odot}$ determines ambient optical background sky conditions:

$$
\text{Twilight State} = \begin{cases}
\mathbf{Daylight}, & El_{\odot} > 0^\circ \\
\mathbf{CivilTwilight}, & 0^\circ \ge El_{\odot} > -6^\circ \\
\mathbf{NauticalTwilight}, & -6^\circ \ge El_{\odot} > -12^\circ \\
\mathbf{AstronomicalTwilight}, & -12^\circ \ge El_{\odot} > -18^\circ \\
\mathbf{Night}, & El_{\odot} \le -18^\circ
\end{cases}
$$

A satellite is optically observable if:

$$
\text{is\_visibly\_observable} \iff (\text{LightingState} \ne \mathbf{Umbra}) \land (El_{\odot} \le -6.0^\circ) \land (El_{\text{sat}} > 0.0^\circ)
$$

### 4.5 Solar & Lunar Satellite Transits
For an observer looking along unit vector $\hat{u}_{\text{target}}$ towards the center of the Sun or Moon, and along unit vector $\hat{u}_{\text{sat}}$ towards the satellite:

$$
\hat{u} = \begin{bmatrix}
\cos(El)\sin(Az) \\
\cos(El)\cos(Az) \\
\sin(El)
\end{bmatrix}
$$

$$
\Delta\theta = \arccos\left(\text{clamp}(\hat{u}_{\text{sat}} \cdot \hat{u}_{\text{target}}, -1.0, 1.0)\right)
$$

A transit event occurs when $\Delta\theta \le \theta_{\text{threshold}}$ while both bodies have positive elevation above the local horizon.

*Implementation:* detection samples $\Delta\theta$ on a 1-minute grid, so a transit shorter than the grid spacing can be missed unless $\theta_{\text{threshold}}$ is large enough to span it. Each run of consecutive in-threshold samples is reported as one transit. Its start and end are the boundaries of $\Delta\theta \le \theta_{\text{threshold}}$ (or elevation $\le 0$), found by bisection between the adjacent samples to $0.1\text{ s}$; its centre is the minimum of $\Delta\theta$ within one minute of the best sample (ternary search, $0.1\text{ s}$). A run touching the start or end of the forecast window is clipped to that window. Duration is end minus start.

---

## 5. Ground Station Pass Scheduling & Numerical Methods

### 5.1 Horizon Threshold Root Finding (Bisection Method)
For a target elevation threshold $El_{\text{th}}$:

$$
g(t) = El(t) - El_{\text{th}} = 0
$$

When coarse 60-second scanning brackets a crossing $[t_{\text{below}}, t_{\text{above}}]$:

$$
t_{\text{mid}} = t_{\text{below}} + \frac{t_{\text{above}} - t_{\text{below}}}{2}
$$

The bisection loop iterates until convergence:

$$
|t_{\text{above}} - t_{\text{below}}| \le 1000\text{ ms}
$$

Yielding sub-second resolution for Acquisition of Signal ($t_{\text{AOS}}$) and Loss of Signal ($t_{\text{LOS}}$).

### 5.2 Peak Elevation Refinement (Golden-Section Search)
Between AOS and LOS, single-pass elevation is strictly unimodal. To refine the peak without numerical derivatives, the Golden-Section Search operates on interval $[a, b]$ with ratio $\tau$:

$$
\tau = \frac{\sqrt{5}-1}{2} \approx 0.61803398875
$$

$$
m_1 = b - \tau(b - a), \quad m_2 = a + \tau(b - a)
$$

Since $\tau > 0.5$, $m_1 < m_2$.
- If $El(m_1) > El(m_2)$, then $b \leftarrow m_2$.
- Else, $a \leftarrow m_1$.

Convergence terminates when $(b - a) \le 1000\text{ ms}$, isolating the Time of Closest Approach ($t_{\text{TCA}}$) and maximum elevation ($El_{\text{max}}$).

### 5.3 Optical Visual Magnitude Approximation
When a satellite is visibly observable at slant range $\rho_{\text{km}}$ and solar phase angle $\gamma$, the satellite is modeled as a diffuse (Lambertian) sphere:

$$
m_v \approx M_0 + 5 \log_{10}\left( \max\left( \frac{\rho}{1000\text{ km}}, 0.1 \right) \right) - 2.5 \log_{10}\left( \max\left(\Phi(\gamma), \Phi_{\min}\right) \right)
$$

$$
\Phi(\gamma) = \frac{\sin\gamma + (\pi - \gamma)\cos\gamma}{\pi}, \qquad \Phi(0) = 1, \;\; \Phi(\tfrac{\pi}{2}) = \tfrac{1}{\pi}, \;\; \Phi(\pi) = 0
$$

with $\Phi_{\min} = 10^{-3}$ preventing divergence for fully backlit geometry ($\gamma \to \pi$).

**Phase angle** $\gamma$ is the Sun–satellite–observer angle at the satellite, with all vectors in the same frame (TEME) at the same epoch ($\vec{r}_{\text{obs,ECI}} = \mathbf{R}_z(\theta)^T \vec{r}_{\text{obs,ECEF}}$, $\vec{r}_\odot$ from §4.1):

$$
\cos\gamma = \frac{(\vec{r}_{\odot} - \vec{r}_{\text{sat}}) \cdot (\vec{r}_{\text{obs}} - \vec{r}_{\text{sat}})}{\|\vec{r}_{\odot} - \vec{r}_{\text{sat}}\| \; \|\vec{r}_{\text{obs}} - \vec{r}_{\text{sat}}\|}, \quad \gamma = \arccos(\text{clamp}(\cos\gamma, -1.0, 1.0))
$$

$M_0 = 2.5$ is the magnitude at $\rho = 1000\text{ km}$ and **zero phase** ($\gamma = 0$). A catalog value quoted at the common $90^\circ$-phase convention converts as $M_0(0^\circ) = M_0(90^\circ) - 1.24$. Real satellites are partly specular and attitude dependent, so this is a planning-grade estimate (typically $\pm 1$–$2$ mag), not a photometric model.

---

## 6. Relative Motion & Rendezvous Proximity Operations (RPO)

### 6.1 Hill / Local-Vertical Local-Horizontal (LVLH) Basis Vectors
Centered on the primary (chief) satellite at position $\vec{r}_p$ and velocity $\vec{v}_p$:

1. **Radial Unit Vector**:
   $$
   \hat{e}_R = \frac{\vec{r}_p}{\|\vec{r}_p\|}
   $$
2. **Cross-Track Unit Vector (Orbital Momentum Normal)**:
   $$
   \vec{h}_p = \vec{r}_p \times \vec{v}_p, \quad \hat{e}_C = \frac{\vec{h}_p}{\|\vec{h}_p\|}
   $$
3. **In-Track Unit Vector**:
   $$
   \hat{e}_I = \hat{e}_C \times \hat{e}_R
   $$

Right-handedness verification:

$$
\hat{e}_R \times \hat{e}_I = \hat{e}_R \times (\hat{e}_C \times \hat{e}_R) = (\hat{e}_R \cdot \hat{e}_R)\hat{e}_C - (\hat{e}_R \cdot \hat{e}_C)\hat{e}_R = \hat{e}_C
$$

Relative position of target $\vec{r}_t$:

$$
\delta\vec{r} = \vec{r}_t - \vec{r}_p
$$

$$
x_R = \delta\vec{r} \cdot \hat{e}_R, \quad y_I = \delta\vec{r} \cdot \hat{e}_I, \quad z_C = \delta\vec{r} \cdot \hat{e}_C
$$

$$
\rho = \|\delta\vec{r}\| = \sqrt{x_R^2 + y_I^2 + z_C^2}
$$

### 6.2 Transport Theorem & Apparent Rotating Velocity
The orbital angular velocity vector of the primary orbit is:

$$
\vec{\omega}_{\text{orb}} = \frac{\vec{h}_p}{\|\vec{r}_p\|^2} = \dot{\nu} \hat{e}_C
$$

By the kinematic transport theorem:

$$
\left(\frac{d(\delta\vec{r})}{dt}\right)_{\text{inertial}} = \left(\frac{d(\delta\vec{r})}{dt}\right)_{\text{rotating}} + \vec{\omega}_{\text{orb}} \times \delta\vec{r}
$$

Given inertial relative velocity $\delta\vec{v} = \vec{v}_t - \vec{v}_p$, the apparent relative velocity vector in the rotating Hill frame is:

$$
\vec{v}_{\text{rel, rot}} = \delta\vec{v} - \vec{\omega}_{\text{orb}} \times \delta\vec{r}
$$

$$
v_R = \vec{v}_{\text{rel, rot}} \cdot \hat{e}_R, \quad v_I = \vec{v}_{\text{rel, rot}} \cdot \hat{e}_I, \quad v_C = \vec{v}_{\text{rel, rot}} \cdot \hat{e}_C
$$

### 6.3 Relative Range Rate Invariance
$$\dot{\rho} = \frac{\delta\vec{r} \cdot \delta\vec{v}}{\rho}$$

Because $\delta\vec{r} \cdot (\vec{\omega}_{\text{orb}} \times \delta\vec{r}) \equiv 0$, the range rate evaluates identically in both inertial and rotating frames:

$$
\frac{\delta\vec{r} \cdot \vec{v}_{\text{rel, rot}}}{\rho} = \frac{\delta\vec{r} \cdot \delta\vec{v}}{\rho} = \dot{\rho}
$$

### 6.4 Proximity Operations Operational Regimes
The separation distance $\rho$ is categorized according to standard DoD/NASA RPO safety standards:
- **$\rho < 1\text{ km}$**: Mating / Docking Box
- **$1 \le \rho < 10\text{ km}$**: Close Proximity / Inspection Zone
- **$10 \le \rho < 100\text{ km}$**: Intermediate Proximity Operations
- **$100 \le \rho < 1000\text{ km}$**: Far-Field Rendezvous Operations
- **$\rho \ge 1000\text{ km}$**: Co-orbital Drift / Distant Tracking

---

## 7. Cartesian State Vectors to Osculating Keplerian Elements

Given Cartesian position $\vec{r}$ and velocity $\vec{v}$ in ECI (TEME):

$$
r = \|\vec{r}\|, \quad v = \|\vec{v}\|
$$

### 7.1 Specific Mechanical Energy & Semi-Major Axis
$$
\mathcal{E} = \frac{v^2}{2} - \frac{\mu_{\oplus}}{r}
$$

$$
a = -\frac{\mu_{\oplus}}{2\mathcal{E}} = \frac{1}{\frac{2}{r} - \frac{v^2}{\mu_{\oplus}}}
$$

### 7.2 Angular Momentum & Laplace-Runge-Lenz Eccentricity Vector
$$
\vec{h} = \vec{r} \times \vec{v}, \quad h = \|\vec{h}\|
$$

$$
\vec{e} = \frac{1}{\mu_{\oplus}} \left[ \left(v^2 - \frac{\mu_{\oplus}}{r}\right)\vec{r} - (\vec{r}\cdot\vec{v})\vec{v} \right], \quad e = \|\vec{e}\|
$$

### 7.3 Orbital Plane Orientation ($i, \Omega, \omega$)
1. **Inclination**:
   $$
   i = \arccos\left(\text{clamp}\left(\frac{h_z}{h}, -1.0, 1.0\right)\right) \in [0, \pi]
   $$
2. **Line of Nodes**:
   $$
   \vec{n} = \hat{k} \times \vec{h} = [-h_y, h_x, 0]^T, \quad n = \|\vec{n}\| = \sqrt{h_x^2 + h_y^2}
   $$
3. **Right Ascension of Ascending Node ($\Omega$)**:
   $$
   \Omega = \begin{cases}
   \arccos\left(\frac{n_x}{n}\right), & n_y \ge 0 \\
   360^\circ - \arccos\left(\frac{n_x}{n}\right), & n_y < 0
   \end{cases} \quad (\text{undefined if } n/h < 10^{-8}, \text{ i.e. } \sin i; \text{ see §7.6})
   $$
4. **Argument of Perigee ($\omega$)**:
   $$
   \omega = \begin{cases}
   \arccos\left(\frac{\vec{n}\cdot\vec{e}}{n e}\right), & e_z \ge 0 \\
   360^\circ - \arccos\left(\frac{\vec{n}\cdot\vec{e}}{n e}\right), & e_z < 0
   \end{cases} \quad (\text{undefined if } n/h < 10^{-8} \lor e < 10^{-6}; \text{ see §7.6})
   $$

### 7.4 Anomaly Conversions (True $\nu$, Eccentric $E$, Mean $M$)
1. **True Anomaly**:
   $$
   \nu = \begin{cases}
   \arccos\left(\frac{\vec{e}\cdot\vec{r}}{e r}\right), & \vec{r}\cdot\vec{v} \ge 0 \\
   360^\circ - \arccos\left(\frac{\vec{e}\cdot\vec{r}}{e r}\right), & \vec{r}\cdot\vec{v} < 0
   \end{cases} \quad (\text{undefined if } e < 10^{-6}; \text{ see §7.6})
   $$
2. **Eccentric Anomaly via Half-Angle / Quadrant-Safe Trigonometry**:
   $$
   \cos E = \frac{e + \cos\nu}{1 + e\cos\nu}, \quad \sin E = \frac{\sqrt{\max(0, 1 - e^2)}\sin\nu}{1 + e\cos\nu}
   $$
   $$
   E = \operatorname{atan2}(\sin E, \cos E)
   $$
3. **Mean Anomaly (Kepler's Equation)**:
   $$
   M = \left( E - e \sin E \right) \pmod{2\pi}
   $$

### 7.5 Orbital Period, Perigee & Apogee Radii
$$
T_{\text{period}} = \frac{2\pi \sqrt{a^3 / \mu_{\oplus}}}{60} \quad (\text{minutes})
$$

$$
h_p = a(1 - e) - R(\phi_{gc}) \quad (\text{Perigee Altitude in km})
$$

$$
h_a = a(1 + e) - R(\phi_{gc}) \quad (\text{Apogee Altitude in km})
$$

where $R(\phi_{gc})$ is the WGS-84 geocentric radius (§3.1) at the geocentric latitude of the apsides, $\sin\phi_{gc} = \pm\sin i \sin\omega$ (the perigee and apogee share one radius because $R$ is even in $\phi_{gc}$). Using the equatorial $R_E$ instead overstates altitude by up to $21\text{ km}$ for a polar perigee.

### 7.6 Non-Singular Angles for Circular & Equatorial Orbits
For $e < 10^{-6}$ there is no perigee, so $\omega$ and $\nu$ are individually undefined; for $n < 10^{-8}$ ($i \approx 0^\circ$ or $180^\circ$) there is no ascending node, so $\Omega$ is undefined. Merely setting the undefined angle to $0^\circ$ discards the satellite's position along the orbit. The defined combined angle is computed instead, and folded into the nearest classical angle so that position remains recoverable:

| Case | Undefined | Defined angle | Reported as |
|---|---|---|---|
| Circular, inclined ($e < 10^{-6}$, $n \ge 10^{-8}$) | $\omega, \nu$ | Argument of latitude $u = \omega + \nu$ | $\omega = 0$, $\nu = u$ |
| Elliptical, equatorial ($e \ge 10^{-6}$, $n < 10^{-8}$) | $\Omega$ | Longitude of periapsis $\varpi = \Omega + \omega$ | $\Omega = 0$, $\omega = \varpi$ |
| Circular, equatorial | $\Omega, \omega, \nu$ | True longitude $l = \Omega + \omega + \nu$ | $\Omega = 0$, $\omega = 0$, $\nu = l$ |

$$
u = \begin{cases}
\arccos\left(\frac{\vec{n}\cdot\vec{r}}{n r}\right), & r_z \ge 0 \\
2\pi - \arccos\left(\frac{\vec{n}\cdot\vec{r}}{n r}\right), & r_z < 0
\end{cases}
\qquad
\varpi = \operatorname{atan2}(e_y, e_x), \qquad l = \operatorname{atan2}(r_y, r_x)
$$

For retrograde equatorial orbits ($h_z < 0$, $i = 180^\circ$) the angles are measured against the flipped sense of rotation: $\varpi \leftarrow 2\pi - \varpi$ and $l \leftarrow 2\pi - l$ (mod $2\pi$). For circular orbits $E = \nu$, so the mean anomaly equals the reported $\nu$.

---

## 8. RF Doppler Shift & Communications

### 8.1 Line-of-Sight Range Rate in ECEF
For an observer at rest in the ECEF frame ($\vec{v}_{\text{obs, ECEF}} = \vec{0}$) observing a satellite with ECEF position $\vec{r}_{\text{sat, ECEF}}$ and ECEF velocity $\vec{v}_{\text{sat, ECEF}}$:

$$
\vec{\rho} = \vec{r}_{\text{sat, ECEF}} - \vec{r}_{\text{obs, ECEF}}, \quad \rho = \|\vec{\rho}\|
$$

$$
\dot{\rho} = \frac{\vec{\rho} \cdot \vec{v}_{\text{sat, ECEF}}}{\rho}
$$

### 8.2 Electromagnetic Doppler Equation (Classical & Relativistic)
*(Implementation: the classical form is the default; the relativistic form is selected with the `relativistic=true` query parameter.)*
For transmit frequency $f_0$ and speed of light $c$, the **classical first-order** shift is:

$$
\Delta f = -f_0 \left( \frac{\dot{\rho}}{c} \right)
$$

$$
f_{\text{received}} = f_0 + \Delta f
$$

- $\dot{\rho} < 0 \implies \Delta f > 0$ (Blue shift: satellite approaching, frequency increases).
- $\dot{\rho} > 0 \implies \Delta f < 0$ (Red shift: satellite receding, frequency decreases).

This is a classical approximation: the neglected second-order term is $\sim(\dot\rho/c)^2 \approx 6 \times 10^{-10}$ and the neglected transverse term is $\sim \beta^2/2 \approx 3 \times 10^{-10}$ for LEO. **Relativistic form** for high-fidelity RF work (X/Ku/Ka-band, where $\sim 10^{-10}$ is $\sim 1\text{--}10\text{ Hz}$), with $\beta_s = \|\vec{v}_{\text{sat, ECI}}\|/c$ and $\beta_o = \omega_{\oplus}\sqrt{x_{\text{obs}}^2 + y_{\text{obs}}^2}/c$ the inertial-frame speeds of transmitter and observer ($\dot\rho$ is the same in ECEF and ECI because the observer is fixed to the rotating Earth):

$$
f_{\text{received}} = f_0 \, \frac{\sqrt{1 - \beta_s^2}}{\sqrt{1 - \beta_o^2}\,(1 + \dot{\rho}/c)} \left[ 1 + \frac{\mu_{\oplus}}{c^2}\left( \frac{1}{r_{\text{obs}}} - \frac{1}{r_{\text{sat}}} \right) \right]
$$

where $\sqrt{1-\beta_s^2}/\sqrt{1-\beta_o^2}$ is the ratio of inverse Lorentz factors (transverse Doppler / time dilation of both ends) and the bracket is the gravitational blue shift for a transmitter at greater geocentric radius than the receiver ($\sim 4 \times 10^{-11}$ for LEO). Light-time retardation (evaluating $\dot\rho$ at transmit time) and ionospheric/tropospheric delay are not modeled.

---

## 9. Atmospheric Drag, Decay Risk & Catalog Lifetime Assessment

### 9.1 SGP4 Ballistic Drag Parameter ($B^*$) Physics
In the SGP4 propagation theory, atmospheric drag is parameterized through $B^*$:

$$
B^* = \frac{1}{2} \frac{\rho_0 C_D A}{m} R_E
$$

where $\rho_0 = 0.1570 \times 10^{-6}\text{ kg/m}^2/\text{Earth radii}$, $C_D$ is the aerodynamic drag coefficient, $A$ is cross-sectional area, and $m$ is satellite mass.

### 9.2 Secular Orbit Decay & Energy Loss Rate
**Semi-major axis.** The TLE mean motion is a Kozai mean motion (periodic $J_2$ terms removed), so Kepler's third law $a = (\mu_{\oplus}/n^2)^{1/3}$ must **not** be applied to it. The mean semi-major axis $a''_0$ is recovered with the SGP4 initialization procedure of §11.2.

**$B^*$ must not be used with a generic density model.** $B^*$ is an empirical fit parameter tied to the fixed pseudo-density profile embedded in SGP4 (§9.1). It is not a physical ballistic coefficient; inserting it into $\rho(h)$ from another atmosphere (NRLMSISE-00, Harris-Priester, JB2008) produces incorrect decay rates. $B^*$ is used only inside SGP4 propagation.

**Physical decay (outside SGP4).** With a physical ballistic coefficient $\beta_{\text{BC}} = \dfrac{m}{C_D A}$ (kg/m$^2$) and an external density model $\rho(h)$, the near-circular secular rate is:

$$
\frac{da}{dt} = -\frac{\rho(h)}{\beta_{\text{BC}}} \sqrt{\mu_{\oplus} a}, \qquad \Delta a_{\text{rev}} = -2\pi a^2 \frac{\rho(h)}{\beta_{\text{BC}}}
$$

(consistent units; optionally scaled by the atmospheric co-rotation factor $(1 - \omega_{\oplus} a \cos i / v)^2$). $m$, $C_D$, $A$ are not contained in a TLE; when they are unavailable, a decay estimate must come from propagating with SGP4 itself, or be reported as unavailable. Synthetic ballistic coefficients are not permitted (see `AGENTS.md`).

### 9.3 King-Hele Analytical Lifetime Approximations
For near-circular low Earth orbits ($e < 0.02$), with $\dot{a}$ from §9.2:

$$
L \approx \frac{H}{|\dot{a}|}
$$

where $H$ is the atmospheric density scale height ($H \sim 5\text{--}8\text{ km}$ at 150–300 km). The TLE-derived form $L \approx H n / (2\dot{n} a)$ is withdrawn: $\dot{n}$ is a fit artifact (§11.1) and the correct relation from $n^2 a^3 = \mu_\oplus$ is $\dot{a} = -\tfrac{2}{3} a \dot{n}/n$.

### 9.4 NASA/NORAD Re-entry Risk Regimes
The decay risk score $S_{\text{risk}} \in [0, 100]$ and the regime are evaluated from perigee altitude (no $B^*$ dependence). The decay-watch scan (`/decay-watch`) selects objects by perigee altitude $\le$ `max_perigee_km` only (default 300 km); there is no $B^*$ filter. Scores below 100 km perigee are clamped to 100. The API reports no point lifetime estimate (`orbitalLifetimeDaysEstimate` is omitted) because a TLE alone contains no physical ballistic coefficient (§9.2); the lifetime column is the regime's nominal range $h_p = a(1 - e) - R_E$:

| Perigee Altitude ($h_p$) | Status / Regime | Estimated Lifetime | Operational Risk Score ($S$) |
|---|---|---|---|
| $h_p < 150\text{ km}$ | **Critical Re-entry Imminent** | $< 48\text{ hours}$ | $100 - (h_p - 100) \cdot 0.1$ |
| $150 \le h_p < 200\text{ km}$ | **High Risk / Imminent Re-entry** | $< 2\text{ weeks}$ | $85 + (200 - h_p) \cdot 0.3$ |
| $200 \le h_p < 300\text{ km}$ | **Moderate Risk / Active Decay** | $2\text{ weeks -- }6\text{ months}$ | $50 + (300 - h_p) \cdot 0.35$ |
| $300 \le h_p < 500\text{ km}$ | **Low Risk / Long-Term LEO Decay** | $6\text{ months -- }10\text{ years}$ | $10 + (500 - h_p) \cdot 0.2$ |
| $h_p \ge 500\text{ km}$ | **Stable Orbit / Negligible Drag** | $> 25\text{ years}$ | $\max(0, (600 - h_p) \cdot 0.05)$ |

---

## 10. Conjunction Assessment & Foster Collision Probability ($P_c$)

### 10.1 Conjunction Screening & Time of Closest Approach (TCA)
Pairwise conjunction screening evaluates separation across duration $[0, T]$:

$$
d(t) = \|\vec{r}_2(t) - \vec{r}_1(t)\|
$$

Local minima are refined to 1-second accuracy. At TCA, the line-of-sight velocity is zero:

$$
\dot{d}(t_{\text{TCA}}) = 0 \iff (\vec{r}_2 - \vec{r}_1) \cdot (\vec{v}_2 - \vec{v}_1) = 0
$$

### 10.2 Encounter Plane (B-Plane) Coordinates
Let $\vec{v}_{\text{rel}} = \vec{v}_2 - \vec{v}_1$ and $\vec{\rho} = \vec{r}_2 - \vec{r}_1$ at TCA.
The encounter plane unit normal is parallel to relative velocity:

$$
\hat{y}_e = \frac{\vec{v}_{\text{rel}}}{\|\vec{v}_{\text{rel}}\|}
$$

The orthogonal 2D encounter plane $\Pi$ is spanned by $\hat{x}_e = \vec{\rho}/\|\vec{\rho}\|$ (any perpendicular if $d = 0$) and $\hat{z}_e = \hat{y}_e \times \hat{x}_e$. At TCA $\vec{\rho} \perp \vec{v}_{\text{rel}}$, so the entire miss vector lies within this plane:

$$
\|\vec{\rho}\| = d, \qquad \vec{d}_{2D} = [d, 0]^T
$$

### 10.3 Combined Positional Covariance Projection
Let $\mathbf{C}_1, \mathbf{C}_2 \in \mathbb{R}^{3\times 3}$ be the positional covariance matrices of the two objects, expressed in the same frame at TCA.
Combined covariance:

$$
\mathbf{C}_{\text{rel}} = \mathbf{C}_1 + \mathbf{C}_2
$$

Projected onto the 2D encounter plane with $\mathbf{P} = [\hat{x}_e \; \hat{z}_e] \in \mathbb{R}^{3\times 2}$:

$$
\mathbf{C}_{2D} = \mathbf{P}^T \mathbf{C}_{\text{rel}} \mathbf{P} = \begin{bmatrix}
\sigma_x^2 & \rho_{xz}\sigma_x\sigma_z \\
\rho_{xz}\sigma_x\sigma_z & \sigma_z^2
\end{bmatrix}
$$

### 10.4 Hard-Body Radius (HBR) Sphere Assumption
Modeling both space objects as spheres of radii $R_1, R_2$, a collision occurs if the relative trajectory passes through a circular disk of combined radius $R_{\text{hbr}}$:

$$
R_{\text{hbr}} = R_1 + R_2
$$

### 10.5 Foster 2D Integral & Akella-Alfriend Closed Form
The exact collision probability is the integral of the 2D Gaussian density over the hard-body disk of radius $R_{\text{hbr}}$, with Gaussian mean at the miss vector:

$$
P_c = \frac{1}{2\pi \sqrt{\det \mathbf{C}_{2D}}} \iint_{x^2 + z^2 \le R_{\text{hbr}}^2} \exp\left( -\frac{1}{2} \begin{bmatrix} x - d_x \\ z - d_z \end{bmatrix}^T \mathbf{C}_{2D}^{-1} \begin{bmatrix} x - d_x \\ z - d_z \end{bmatrix} \right) dx dz
$$

> [!WARNING]
> **Covariance circularization is prohibited.** Orbital position covariances are strongly anisotropic (in-track $\gg$ radial, cross-track). Replacing $\mathbf{C}_{2D}$ with an isotropic $\sigma = \sqrt{(\sigma_x^2+\sigma_z^2)/2}$ smears probability density over the wrong geometry and yields large false positives and false negatives.

**Principal-axis form.** Eigen-decompose $\mathbf{C}_{2D} = \mathbf{V}\,\operatorname{diag}(\sigma_1^2, \sigma_2^2)\,\mathbf{V}^T$ and rotate the miss vector, $\vec{m} = \mathbf{V}^T \vec{d}_{2D} = (m_1, m_2)$ (the circular disk is rotation-invariant):

$$
P_c = \frac{1}{2\pi\sigma_1\sigma_2} \iint_{u_1^2 + u_2^2 \le R_{\text{hbr}}^2} \exp\left[ -\frac{1}{2}\left( \frac{(u_1 - m_1)^2}{\sigma_1^2} + \frac{(u_2 - m_2)^2}{\sigma_2^2} \right) \right] du_1\, du_2
$$

**Numerical evaluation (reference method).** Reduce to one dimension with the error function and integrate by Gauss-Legendre (or adaptive Simpson) quadrature:

$$
P_c = \int_{-R_{\text{hbr}}}^{R_{\text{hbr}}} \frac{e^{-(u_1 - m_1)^2 / 2\sigma_1^2}}{\sigma_1\sqrt{2\pi}} \cdot \frac{1}{2}\left[ \operatorname{erf}\left(\frac{w(u_1) - m_2}{\sigma_2\sqrt{2}}\right) - \operatorname{erf}\left(\frac{-w(u_1) - m_2}{\sigma_2\sqrt{2}}\right) \right] du_1, \quad w(u_1) = \sqrt{R_{\text{hbr}}^2 - u_1^2}
$$

**Akella-Alfriend / constant-density closed form.** When $R_{\text{hbr}} \ll \sigma_{\min} = \min(\sigma_1, \sigma_2)$ the density is approximately constant over the disk, giving the anisotropic closed form (eigenvalue-based, never circularized):

$$
P_c \approx \frac{R_{\text{hbr}}^2}{2\sigma_1\sigma_2} \exp\left[ -\frac{1}{2}\left( \frac{m_1^2}{\sigma_1^2} + \frac{m_2^2}{\sigma_2^2} \right) \right]
$$

If $R_{\text{hbr}}$ is not small relative to $\sigma_{\min}$ (very thin covariance axis), the closed form is invalid and the quadrature above is used.

**Isotropic special case** ($\sigma_1 = \sigma_2 = \sigma$, only where the covariance is genuinely isotropic): the integral reduces to the Rician form, and with $I_0(r d/\sigma^2) \approx 1$ for $R_{\text{hbr}} \ll \sigma$:

$$
P_c \approx \exp\left( -\frac{d^2}{2\sigma^2} \right) \left[ 1 - \exp\left( -\frac{R_{\text{hbr}}^2}{2\sigma^2} \right) \right] \;\approx\; \frac{R_{\text{hbr}}^2}{2\sigma^2} \exp\left( -\frac{d^2}{2\sigma^2} \right)
$$

**Implementation.** `POST /v1/conjunctions/collision-probability` takes the principal-axis $1\sigma$ values `sigma1M`, `sigma2M` (required, $>0$) and optionally `missAngleDeg`, the angle of the miss vector from the $\sigma_1$ axis (default $0$), so $m_1 = d\cos\alpha$, $m_2 = d\sin\alpha$. $P_c$ is always computed with the erf quadrature (substitution $u_1 = R_{\text{hbr}}\sin t$, composite Simpson, 20 000 panels). Conjunction screening (§10.1) reports no $P_c$ because TLEs carry no covariance.

### 10.6 Probability Dilution Region & Operational Thresholds
- **Direct Hit ($d = 0$, isotropic only)**:
  $$
  P_c = 1 - \exp\left( -\frac{R_{\text{hbr}}^2}{2\sigma^2} \right)
  $$
  For anisotropic covariance with $d = 0$ no elementary closed form exists; use the quadrature of §10.5.
- **Dilution Phenomenon**: As tracking uncertainty becomes arbitrarily large (both $\sigma_i \to \infty$), $P_c \to 0$. NASA CARA conjunction monitoring flags low-$P_c$ events with high uncertainty as indeterminate rather than safe.
- **Risk Categorization (NASA CARA Guidelines)**:
  - **Critical ($P_c \ge 10^{-4}$)**: Collision Avoidance Maneuver (CAM) mandatory.
  - **Elevated ($10^{-5} \le P_c < 10^{-4}$)**: Heightened radar tasking and maneuver planning.
  - **Low ($10^{-7} \le P_c < 10^{-5}$)**: Routine tracking monitoring.
  - **Negligible ($P_c < 10^{-7}$)**: Nominal pass, no action required.

---

## 11. Maneuver Reconstruction & Statistical Anomaly Detection

### 11.1 Mean Motion Derivative Fields & Baseline Drift Model
The TLE Line 1 fields $\dot{n}/2$ (`mean_motion_dot`) and $\ddot{n}/6$ (`mean_motion_ddot`) are **not** physical kinematic derivatives. In the USSPACECOM catalog they are catch-all fit parameters from differential correction that absorb unmodeled drag, solar radiation pressure and observation noise, and SGP4 itself ignores them (drag enters only through $B^*$). Extrapolating them with a polynomial in $\Delta t$ diverges and produces false maneuver flags, so **no $\dot{n}$/$\ddot{n}$ polynomial prediction is used**.

The expected quiescent drift between consecutive TLEs is instead estimated from the object's own recent history. For consecutive mean semi-major axes $a''_k$ (§11.2) at epochs $t_k$ (days):

$$
\dot{a}_k = \frac{a''_k - a''_{k-1}}{t_k - t_{k-1}}, \qquad \dot{a}_{\text{base}} = \operatorname{median}\left( \dot{a}_{k-W}, \dots, \dot{a}_{k-1} \right)
$$

over a trailing window of $W = 5$ samples ($\dot{a}_{\text{base}} = 0$ if fewer than 3 samples). The median is robust to an isolated maneuver, so flagged windows are not excluded explicitly; a steady decay shorter than 4 epochs into the history is therefore not yet absorbed into the baseline. The predicted value is $a_{\text{pred}} = a''_{k-1} + \dot{a}_{\text{base}} (t_k - t_{k-1})$.

### 11.2 Semi-Major Axis Residuals & Maneuver Classification
**Mean semi-major axis from Kozai mean motion.** The TLE mean motion $n_o$ (rad/min) is a Kozai mean motion with the periodic $J_2$ perturbation removed. Pure Kepler ($a = (\mu_\oplus/n^2)^{1/3}$) is wrong by $\approx 0\text{--}10\text{ km}$ for LEO (it vanishes near $i \approx 54.7^\circ$ where $3\cos^2 i = 1$ and is largest near $0^\circ$/$90^\circ$) (and the error is inclination- and eccentricity-dependent, so it does not cancel between TLEs). The mean semi-major axis $a''_0$ is recovered with the SGP4 initialization (Spacetrack Report #3), using the **WGS-72** constants of §1 ($k_2 = \tfrac{1}{2}J_2$, lengths in Earth radii), with inclination $i_o$ and eccentricity $e_o$ from the same TLE:

$$
a_1 = \left(\frac{k_e}{n_o}\right)^{2/3}, \qquad \delta_1 = \frac{3}{2}\frac{k_2}{a_1^2}\frac{3\cos^2 i_o - 1}{(1 - e_o^2)^{3/2}}
$$

$$
a_0 = a_1\left(1 - \tfrac{1}{3}\delta_1 - \delta_1^2 - \tfrac{134}{81}\delta_1^3\right), \qquad \delta_0 = \frac{3}{2}\frac{k_2}{a_0^2}\frac{3\cos^2 i_o - 1}{(1 - e_o^2)^{3/2}}
$$

$$
n''_0 = \frac{n_o}{1 + \delta_0}, \qquad a''_0 = \frac{a_0}{1 - \delta_0}, \qquad a''(n) \equiv a''_0 \cdot R_{E,\text{WGS-72}}\ (\text{km})
$$

The residual and inclination change are then:

$$
\Delta a_{\text{residual}} = a''(n_{\text{obs}}) - a_{\text{pred}}
$$

$$
\Delta i = i_{\text{obs}} - i_{\text{prev}}
$$

**Maneuver Classification**:
- $|\Delta a| \ge \Delta a_{\text{min}}$ and $|\Delta i| \ge \Delta i_{\text{min}} \implies$ **Combined**
- $|\Delta a| \ge \Delta a_{\text{min}}$ and $\Delta a > 0 \implies$ **SemiMajorAxisIncrease** (Orbit Raise)
- $|\Delta a| \ge \Delta a_{\text{min}}$ and $\Delta a < 0 \implies$ **SemiMajorAxisDecrease** (Orbit Lower)
- $|\Delta i| \ge \Delta i_{\text{min}} \implies$ **InclinationChange** (Plane Change)

### 11.3 Non-Parametric Outlier Scoring via Median Absolute Deviation (MAD)
To detect unannounced orbital anomalies without parametric Gaussian distribution assumptions:

$$
\tilde{X} = \operatorname{median}(X)
$$

$$
\text{MAD} = \operatorname{median}\left( |x_i - \tilde{X}| \right)
$$

The robust $z$-score is scaled by the normal consistency constant $k = 1.4826 \approx \frac{1}{\Phi^{-1}(0.75)}$:

$$
z_i = \frac{|x_i - \tilde{X}|}{1.4826 \cdot \text{MAD}}
$$

An anomaly is flagged when both the robust $z$-score exceeds the statistical sigma threshold ($z_i \ge z_{\text{threshold}}$) and the absolute orbital change exceeds physical detection limits ($|\Delta a| \ge \Delta a_{\text{min}}$ or $|\Delta i| \ge \Delta i_{\text{min}}$).

## 12. Element Representations & Coordinate Frame Transforms (`/v1/satellites/transforms/*`)

Implemented in `cartesian_to_keplerian`, `keplerian_to_cartesian`, `keplerian_to_equinoctial`, `equinoctial_to_keplerian`, `solve_kepler_equation`, `transform_orbital_elements`, and `transform_coordinate_frame` in `src/services/astrodynamics.rs`. `GET /v1/satellites/{id}/state` uses the same `cartesian_to_keplerian` routine as Section 7. All elements are osculating, in the TEME/ECI frame, with $\mu_\oplus = 398600.4418\ \text{km}^3/\text{s}^2$.

### 12.1 Keplerian to Cartesian (Perifocal Rotation)
Semi-latus rectum and radius:

$$p = a(1 - e^2), \qquad r = \frac{p}{1 + e\cos\nu}$$

Perifocal (PQW) state:

$$\vec{r}_{PQW} = r\begin{bmatrix}\cos\nu \\ \sin\nu \\ 0\end{bmatrix}, \qquad \vec{v}_{PQW} = \sqrt{\frac{\mu}{p}}\begin{bmatrix}-\sin\nu \\ e + \cos\nu \\ 0\end{bmatrix}$$

The PQW to ECI rotation is $\mathbf{R}_z(-\Omega)\,\mathbf{R}_x(-i)\,\mathbf{R}_z(-\omega)$. Its first two columns (perifocal axes $\hat{P}$, $\hat{Q}$ in ECI) are:

$$\hat{P} = \begin{bmatrix}\cos\Omega\cos\omega - \sin\Omega\sin\omega\cos i \\ \sin\Omega\cos\omega + \cos\Omega\sin\omega\cos i \\ \sin\omega\sin i\end{bmatrix}, \qquad \hat{Q} = \begin{bmatrix}-\cos\Omega\sin\omega - \sin\Omega\cos\omega\cos i \\ -\sin\Omega\sin\omega + \cos\Omega\cos\omega\cos i \\ \cos\omega\sin i\end{bmatrix}$$

$$\vec{r}_{ECI} = r_{P}\hat{P} + r_{Q}\hat{Q}, \qquad \vec{v}_{ECI} = v_{P}\hat{P} + v_{Q}\hat{Q}$$

Inputs are validated: $a > 0$ and $0 \le e < 1$ (bound elliptic orbits only). Hyperbolic and parabolic cases return HTTP 400.

### 12.2 Cartesian to Keplerian (Singular Cases)
`cartesian_to_keplerian` is the same routine as Section 7 (`osculating_elements`), including the non-singular angle folding of §7.6: relative equatorial test $n/h < 10^{-8}$, circular test $e < 10^{-6}$, and the retrograde flip of $\varpi$ and $l$. The state endpoint and the transform endpoints therefore report identical angles for the same state. Perigee and apogee altitudes use the WGS-84 radius at the apsis latitude (§7.5), not the equatorial $R_E$.

Angles in the degenerate cases are conventions, not physical quantities. Only the combined longitude is well defined, which is why Section 12.3 exists. A Keplerian request with $e < 10^{-6}$ or $i \approx 0^\circ$ therefore returns folded angles rather than echoing the input $\omega$, $\Omega$.

**Bound orbits only.** A Cartesian state with $a \le 0$ or $e \ge 1$ (specific energy $\ge 0$) returns HTTP 400.

### 12.3 Modified Equinoctial Elements (MEE)
Singularity-free for $e \to 0$ and $i \to 0$ (Walker, Ireland & Owens 1985; Broucke & Cefola 1972):

$$p = a(1 - e^2)$$

$$f = e\cos(\omega + I\Omega), \qquad g = e\sin(\omega + I\Omega)$$

$$h = \tan^{I}\!\left(\frac{i}{2}\right)\cos\Omega, \qquad k = \tan^{I}\!\left(\frac{i}{2}\right)\sin\Omega$$

$$L = \Omega\cdot I + \omega + \nu \pmod{2\pi}$$

The retrograde factor is $I = +1$ for $0 \le i < 90^\circ$ and $I = -1$ for $90^\circ < i \le 180^\circ$ (the response reports it as `retrogradeFactor`). For $I = -1$, $\tan^{-1}(i/2) = \cot(i/2)$, and both $\Omega$ terms flip sign: $L = -\Omega + \omega + \nu$.

The mean longitude is $\lambda = I\Omega + \omega + M$ (same sign convention as $L$).

**Inverse** (`equinoctial_to_keplerian`):

$$e = \sqrt{f^2 + g^2}, \qquad a = \frac{p}{1 - e^2}$$

$$\tan\frac{i}{2} = \sqrt{h^2 + k^2}\ \ (I = +1), \qquad \cot\frac{i}{2} = \sqrt{h^2 + k^2}\ \ (I = -1)$$

$$\Omega = \text{atan2}(k, h), \qquad \varpi = \text{atan2}(g, f), \qquad \omega = \varpi - I\Omega, \qquad \nu = L - \varpi$$

where $\varpi$ is the longitude of periapsis ($\omega + I\Omega$). Inputs with $e \ge 1$ return HTTP 400.

Note: when $h = k = 0$ (exactly equatorial, $I = +1$), $\Omega$ from $\text{atan2}(0, 0)$ is defined as $0$. Only $\varpi$ and $L$ carry physical meaning there, which is exactly what MEE preserves.

### 12.4 Kepler's Equation (`solve_kepler_equation`)
Given mean anomaly $M$ and eccentricity $e$, solve for the eccentric anomaly $E$:

$$M = E - e\sin E$$

Newton-Raphson on $f(E) = E - e\sin E - M$ with $f'(E) = 1 - e\cos E$. Initial guess is $E_0 = M$ for $e < 0.8$ and $E_0 = \pi$ otherwise. Iteration stops at $|\Delta E| < 10^{-13}$ or 60 iterations. True anomaly:

$$\cos\nu = \frac{\cos E - e}{1 - e\cos E}, \qquad \sin\nu = \frac{\sqrt{1 - e^2}\sin E}{1 - e\cos E}, \qquad \nu = \text{atan2}(\sin\nu, \cos\nu)$$

### 12.5 ECEF to Topocentric (SEZ and NED)
The ECEF to SEZ rotation, observer position, and look angles are Section 2.5. Frame-transform endpoints add the following.

**Inverse (SEZ to ECEF)** uses the transpose (a rotation matrix is orthogonal), then adds the observer position:

$$\begin{bmatrix}r_x \\ r_y \\ r_z\end{bmatrix} = \begin{bmatrix}\sin\phi\cos\lambda & -\sin\lambda & \cos\phi\cos\lambda \\ \sin\phi\sin\lambda & \cos\lambda & \cos\phi\sin\lambda \\ -\cos\phi & 0 & \sin\phi\end{bmatrix}\begin{bmatrix}\rho_S \\ \rho_E \\ \rho_Z\end{bmatrix}, \qquad \vec{r}_{ECEF} = \vec{r}_{obs} + \vec{\rho}_{ECEF}$$

**NED**: North-East-Down relates to SEZ by sign flips on the North and Zenith axes:

$$\rho_N = -\rho_S, \qquad \rho_E = \rho_E, \qquad \rho_D = -\rho_Z$$

**Velocity** in a topocentric frame is the ECEF velocity rotated with the same direction cosine matrix. It is a relative velocity w.r.t. the rotating Earth (the observer is fixed in ECEF), so no additional transport term is needed. Range rate:

$$\dot{\rho} = \frac{\vec{\rho}_{SEZ}\cdot\vec{v}_{SEZ}}{|\vec{\rho}_{SEZ}|}$$

### 12.6 Frame Chain and Velocity Inversion
`transform_coordinate_frame` always routes through ECEF: source $\to$ ECEF $\to$ target. ECI to ECEF (position and velocity) is Section 2.3. The inverse (ECEF to ECI) transposes the rotation and reverses the transport term:

$$\vec{r}_{ECI} = \mathbf{R}_z(\theta)^T\vec{r}_{ECEF}, \qquad \vec{v}_{ECI} = \mathbf{R}_z(\theta)^T\vec{v}_{ECEF} + \vec{\omega}_\oplus \times \vec{r}_{ECI}$$

with $\vec{\omega}_\oplus \times \vec{r}_{ECI} = [-\omega_\oplus y,\ \omega_\oplus x,\ 0]^T$ and $\theta$ = GMST (Section 2.2). The response also returns WGS-84 geodetic coordinates via the Bowring method (Section 2.4).

### 12.7 Numerical Tolerances and Limitations
- **Round trips:** Keplerian to Cartesian to Keplerian reproduces $a$ to $10^{-5}$ km and $e$ to $10^{-9}$. Cartesian states round-trip to $10^{-5}$ km and $10^{-8}$ km/s (verified in `tests/transforms_tests.rs`).
- **Independent oracle:** Vallado, *Fundamentals of Astrodynamics and Applications*, Example 2-5 (RV to COE) is reproduced within the textbook's printed precision.
- **Frame accuracy:** the ECI/ECEF rotation uses GMST from $d_{\text{UT1}}$ only, with $DUT1 = 0$ (§2.1) and no polar motion or nutation. The $DUT1$ omission alone gives up to $\approx 0.4\text{ km}$ of position error at the equator (§2.1 error budget), and polar motion adds up to $\approx 10\text{ m}$. Do not treat this as an ICRF or high-precision transform.
- **Bound orbits only:** $a > 0$, $0 \le e < 1$.
- **Required anomaly:** Keplerian input needs `trueAnomalyDeg` or `meanAnomalyDeg`; equinoctial input needs `trueLongitudeDeg` or `meanLongitudeDeg`. A missing angle returns HTTP 400 rather than defaulting to $0$.
- **Validation:** `tests/transforms_reference_tests.rs` checks the element transforms (Cartesian, Keplerian and equinoctial inputs, prograde and retrograde, equatorial, polar, GEO, Molniya) against an independent equinoctial implementation built from the angular-momentum and eccentricity vectors, Kepler's equation against bisection and Vallado Example 2-1, and the frame chain against ERFA (GMST 1982, WGS-84, finite-differenced velocity). Tolerances: elements $10^{-9}$, position 1 m, velocity 1 mm/s. The values are regenerated with `tests/reference/gen_transform_reference.py`.


---

## 13. CZML Trajectory Output (`format=czml`)

Presentation encoding of the ECEF states from Section 2.3; no new physics. Implemented in `src/services/ephemeris.rs` (sampling) and `src/services/czml/` (packets).

### 13.1 Encoding
- **Frame:** every `position` sets `referenceFrame: "FIXED"` explicitly. Samples are the Section 2.3 ECEF vectors converted to metres: $[t_i,\ 1000\,x_i,\ 1000\,y_i,\ 1000\,z_i]$, with $t_i$ in seconds from the property's `epoch` (the first sample).
- **Interpolation:** every `position` sets `interpolationAlgorithm: "LAGRANGE"`, `interpolationDegree: 5`. Cesium's default is `LINEAR`, degree 1.
- **Clock:** the `document` packet carries a `clock` spanning the sampled interval.
- **Passes:** `/passes?format=czml` re-propagates each pass over $[t_{AOS}, t_{LOS}]$ at 30 s and appends the exact LOS instant, giving one entity per pass.

### 13.2 Why not the default
Interpolation error against 1 s samples of the same ephemeris (interior samples, max position error; measured once, throwaway test, one real LEO TLE (e = 0.058) and one synthetic Molniya-like TLE, 2 h window):

| Step | LEO linear | LEO Lagrange-5 | Molniya linear | Molniya Lagrange-5 |
|------|-----------|----------------|----------------|--------------------|
| 30 s | 0.85 km | 0.022 m | 0.35 km | 0.064 m |
| 60 s | 3.4 km | 0.023 m | 1.3 km | 0.064 m |
| 120 s | 13.7 km | 0.41 m | 4.6 km | 0.14 m |
| 300 s | 85 km | 93 m | 19.7 km | 10 m |

Hence the CZML step is capped at 120 s. `tests/czml_tests.rs` keeps a regression check (ISS TLE, 30/60/120 s, error < 1 m) using an independent textbook Lagrange implementation. This bounds *interpolation* error only; it says nothing about frame accuracy.

### 13.3 Limits
- **Frame accuracy is unchanged from Section 12.7:** GMST-only rotation, $DUT1 = 0$, no polar motion or nutation (up to $\approx 0.4$ km + $\approx 10$ m). Cesium's own FIXED-frame orientation model is a separate, unquantified difference (open: log R2/Q3).
- **Caps (reject, never clamp):** span $\le 24$ h, $\le 2000$ samples per track, step $\le 120$ s for CZML.
- **Dropped samples:** instants where SGP4 fails or returns non-finite values are omitted and counted in `droppedSamples` (ground track). A gap shorter than the interpolation window degrades the local fit; the response does not mark where.
- **Time scale:** timestamps are UTC as supplied; leap-second handling by Cesium is unverified.
