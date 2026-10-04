# 🛰️ Astrea SDA API — Astrodynamics & Mathematical Specification

This document provides the formal mathematical foundations, coordinate frame transformations, astrodynamical models, and numerical algorithms implemented across the Astrea Space Domain Awareness (SDA) platform.

> [!IMPORTANT]
> **Agent Operating Constraint**:  
> Per `AGENTS.md`, this specification is the single source of mathematical truth for Astrea SDA API. Any addition, modification, or refactoring of astrodynamic calculations in `src/services/` **must** be documented here with corresponding mathematical proofs, coordinate frame definitions, and numerical tolerances.

---

## Table of Contents

1. [Fundamental Physical Constants & Standards](#1-fundamental-physical-constants--standards)
2. [Time Systems & Geodetic Coordinate Transformations](#2-time-systems--geodetic-coordinate-transformations)
   - [2.1 Julian Date & Epoch Calculations](#21-julian-date--epoch-calculations)
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
8. [RF Doppler Shift & Communications](#8-rf-doppler-shift--communications)
   - [8.1 Line-of-Sight Range Rate in ECEF](#81-line-of-sight-range-rate-in-ecef)
   - [8.2 First-Order Electromagnetic Doppler Equation](#82-first-order-electromagnetic-doppler-equation)
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
    - [11.1 Mean Motion Extrapolation Polynomial](#111-mean-motion-extrapolation-polynomial)
    - [11.2 Semi-Major Axis Residuals & Maneuver Classification](#112-semi-major-axis-residuals--maneuver-classification)
    - [11.3 Non-Parametric Outlier Scoring via Median Absolute Deviation (MAD)](#113-non-parametric-outlier-scoring-via-median-absolute-deviation-mad)

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

---

## 2. Time Systems & Geodetic Coordinate Transformations

### 2.1 Julian Date & Epoch Calculations
Given a UTC timestamp $t$ represented as milliseconds since the Unix epoch (1970-01-01T00:00:00Z):
$$JD(t) = \frac{t_{\text{ms}}}{86\,400\,000} + 2\,440\,587.5$$

The time elapsed in Julian centuries from the standard epoch **J2000.0** (2000-01-01 12:00:00 TT, $JD = 2\,451\,545.0$):
$$T_{\text{UT1}} \approx \frac{JD - 2\,451\,545.0}{36\,525}$$
Days elapsed since J2000.0:
$$d = JD - 2\,451\,545.0$$

### 2.2 Greenwich Mean Sidereal Time (GMST) & Local Sidereal Time (LST)
Greenwich Mean Sidereal Time in degrees is computed via the IAU formula:
$$GMST(d) = \left( 280.46061837^\circ + 360.98564736629^\circ \cdot d \right) \pmod{360^\circ}$$
Guaranteed strictly positive in $[0^\circ, 360^\circ)$.

For an observer at East geodetic longitude $\lambda_{\text{deg}}$:
$$\theta_{\text{LST}} = \left( GMST(d) + \lambda_{\text{deg}} \right) \pmod{360^\circ}$$
In radians: $\theta_{\text{LST, rad}} = \theta_{\text{LST}} \cdot \frac{\pi}{180^\circ}$.

### 2.3 ECI to ECEF Position & Velocity (Earth Rotation Kinematics)
The transformation from Earth-Centered Inertial (ECI / TEME) to Earth-Centered Earth-Fixed (ECEF / ECF) is a rotation around the $Z$-axis by the Greenwich Mean Sidereal Time $\theta = \theta_{\text{GMST}}$:
$$\mathbf{R}_z(\theta) = \begin{bmatrix} \cos\theta & \sin\theta & 0 \\ -\sin\theta & \cos\theta & 0 \\ 0 & 0 & 1 \end{bmatrix}$$

**Position Transformation**:
$$\vec{r}_{\text{ECEF}} = \mathbf{R}_z(\theta) \vec{r}_{\text{ECI}} = \begin{bmatrix} x_{\text{ECI}}\cos\theta + y_{\text{ECI}}\sin\theta \\ -x_{\text{ECI}}\sin\theta + y_{\text{ECI}}\cos\theta \\ z_{\text{ECI}} \end{bmatrix}$$

**Velocity Transformation (Kinematic Transport Theorem)**:
Because ECEF rotates with constant angular velocity $\vec{\omega}_{\oplus} = [0, 0, \omega_{\oplus}]^T$:
$$\left(\frac{d\vec{r}}{dt}\right)_{\text{ECI}} = \left(\frac{d\vec{r}}{dt}\right)_{\text{ECEF}} + \vec{\omega}_{\oplus} \times \vec{r}_{\text{ECI}}$$
$$\vec{v}_{\text{ECEF}} = \mathbf{R}_z(\theta) \left( \vec{v}_{\text{ECI}} - \vec{\omega}_{\oplus} \times \vec{r}_{\text{ECI}} \right)$$

Evaluating the cross product $\vec{\omega}_{\oplus} \times \vec{r}_{\text{ECI}} = [-\omega_{\oplus} y, \omega_{\oplus} x, 0]^T$:
$$\vec{v}_{\text{eff}} = \vec{v}_{\text{ECI}} - \vec{\omega}_{\oplus} \times \vec{r}_{\text{ECI}} = \begin{bmatrix} v_x + \omega_{\oplus} y \\ v_y - \omega_{\oplus} x \\ v_z \end{bmatrix}$$
$$\vec{v}_{\text{ECEF}} = \mathbf{R}_z(\theta) \vec{v}_{\text{eff}} = \begin{bmatrix} v_{\text{eff}, x}\cos\theta + v_{\text{eff}, y}\sin\theta \\ -v_{\text{eff}, x}\sin\theta + v_{\text{eff}, y}\cos\theta \\ v_z \end{bmatrix}$$

### 2.4 ECEF to WGS-84 Geodetic Coordinates (Bowring's Closed-Form Algorithm)
Given Cartesian coordinates $[x, y, z]^T$ in ECEF (km), Bowring's 1976 closed-form vector method avoids transcendental iterations and provides sub-millimeter precision for $|h| < 10\,000\text{ km}$:

1. Distance from Earth rotation axis:
   $$p = \sqrt{x^2 + y^2}$$
2. Geodetic longitude:
   $$\lambda = \text{atan2}(y, x)$$
3. If $p < 10^{-6}\text{ km}$ (polar singularity):
   $$\phi = \begin{cases} +90^\circ, & z \ge 0 \\ -90^\circ, & z < 0 \end{cases}, \quad h = |z| - b$$
4. Parametric (reduced) latitude $\theta$:
   $$\theta = \text{atan2}(z \cdot R_E, p \cdot b)$$
5. Geodetic latitude $\phi$:
   $$\phi = \text{atan2}\left( z + e'^2 b \sin^3\theta, \; p - e^2 R_E \cos^3\theta \right)$$
6. Prime vertical radius of curvature $N(\phi)$:
   $$N(\phi) = \frac{R_E}{\sqrt{1 - e^2 \sin^2\phi}}$$
7. Ellipsoidal altitude $h$:
   $$h = \frac{p}{\cos\phi} - N(\phi)$$

### 2.5 Topocentric Horizon Look Angles (SEZ Coordinates)
The observer's ECEF coordinates at geodetic latitude $\phi$, longitude $\lambda$, and ellipsoidal height $h_{\text{km}}$:
$$C = \frac{1}{\sqrt{1 - e^2 \sin^2\phi}}, \quad S = C(1-f)^2$$
$$\vec{r}_{\text{obs}} = \begin{bmatrix} (R_E C + h)\cos\phi\cos\lambda \\ (R_E C + h)\cos\phi\sin\lambda \\ (R_E S + h)\sin\phi \end{bmatrix}_{\text{ECEF}}$$

The slant range vector in ECEF is:
$$\vec{\rho}_{\text{ECEF}} = \vec{r}_{\text{sat, ECEF}} - \vec{r}_{\text{obs, ECEF}} = \begin{bmatrix} r_x \\ r_y \\ r_z \end{bmatrix}$$

Transforming into the **Topocentric Horizon (SEZ: South, East, Zenith)** frame:
$$\begin{bmatrix} \rho_S \\ \rho_E \\ \rho_Z \end{bmatrix} = \begin{bmatrix} \sin\phi\cos\lambda & \sin\phi\sin\lambda & -\cos\phi \\ -\sin\lambda & \cos\lambda & 0 \\ \cos\phi\cos\lambda & \cos\phi\sin\lambda & \sin\phi \end{bmatrix} \begin{bmatrix} r_x \\ r_y \\ r_z \end{bmatrix}$$

Scalar slant range:
$$\rho = \|\vec{\rho}\| = \sqrt{r_x^2 + r_y^2 + r_z^2}$$

Topocentric elevation ($El$) and geographic azimuth ($Az$):
$$El = \arcsin\left(\frac{\rho_Z}{\rho}\right)$$
$$Az = \text{atan2}(\rho_E, -\rho_S) \pmod{360^\circ}$$
*(Because $-\rho_S = \rho_N$, $\text{atan2}(\rho_E, \rho_N)$ represents True North-referenced clockwise azimuth: $0^\circ = \text{North}, 90^\circ = \text{East}, 180^\circ = \text{South}, 270^\circ = \text{West}$).*

---

## 3. Ground Track, Footprints & Spatial Geometry

### 3.1 Satellite Footprint Radius & Geodesic Boundary Polygons
The instantaneous sub-satellite footprint corresponds to the circular horizon visibility cap on the spherical Earth from altitude $h_{\text{km}}$:
$$\cos\sigma = \frac{R_E}{R_E + \max(h, 0)}$$
$$\sigma = \arccos\left(\frac{R_E}{R_E + \max(h, 0)}\right) \quad (\text{Central Earth Angular Radius in radians})$$
$$r_{\text{footprint}} = R_E \cdot \sigma \quad (\text{Surface distance in km})$$

The 36 boundary vertices ($k = 0, \dots, 36$) around azimuth $\alpha_k = k \cdot 10^\circ$ from the sub-satellite point $(\phi_0, \lambda_0)$:
$$\sin\phi_k = \sin\phi_0 \cos\sigma + \cos\phi_0 \sin\sigma \cos\alpha_k$$
$$\Delta\lambda_k = \text{atan2}\left(\sin\alpha_k \sin\sigma \cos\phi_0, \; \cos\sigma - \sin\phi_0 \sin\phi_k\right)$$
$$\lambda_k = \left(\lambda_0 + \Delta\lambda_k\right) \pmod{360^\circ}$$
Normalized to $[-180^\circ, +180^\circ]$.

### 3.2 Anti-Meridian Handling & Topology
A spherical cap is topologically simple on a planar map projection if:
1. It does not enclose a geographic pole:
   $$|\phi_0| + \sigma_{\text{deg}} < 90^\circ$$
2. Its maximum longitude half-width does not cross $\pm 180^\circ$:
   $$\Delta\lambda_{\text{half}} = \arcsin\left(\min\left(1.0, \frac{\sin\sigma}{\cos\phi_0}\right)\right)$$
   $$\lambda_0 - \Delta\lambda_{\text{half}} > -180^\circ \quad \text{and} \quad \lambda_0 + \Delta\lambda_{\text{half}} < 180^\circ$$

When continuous trajectory LineStrings traverse the $\pm 180^\circ$ meridian ($|\lambda_{k} - \lambda_{k-1}| > 180^\circ$), the geometry is dynamically segmented into a GeoJSON `MultiLineString` to prevent rendering wraparound streaks.

---

## 4. Celestial Ephemerides & Illumination Geometry

### 4.1 Solar Ephemeris (ECI)
The Sun's position vector in ECI is computed via the low-precision analytical ephemeris (Astronomical Almanac / USNO):
1. Mean anomaly of the Sun:
   $$M_{\odot} = \left( 357.529^\circ + 0.98560028^\circ \cdot d \right) \pmod{360^\circ}$$
2. Mean longitude of the Sun:
   $$q = \left( 280.459^\circ + 0.98564736^\circ \cdot d \right) \pmod{360^\circ}$$
3. Ecliptic longitude:
   $$\lambda_{\odot} = q + 1.915^\circ \sin M_{\odot} + 0.020^\circ \sin(2 M_{\odot})$$
4. Distance from Earth (km):
   $$R_{\odot} = (1.00014 - 0.01671 \cos M_{\odot} - 0.00014 \cos(2 M_{\odot})) \times 149\,597\,870.7$$
5. Obliquity of the ecliptic:
   $$\epsilon = 23.439^\circ - 0.00000036^\circ \cdot d$$
6. ECI Position:
   $$\vec{r}_{\odot} = \begin{bmatrix} R_{\odot} \cos\lambda_{\odot} \\ R_{\odot} \cos\epsilon \sin\lambda_{\odot} \\ R_{\odot} \sin\epsilon \sin\lambda_{\odot} \end{bmatrix}$$

### 4.2 Lunar Ephemeris (ECI)
The Moon's position vector in ECI is determined via Meeus analytical lunar expansions:
1. Fundamental arguments:
   $$L' = (218.316^\circ + 13.176396^\circ \cdot d) \pmod{360^\circ} \quad (\text{Mean Longitude})$$
   $$M' = (134.963^\circ + 13.064993^\circ \cdot d) \pmod{360^\circ} \quad (\text{Mean Anomaly})$$
   $$F = (93.272^\circ + 13.229350^\circ \cdot d) \pmod{360^\circ} \quad (\text{Argument of Latitude})$$
2. Geocentric distance and ecliptic coordinates:
   $$\lambda_{☾} = L' + 6.289^\circ \sin M'$$
   $$\beta_{☾} = 5.128^\circ \sin F$$
   $$R_{☾} = 385\,001.0 - 20\,905.0 \cos M' \quad (\text{km})$$
3. ECI transformation:
   $$\vec{r}_{☾} = \begin{bmatrix} R_{☾} \cos\beta_{☾} \cos\lambda_{☾} \\ R_{☾} (\cos\beta_{☾} \sin\lambda_{☾} \cos\epsilon - \sin\beta_{☾} \sin\epsilon) \\ R_{☾} (\cos\beta_{☾} \sin\lambda_{☾} \sin\epsilon + \sin\beta_{☾} \cos\epsilon) \end{bmatrix}$$

### 4.3 Dual-Cone Solar Shadow Geometry (Umbra & Penumbra)
To determine whether a satellite is in Earth's shadow without spherical cylinder simplifications:
1. Relative Sun vector:
   $$\vec{d} = \vec{r}_{\odot} - \vec{r}_{\text{sat}}, \quad d = \|\vec{d}\|$$
2. Apparent angular radii of Earth and Sun subtended at the satellite:
   $$\theta_E = \arcsin\left(\frac{R_E}{\|\vec{r}_{\text{sat}}\|}\right)$$
   $$\theta_{\odot} = \arcsin\left(\frac{R_{\odot}}{d}\right)$$
3. Angle between Earth center ($-\vec{r}_{\text{sat}}$) and Sun ($\vec{d}$):
   $$\cos\theta = \frac{-\vec{r}_{\text{sat}} \cdot \vec{d}}{\|\vec{r}_{\text{sat}}\| \cdot d}, \quad \theta = \arccos(\text{clamp}(\cos\theta, -1.0, 1.0))$$
4. **Lighting State Classification**:
   $$\text{State} = \begin{cases} \mathbf{Umbra} \quad (\text{Total eclipse}), & \theta < \theta_E - \theta_{\odot} \\ \mathbf{Penumbra} \quad (\text{Partial eclipse}), & |\theta_E - \theta_{\odot}| \le \theta < \theta_E + \theta_{\odot} \\ \mathbf{FullSunlight}, & \theta \ge \theta_E + \theta_{\odot} \end{cases}$$

### 4.4 Ground Observer Twilight States & Optical Visibility
Observer sun elevation $El_{\odot}$ determines ambient optical background sky conditions:
$$\text{Twilight State} = \begin{cases} \mathbf{Daylight}, & El_{\odot} > 0^\circ \\ \mathbf{CivilTwilight}, & 0^\circ \ge El_{\odot} > -6^\circ \\ \mathbf{NauticalTwilight}, & -6^\circ \ge El_{\odot} > -12^\circ \\ \mathbf{AstronomicalTwilight}, & -12^\circ \ge El_{\odot} > -18^\circ \\ \mathbf{Night}, & El_{\odot} \le -18^\circ \end{cases}$$

A satellite is optically observable if:
$$\text{is\_visibly\_observable} \iff (\text{LightingState} \ne \mathbf{Umbra}) \land (El_{\odot} \le -6.0^\circ) \land (El_{\text{sat}} > 0.0^\circ)$$

### 4.5 Solar & Lunar Satellite Transits
For an observer looking along unit vector $\hat{u}_{\text{target}}$ towards the center of the Sun or Moon, and along unit vector $\hat{u}_{\text{sat}}$ towards the satellite:
$$\hat{u} = \begin{bmatrix} \cos(El)\sin(Az) \\ \cos(El)\cos(Az) \\ \sin(El) \end{bmatrix}$$
$$\Delta\theta = \arccos\left(\text{clamp}(\hat{u}_{\text{sat}} \cdot \hat{u}_{\text{target}}, -1.0, 1.0)\right)$$

A transit event occurs when $\Delta\theta \le \theta_{\text{threshold}}$ while both bodies have positive elevation above the local horizon.

---

## 5. Ground Station Pass Scheduling & Numerical Methods

### 5.1 Horizon Threshold Root Finding (Bisection Method)
For a target elevation threshold $El_{\text{th}}$:
$$g(t) = El(t) - El_{\text{th}} = 0$$
When coarse 60-second scanning brackets a crossing $[t_{\text{below}}, t_{\text{above}}]$:
$$t_{\text{mid}} = t_{\text{below}} + \frac{t_{\text{above}} - t_{\text{below}}}{2}$$
The bisection loop iterates until convergence:
$$|t_{\text{above}} - t_{\text{below}}| \le 1000\text{ ms}$$
Yielding sub-second resolution for Acquisition of Signal ($t_{\text{AOS}}$) and Loss of Signal ($t_{\text{LOS}}$).

### 5.2 Peak Elevation Refinement (Golden-Section Search)
Between AOS and LOS, single-pass elevation is strictly unimodal. To refine the peak without numerical derivatives, the Golden-Section Search operates on interval $[a, b]$ with ratio $\tau$:
$$\tau = \frac{\sqrt{5}-1}{2} \approx 0.61803398875$$
$$m_1 = b - \tau(b - a), \quad m_2 = a + \tau(b - a)$$
Since $\tau > 0.5$, $m_1 < m_2$.
- If $El(m_1) > El(m_2)$, then $b \leftarrow m_2$.
- Else, $a \leftarrow m_1$.

Convergence terminates when $(b - a) \le 1000\text{ ms}$, isolating the Time of Closest Approach ($t_{\text{TCA}}$) and maximum elevation ($El_{\text{max}}$).

### 5.3 Optical Visual Magnitude Approximation
When a satellite is visibly observable at slant range $\rho_{\text{km}}$:
$$m_v \approx M_0 + 5 \log_{10}\left( \max\left( \frac{\rho}{1000\text{ km}}, 0.1 \right) \right)$$
where $M_0 = 2.5$ represents the baseline standard absolute visual magnitude for a nominal 1000 km low Earth orbit bus.

---

## 6. Relative Motion & Rendezvous Proximity Operations (RPO)

### 6.1 Hill / Local-Vertical Local-Horizontal (LVLH) Basis Vectors
Centered on the primary (chief) satellite at position $\vec{r}_p$ and velocity $\vec{v}_p$:

1. **Radial Unit Vector**:
   $$\hat{e}_R = \frac{\vec{r}_p}{\|\vec{r}_p\|}$$
2. **Cross-Track Unit Vector (Orbital Momentum Normal)**:
   $$\vec{h}_p = \vec{r}_p \times \vec{v}_p, \quad \hat{e}_C = \frac{\vec{h}_p}{\|\vec{h}_p\|}$$
3. **In-Track Unit Vector**:
   $$\hat{e}_I = \hat{e}_C \times \hat{e}_R$$

Right-handedness verification:
$$\hat{e}_R \times \hat{e}_I = \hat{e}_R \times (\hat{e}_C \times \hat{e}_R) = (\hat{e}_R \cdot \hat{e}_R)\hat{e}_C - (\hat{e}_R \cdot \hat{e}_C)\hat{e}_R = \hat{e}_C$$

Relative position of target $\vec{r}_t$:
$$\delta\vec{r} = \vec{r}_t - \vec{r}_p$$
$$x_R = \delta\vec{r} \cdot \hat{e}_R, \quad y_I = \delta\vec{r} \cdot \hat{e}_I, \quad z_C = \delta\vec{r} \cdot \hat{e}_C$$
$$\rho = \|\delta\vec{r}\| = \sqrt{x_R^2 + y_I^2 + z_C^2}$$

### 6.2 Transport Theorem & Apparent Rotating Velocity
The orbital angular velocity vector of the primary orbit is:
$$\vec{\omega}_{\text{orb}} = \frac{\vec{h}_p}{\|\vec{r}_p\|^2} = \dot{\nu} \hat{e}_C$$

By the kinematic transport theorem:
$$\left(\frac{d(\delta\vec{r})}{dt}\right)_{\text{inertial}} = \left(\frac{d(\delta\vec{r})}{dt}\right)_{\text{rotating}} + \vec{\omega}_{\text{orb}} \times \delta\vec{r}$$

Given inertial relative velocity $\delta\vec{v} = \vec{v}_t - \vec{v}_p$, the apparent relative velocity vector in the rotating Hill frame is:
$$\vec{v}_{\text{rel, rot}} = \delta\vec{v} - \vec{\omega}_{\text{orb}} \times \delta\vec{r}$$
$$v_R = \vec{v}_{\text{rel, rot}} \cdot \hat{e}_R, \quad v_I = \vec{v}_{\text{rel, rot}} \cdot \hat{e}_I, \quad v_C = \vec{v}_{\text{rel, rot}} \cdot \hat{e}_C$$

### 6.3 Relative Range Rate Invariance
$$\dot{\rho} = \frac{\delta\vec{r} \cdot \delta\vec{v}}{\rho}$$
Because $\delta\vec{r} \cdot (\vec{\omega}_{\text{orb}} \times \delta\vec{r}) \equiv 0$, the range rate evaluates identically in both inertial and rotating frames:
$$\frac{\delta\vec{r} \cdot \vec{v}_{\text{rel, rot}}}{\rho} = \frac{\delta\vec{r} \cdot \delta\vec{v}}{\rho} = \dot{\rho}$$

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
$$r = \|\vec{r}\|, \quad v = \|\vec{v}\|$$

### 7.1 Specific Mechanical Energy & Semi-Major Axis
$$\mathcal{E} = \frac{v^2}{2} - \frac{\mu_{\oplus}}{r}$$
$$a = -\frac{\mu_{\oplus}}{2\mathcal{E}} = \frac{1}{\frac{2}{r} - \frac{v^2}{\mu_{\oplus}}}$$

### 7.2 Angular Momentum & Laplace-Runge-Lenz Eccentricity Vector
$$\vec{h} = \vec{r} \times \vec{v}, \quad h = \|\vec{h}\|$$
$$\vec{e} = \frac{1}{\mu_{\oplus}} \left[ \left(v^2 - \frac{\mu_{\oplus}}{r}\right)\vec{r} - (\vec{r}\cdot\vec{v})\vec{v} \right], \quad e = \|\vec{e}\|$$

### 7.3 Orbital Plane Orientation ($i, \Omega, \omega$)
1. **Inclination**:
   $$i = \arccos\left(\text{clamp}\left(\frac{h_z}{h}, -1.0, 1.0\right)\right) \in [0, \pi]$$
2. **Line of Nodes**:
   $$\vec{n} = \hat{k} \times \vec{h} = [-h_y, h_x, 0]^T, \quad n = \|\vec{n}\| = \sqrt{h_x^2 + h_y^2}$$
3. **Right Ascension of Ascending Node ($\Omega$)**:
   $$\Omega = \begin{cases} \arccos\left(\frac{n_x}{n}\right), & n_y \ge 0 \\ 360^\circ - \arccos\left(\frac{n_x}{n}\right), & n_y < 0 \end{cases} \quad (\Omega = 0^\circ \text{ if } n < 10^{-8})$$
4. **Argument of Perigee ($\omega$)**:
   $$\omega = \begin{cases} \arccos\left(\frac{\vec{n}\cdot\vec{e}}{n e}\right), & e_z \ge 0 \\ 360^\circ - \arccos\left(\frac{\vec{n}\cdot\vec{e}}{n e}\right), & e_z < 0 \end{cases} \quad (\omega = 0^\circ \text{ if } n < 10^{-8} \lor e < 10^{-6})$$

### 7.4 Anomaly Conversions (True $\nu$, Eccentric $E$, Mean $M$)
1. **True Anomaly**:
   $$\nu = \begin{cases} \arccos\left(\frac{\vec{e}\cdot\vec{r}}{e r}\right), & \vec{r}\cdot\vec{v} \ge 0 \\ 360^\circ - \arccos\left(\frac{\vec{e}\cdot\vec{r}}{e r}\right), & \vec{r}\cdot\vec{v} < 0 \end{cases} \quad (\nu = 0^\circ \text{ if } e < 10^{-6})$$
2. **Eccentric Anomaly via Half-Angle / Quadrant-Safe Trigonometry**:
   $$\cos E = \frac{e + \cos\nu}{1 + e\cos\nu}, \quad \sin E = \frac{\sqrt{\max(0, 1 - e^2)}\sin\nu}{1 + e\cos\nu}$$
   $$E = \text{atan2}(\sin E, \cos E)$$
3. **Mean Anomaly (Kepler's Equation)**:
   $$M = \left( E - e \sin E \right) \pmod{2\pi}$$

### 7.5 Orbital Period, Perigee & Apogee Radii
$$T_{\text{period}} = \frac{2\pi \sqrt{a^3 / \mu_{\oplus}}}{60} \quad (\text{minutes})$$
$$h_p = a(1 - e) - R_E \quad (\text{Perigee Altitude in km})$$
$$h_a = a(1 + e) - R_E \quad (\text{Apogee Altitude in km})$$

---

## 8. RF Doppler Shift & Communications

### 8.1 Line-of-Sight Range Rate in ECEF
For an observer at rest in the ECEF frame ($\vec{v}_{\text{obs, ECEF}} = \vec{0}$) observing a satellite with ECEF position $\vec{r}_{\text{sat, ECEF}}$ and ECEF velocity $\vec{v}_{\text{sat, ECEF}}$:
$$\vec{\rho} = \vec{r}_{\text{sat, ECEF}} - \vec{r}_{\text{obs, ECEF}}, \quad \rho = \|\vec{\rho}\|$$
$$\dot{\rho} = \frac{\vec{\rho} \cdot \vec{v}_{\text{sat, ECEF}}}{\rho}$$

### 8.2 First-Order Electromagnetic Doppler Equation
For transmit frequency $f_0$ and speed of light $c$:
$$\Delta f = -f_0 \left( \frac{\dot{\rho}}{c} \right)$$
$$f_{\text{received}} = f_0 + \Delta f$$
- $\dot{\rho} < 0 \implies \Delta f > 0$ (Blue shift: satellite approaching, frequency increases).
- $\dot{\rho} > 0 \implies \Delta f < 0$ (Red shift: satellite receding, frequency decreases).

---

## 9. Atmospheric Drag, Decay Risk & Catalog Lifetime Assessment

### 9.1 SGP4 Ballistic Drag Parameter ($B^*$) Physics
In the SGP4 propagation theory, atmospheric drag is parameterized through $B^*$:
$$B^* = \frac{1}{2} \frac{\rho_0 C_D A}{m} R_E$$
where $\rho_0 = 0.1570 \times 10^{-6}\text{ kg/m}^2/\text{Earth radii}$, $C_D$ is the aerodynamic drag coefficient, $A$ is cross-sectional area, and $m$ is satellite mass.

### 9.2 Secular Orbit Decay & Energy Loss Rate
Mean motion $n$ in rad/s:
$$n = \text{mean\_motion} \times \frac{2\pi}{86\,400}$$
Semi-major axis from Kepler's third law:
$$a = \left( \frac{\mu_{\oplus}}{n^2} \right)^{1/3}$$

The secular rate of semi-major axis loss over one complete revolution:
$$\frac{da}{dt} = -2 B^* R_E \left(\frac{\rho(h)}{\rho_0}\right) \sqrt{\mu_{\oplus} a}$$

### 9.3 King-Hele Analytical Lifetime Approximations
For near-circular low Earth orbits ($e < 0.02$):
$$L \approx -\frac{H}{\dot{a}} \approx \frac{H n}{2 \dot{n} a}$$
where $H$ is the atmospheric density scale height ($H \sim 5\text{--}8\text{ km}$ at 150–300 km).

### 9.4 NASA/NORAD Re-entry Risk Regimes
The decay risk score $S_{\text{risk}} \in [0, 100]$ and estimated lifetime $L$ are evaluated from perigee altitude $h_p = a(1 - e) - R_E$:

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
$$d(t) = \|\vec{r}_2(t) - \vec{r}_1(t)\|$$
Local minima are refined to 1-second accuracy. At TCA, the line-of-sight velocity is zero:
$$\dot{d}(t_{\text{TCA}}) = 0 \iff (\vec{r}_2 - \vec{r}_1) \cdot (\vec{v}_2 - \vec{v}_1) = 0$$

### 10.2 Encounter Plane (B-Plane) Coordinates
Let $\vec{v}_{\text{rel}} = \vec{v}_2 - \vec{v}_1$ and $\vec{\rho} = \vec{r}_2 - \vec{r}_1$ at TCA.
The encounter plane unit normal is parallel to relative velocity:
$$\hat{y}_e = \frac{\vec{v}_{\text{rel}}}{\|\vec{v}_{\text{rel}}\|}$$
The orthogonal 2D encounter plane $\Pi$ is spanned by $\hat{x}_e, \hat{z}_e \perp \hat{y}_e$. The entire miss vector $\vec{\rho}$ lies within this plane:
$$\|\vec{\rho}\| = d$$

### 10.3 Combined Positional Covariance Projection
Let $\mathbf{C}_1, \mathbf{C}_2 \in \mathbb{R}^{3\times 3}$ be the positional covariance matrices of the two objects.
Combined covariance:
$$\mathbf{C}_{\text{rel}} = \mathbf{C}_1 + \mathbf{C}_2$$
Projected onto the 2D encounter plane:
$$\mathbf{C}_{2D} = \begin{bmatrix} \sigma_x^2 & \rho_{xz}\sigma_x\sigma_z \\ \rho_{xz}\sigma_x\sigma_z & \sigma_z^2 \end{bmatrix}$$

### 10.4 Hard-Body Radius (HBR) Sphere Assumption
Modeling both space objects as spheres of radii $R_1, R_2$, a collision occurs if the relative trajectory passes through a circular disk of combined radius $R_{\text{hbr}}$:
$$R_{\text{hbr}} = R_1 + R_2$$

### 10.5 Foster 2D Integral & Akella-Alfriend Closed Form
The exact collision probability is the integral of the 2D Gaussian density function over the circle of radius $R_{\text{hbr}}$ centered at miss distance $d$:
$$P_c = \frac{1}{2\pi \sqrt{\det \mathbf{C}_{2D}}} \iint_{x^2 + z^2 \le R_{\text{hbr}}^2} \exp\left( -\frac{1}{2} \begin{bmatrix} x - d_x \\ z - d_z \end{bmatrix}^T \mathbf{C}_{2D}^{-1} \begin{bmatrix} x - d_x \\ z - d_z \end{bmatrix} \right) dx dz$$

In the isotropic/circularized covariance projection with equivalent standard deviation $\sigma = \sqrt{\frac{\sigma_x^2 + \sigma_z^2}{2}}$:
$$P_c = \frac{1}{\sigma^2} e^{-\frac{d^2}{2\sigma^2}} \int_0^{R_{\text{hbr}}} r e^{-\frac{r^2}{2\sigma^2}} I_0\left(\frac{r d}{\sigma^2}\right) dr$$

Because $R_{\text{hbr}} \ll \sigma$ in orbital conjunctions ($R_{\text{hbr}} \sim 10\text{ m}$ vs. $\sigma \sim 50\text{--}500\text{ m}$), $I_0\left(\frac{r d}{\sigma^2}\right) \approx 1$. Integrating the remaining term yields the **Foster / Akella-Alfriend closed-form encounter plane formula**:
$$P_c \approx \exp\left( -\frac{d^2}{2\sigma^2} \right) \left[ 1 - \exp\left( -\frac{R_{\text{hbr}}^2}{2\sigma^2} \right) \right]$$

For $R_{\text{hbr}}^2 \ll 2\sigma^2$, the first-order Taylor expansion gives:
$$P_c \approx \frac{R_{\text{hbr}}^2}{2\sigma^2} \exp\left( -\frac{d^2}{2\sigma^2} \right)$$

### 10.6 Probability Dilution Region & Operational Thresholds
- **Direct Hit ($d = 0$)**:
  $$P_c = 1 - \exp\left( -\frac{R_{\text{hbr}}^2}{2\sigma^2} \right)$$
- **Dilution Phenomenon**: As tracking uncertainty becomes arbitrarily large ($\sigma \to \infty$), $P_c \to 0$. NASA CARA conjunction monitoring flags low-$P_c$ events with high uncertainty as indeterminate rather than safe.
- **Risk Categorization (NASA CARA Guidelines)**:
  - **Critical ($P_c \ge 10^{-4}$)**: Collision Avoidance Maneuver (CAM) mandatory.
  - **Elevated ($10^{-5} \le P_c < 10^{-4}$)**: Heightened radar tasking and maneuver planning.
  - **Low ($10^{-7} \le P_c < 10^{-5}$)**: Routine tracking monitoring.
  - **Negligible ($P_c < 10^{-7}$)**: Nominal pass, no action required.

---

## 11. Maneuver Reconstruction & Statistical Anomaly Detection

### 11.1 Mean Motion Extrapolation Polynomial
Between consecutive historical TLEs of an object spanning time $\Delta t$ (days):
$$n_{\text{pred}}(\Delta t) = n_0 + 2\left(\frac{\dot{n}}{2}\right)\Delta t + 3\left(\frac{\ddot{n}}{6}\right)\Delta t^2$$
where $\frac{\dot{n}}{2}$ (`mean_motion_dot`) and $\frac{\ddot{n}}{6}$ (`mean_motion_ddot`) are standard TLE Line 1 ballistic coefficients.

### 11.2 Semi-Major Axis Residuals & Maneuver Classification
From predicted mean motion $n_{\text{pred}}$ and observed mean motion $n_{\text{obs}}$:
$$a(n) = \left( \frac{\mu_{\oplus}}{(n \cdot 2\pi / 86400)^2} \right)^{1/3}$$
$$\Delta a_{\text{residual}} = a(n_{\text{obs}}) - a(n_{\text{pred}})$$
$$\Delta i = i_{\text{obs}} - i_{\text{prev}}$$

**Maneuver Classification**:
- $|\Delta a| \ge \Delta a_{\text{min}}$ and $|\Delta i| \ge \Delta i_{\text{min}} \implies$ **Combined**
- $|\Delta a| \ge \Delta a_{\text{min}}$ and $\Delta a > 0 \implies$ **SemiMajorAxisIncrease** (Orbit Raise)
- $|\Delta a| \ge \Delta a_{\text{min}}$ and $\Delta a < 0 \implies$ **SemiMajorAxisDecrease** (Orbit Lower)
- $|\Delta i| \ge \Delta i_{\text{min}} \implies$ **InclinationChange** (Plane Change)

### 11.3 Non-Parametric Outlier Scoring via Median Absolute Deviation (MAD)
To detect unannounced orbital anomalies without parametric Gaussian distribution assumptions:
$$\tilde{X} = \text{median}(X)$$
$$\text{MAD} = \text{median}\left( |x_i - \tilde{X}| \right)$$

The robust $z$-score is scaled by the normal consistency constant $k = 1.4826 \approx \frac{1}{\Phi^{-1}(0.75)}$:
$$z_i = \frac{|x_i - \tilde{X}|}{1.4826 \cdot \text{MAD}}$$

An anomaly is flagged when both the robust $z$-score exceeds the statistical sigma threshold ($z_i \ge z_{\text{threshold}}$) and the absolute orbital change exceeds physical detection limits ($|\Delta a| \ge \Delta a_{\text{min}}$ or $|\Delta i| \ge \Delta i_{\text{min}}$).
