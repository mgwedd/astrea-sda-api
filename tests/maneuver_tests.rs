use astrea_sda_api::{
    models::{AnomalyStatus, ManeuverType, Satellite, Tle},
    services::maneuver::{detect_anomalies, reconstruct_maneuvers},
};
use uuid::Uuid;

fn sat(name: &str, line1: &str, line2: &str) -> Satellite {
    Satellite {
        id: Uuid::new_v4(),
        name: name.to_string(),
        tle: Tle {
            line_one: line1.to_string(),
            line_two: line2.to_string(),
        },
        created_date: chrono::Utc::now(),
        last_modified_date: chrono::Utc::now(),
    }
}

#[test]
fn test_single_tle_returns_zero_maneuvers_and_insufficient_data() {
    let s = sat(
        "ATLAS CENTAUR 2",
        "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397",
    );
    let history = [s.tle.clone()];

    let res = reconstruct_maneuvers(&s, &history, None, None, 0.1, 0.005).unwrap();
    assert_eq!(res.total_maneuvers_detected, 0);

    let anom = detect_anomalies(&s, &history, 3.0, 0.1, 0.005).unwrap();
    assert_eq!(anom.status, AnomalyStatus::InsufficientData);
}

#[test]
fn test_maneuver_reconstruction_with_synthetic_tle_pair() {
    let s = sat(
        "ATLAS CENTAUR 2",
        "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397",
    );
    let tle2 = Tle {
        line_one: "00694U 63047A   21240.66170074  .00000250  00000-0  20987-4 0  9996".to_string(),
        line_two: "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.00000000898397".to_string(),
    };
    let history = [s.tle.clone(), tle2];

    let res = reconstruct_maneuvers(&s, &history, None, None, 0.1, 0.005).unwrap();
    assert_eq!(res.tle_epochs_analyzed, 2);
    assert!(res.total_maneuvers_detected >= 1);
    assert_eq!(
        res.maneuvers[0].maneuver_type,
        ManeuverType::SemiMajorAxisIncrease
    );
}

fn checksum(body: &str) -> String {
    let sum: u32 = body
        .chars()
        .map(|c| c.to_digit(10).unwrap_or(u32::from(c == '-')))
        .sum();
    format!("{body}{}", sum % 10)
}

fn tle_at(day: f64, mean_motion: f64) -> Tle {
    Tle {
        line_one: checksum(&format!(
            "1 00694U 63047A   {day:.8}  .00000250  00000-0  20987-4 0  999"
        )),
        line_two: checksum(&format!(
            "2 00694  30.3579   8.5616 0584817  14.9507 346.7615 {mean_motion:11.8}89839"
        )),
    }
}

// A steady drag decay (~0.3 km/day) is the quiescent baseline and must not be flagged once the
// trailing-median drift is known; only the later step (a ~3 km raise) is a maneuver.
#[test]
fn steady_decay_is_baseline_and_only_the_step_is_flagged() {
    let s = sat(
        "ATLAS CENTAUR 2",
        &tle_at(21239.5, 14.0).line_one,
        &tle_at(21239.5, 14.0).line_two,
    );
    let mut history = Vec::new();
    for k in 0..6 {
        history.push(tle_at(21239.5 + k as f64, 14.0 + 0.001 * k as f64));
    }
    // raise: mean motion drops by 0.01 rev/day relative to the trend
    history.push(tle_at(21245.5, 14.0 + 0.006 - 0.01));

    let res = reconstruct_maneuvers(&s, &history, None, None, 0.5, 0.005).unwrap();
    assert_eq!(res.total_maneuvers_detected, 1, "{:?}", res.maneuvers);
    assert_eq!(
        res.maneuvers[0].maneuver_type,
        ManeuverType::SemiMajorAxisIncrease
    );
}
