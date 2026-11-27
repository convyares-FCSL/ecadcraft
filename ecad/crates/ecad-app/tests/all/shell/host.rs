//! The default host, before the program installs its own: UTC times through chrono and the
//! engine's wall clock.

use ecad_app::shell::host::Host;

#[test]
fn the_default_host_formats_utc_time() {
    let h = Host::default();
    let ms = 1_700_000_000_000 + 3_723_000;
    let rows: &[(u64, bool, &str)] = &[(0, true, "00:00:00"), (3_723_000, true, "01:02:03"), (3_723_999, false, "01:02"), (ms, true, "23:15:23")];
    for (ms, seconds, want) in rows {
        assert_eq!((h.local_time)(*ms, *seconds), *want, "{ms} {seconds}");
    }
    assert_eq!((h.local_time)(u64::MAX, true), "");
    assert!((h.now_ms)() > 1_700_000_000_000);
}
