//! Pure timing and device-classification policy.
//!
//! Keeping these functions free of COM makes them easy to unit-test and leaves
//! the engine loop focused on orchestration.

use std::time::{Duration, Instant};

use windows::core::GUID;

/// How often the endpoint list is polled as a safety net for missed callbacks.
pub(crate) const SCAN_INTERVAL: Duration = Duration::from_secs(3);
pub(crate) const LOOP_TICK: Duration = Duration::from_millis(200);
/// Volume notifications during this window after connecting are considered to be
/// the headset's own AVRCP/absolute-volume sync, not user intent.
pub(crate) const CONNECT_GRACE: Duration = Duration::from_secs(3);
/// Delays between re-applying the saved volume after a connect.  Some headsets
/// send their own volume after Windows reports the endpoint as active.
pub(crate) const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(600),
    Duration::from_millis(1_400),
    Duration::from_millis(2_800),
];
pub(crate) const MAX_ATTEMPTS: u32 = 4;

/// Event context passed to volume notifications caused by this program.
pub(crate) const BT_VOLUME_CONTEXT: GUID =
    GUID::from_u128(0x5b17e4b2_1f8c_4a6d_9a3e_7c2d4f6b8a01);

/// True when either device property identifies a Bluetooth audio endpoint.
///
/// `DEVPKEY_Device_InstanceId` comes back empty on some headsets, so the
/// enumerator name has to be accepted as well.
pub(crate) fn is_bluetooth(instance_id: &str, enumerator_name: &str) -> bool {
    instance_id.to_ascii_uppercase().starts_with("BTH")
        || enumerator_name.to_ascii_uppercase().starts_with("BTH")
}

/// Delay before the next restore attempt, given how many were already applied.
///
/// Each delay is relative to the attempt that scheduled it, so the effective
/// timeline for four attempts is 0 s, 0.6 s, 2.0 s and 4.8 s.  `None` means the
/// attempt budget is spent.
pub(crate) fn next_retry_delay(attempts_done: u32) -> Option<Duration> {
    if attempts_done == 0 || attempts_done >= MAX_ATTEMPTS {
        return None;
    }
    RETRY_DELAYS.get(attempts_done as usize - 1).copied()
}

/// Whether a volume change happened after the post-connect grace window.
pub(crate) fn outside_grace(connected_at: Instant, changed_at: Instant) -> bool {
    changed_at.saturating_duration_since(connected_at) >= CONNECT_GRACE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_detected_from_instance_id() {
        assert!(is_bluetooth(
            "BTHENUM\\{0000110b-0000-1000-8000-00805f9b34fb}",
            ""
        ));
        assert!(is_bluetooth("bthenum\\device", ""));
    }

    #[test]
    fn bluetooth_detected_from_enumerator_when_instance_is_empty() {
        // The headset used for manual testing reports an empty instance id.
        assert!(is_bluetooth("", "BTHENUM"));
        assert!(is_bluetooth("", "bthhfenum"));
    }

    #[test]
    fn non_bluetooth_endpoints_are_ignored() {
        assert!(!is_bluetooth("", ""));
        assert!(!is_bluetooth("USB\\VID_1234", "USB"));
        assert!(!is_bluetooth("SWD\\MMDEVAPI\\{...}", "SWD"));
        assert!(!is_bluetooth("HDAUDIO\\FUNC_01", "HDAUDIO"));
        // "BTH" must be a prefix, not a substring.
        assert!(!is_bluetooth("USB\\BTHENUM", "USB"));
    }

    #[test]
    fn retry_delay_grows_until_the_budget_is_spent() {
        assert_eq!(next_retry_delay(0), None);
        assert_eq!(next_retry_delay(1), Some(Duration::from_millis(600)));
        assert_eq!(next_retry_delay(2), Some(Duration::from_millis(1_400)));
        assert_eq!(next_retry_delay(3), Some(Duration::from_millis(2_800)));
        assert_eq!(next_retry_delay(MAX_ATTEMPTS), None);
        assert_eq!(next_retry_delay(MAX_ATTEMPTS + 10), None);
    }

    #[test]
    fn restore_timeline_is_zero_then_accumulated_delays() {
        let mut at = Duration::ZERO;
        let mut times = vec![at];
        let mut attempts = 0;

        loop {
            attempts += 1;
            let Some(delay) = next_retry_delay(attempts) else {
                break;
            };
            at += delay;
            times.push(at);
        }

        assert_eq!(
            times,
            [
                Duration::ZERO,
                Duration::from_millis(600),
                Duration::from_millis(2_000),
                Duration::from_millis(4_800),
            ]
        );
        assert_eq!(times.len(), MAX_ATTEMPTS as usize);
    }

    #[test]
    fn grace_window_boundary() {
        let connected = Instant::now();
        assert!(!outside_grace(connected, connected));
        assert!(!outside_grace(
            connected,
            connected + Duration::from_millis(2_999)
        ));
        assert!(outside_grace(connected, connected + CONNECT_GRACE));
        assert!(outside_grace(
            connected,
            connected + Duration::from_secs(30)
        ));
    }

    #[test]
    fn grace_window_tolerates_clock_skew() {
        let connected = Instant::now();
        let earlier = connected.checked_sub(Duration::from_secs(1)).unwrap();
        assert!(!outside_grace(connected, earlier));
    }
}
