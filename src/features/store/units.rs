//! Conversion between the Core Audio scalar (`0.0..=1.0`) and the integer
//! hundredths-of-a-percent used by the store.

pub const UNITS: u32 = 10_000; // 100.00 % in 0.01 % steps

pub fn units_from_scalar(level: f32) -> u32 {
    (level.clamp(0.0, 1.0) * UNITS as f32).round() as u32
}

pub fn scalar_from_units(level: u32) -> f32 {
    level.min(UNITS) as f32 / UNITS as f32
}

pub fn percent(level: u32) -> u32 {
    (level.min(UNITS) + 50) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_helpers_round_trip() {
        assert_eq!(units_from_scalar(0.0), 0);
        assert_eq!(units_from_scalar(1.0), UNITS);
        assert_eq!(units_from_scalar(0.42), 4200);
        assert_eq!(units_from_scalar(0.425), 4250);
        assert_eq!(units_from_scalar(-1.0), 0);
        assert_eq!(units_from_scalar(2.0), UNITS);
        assert_eq!(scalar_from_units(0), 0.0);
        assert_eq!(scalar_from_units(UNITS), 1.0);
        assert_eq!(scalar_from_units(999_999), 1.0);
        assert!((scalar_from_units(4200) - 0.42).abs() < f32::EPSILON);
    }

    #[test]
    fn percent_rounds_to_nearest() {
        assert_eq!(percent(0), 0);
        assert_eq!(percent(4250), 43);
        assert_eq!(percent(4249), 42);
        assert_eq!(percent(UNITS), 100);
        assert_eq!(percent(UNITS + 500), 100);
    }
}
