use astrea_sda_api::{
    models::{Satellite, Tle},
    services::astrodynamics,
};
use chrono::{TimeZone, Utc};
use uuid::Uuid;

fn sample_iss_satellite() -> Satellite {
    Satellite {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        name: "ISS (ZARYA)".to_string(),
        tle: Tle {
            line_one: "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994"
                .to_string(),
            line_two: "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
                .to_string(),
        },
        created_date: Utc::now(),
        last_modified_date: Utc::now(),
    }
}

#[test]
fn test_doppler_shift_calculation_approaching_and_receding() {
    let sat = sample_iss_satellite();
    let center_freq_hz = 437_500_000.0; // 437.5 MHz UHF
    let lat = 34.0522; // Los Angeles
    let lon = -118.2437;
    let alt_km = 0.0;
    let epoch = Utc.with_ymd_and_hms(2024, 3, 20, 12, 0, 0).unwrap();

    let res = astrodynamics::calculate_doppler_shift(
        &sat,
        center_freq_hz,
        lat,
        lon,
        alt_km,
        epoch,
        false,
    )
    .expect("Doppler calculation failed");

    assert_eq!(res.satellite_id, sat.id);
    assert_eq!(res.satellite_name, "ISS (ZARYA)");
    assert_eq!(res.center_freq_hz, center_freq_hz);

    // Corrected frequency must equal center frequency + doppler shift
    let expected_corrected = res.center_freq_hz + res.doppler_shift_hz;
    assert!(
        (res.corrected_freq_hz - expected_corrected).abs() < 1e-3,
        "Corrected frequency mismatch"
    );

    // Speed of light check: delta_f = -f0 * (range_rate / c)
    let c_kms = 299_792.458;
    let expected_doppler = -center_freq_hz * (res.range_rate_kms / c_kms);
    assert!(
        (res.doppler_shift_hz - expected_doppler).abs() < 1e-3,
        "Doppler shift physics mismatch"
    );

    // Check signal direction classification
    if res.range_rate_kms < -1e-5 {
        assert_eq!(res.signal_direction, "Approaching (Blue Shift)");
    } else if res.range_rate_kms > 1e-5 {
        assert_eq!(res.signal_direction, "Receding (Red Shift)");
    } else {
        assert_eq!(res.signal_direction, "Stationary / Zero Doppler");
    }
}

// relativistic=true goes through the ECI-speed path: it must stay within the spec 8.2 bound of
// the classical shift (difference ~ beta^2/2 + grav ~ 3e-10 relative) and differ from it.
#[test]
fn relativistic_doppler_end_to_end_is_close_to_classical_but_not_equal() {
    let sat = sample_iss_satellite();
    let f0 = 1.0e10;
    let t = Utc.with_ymd_and_hms(2024, 3, 20, 12, 0, 0).unwrap();
    let classic =
        astrodynamics::calculate_doppler_shift(&sat, f0, 34.05, -118.24, 0.0, t, false).unwrap();
    let rel =
        astrodynamics::calculate_doppler_shift(&sat, f0, 34.05, -118.24, 0.0, t, true).unwrap();
    assert_eq!(classic.range_rate_kms, rel.range_rate_kms);
    let diff = (rel.corrected_freq_hz - classic.corrected_freq_hz).abs();
    assert!(diff > 0.1, "relativistic correction missing: {diff} Hz");
    // second-order terms: (rr/c)^2 + beta_s^2/2 + gravity, all < 1e-9 for LEO
    assert!(diff < 1e-9 * f0 * 2.0, "{diff} Hz");
}
