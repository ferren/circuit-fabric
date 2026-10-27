//! Display-time conversion of persisted UTC timestamps into the configured zone.
//!
//! Everything stored (session records, audit logs, CSV exports) stays RFC 3339
//! UTC; only user-facing rendering goes through these helpers. `System` follows
//! the operating system through chrono's `Local`, so daylight-saving transitions
//! are honored per timestamp rather than frozen at the current offset.

use chrono::{DateTime, FixedOffset, Local, NaiveDate, SecondsFormat, Utc};
use circuitfabric_codex_runtime::GlobalTimeZone;

fn utc(unix_seconds: u64) -> DateTime<Utc> {
    DateTime::from_timestamp(i64::try_from(unix_seconds).unwrap_or(i64::MAX), 0)
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}

fn in_zone(unix_seconds: u64, zone: GlobalTimeZone) -> DateTime<FixedOffset> {
    let utc = utc(unix_seconds);
    match zone {
        GlobalTimeZone::Utc => utc.with_timezone(&FixedOffset::east_opt(0).unwrap()),
        GlobalTimeZone::Offset(seconds) => utc.with_timezone(
            &FixedOffset::east_opt(seconds).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap()),
        ),
        GlobalTimeZone::System => {
            let local = utc.with_timezone(&Local);
            local.with_timezone(local.offset())
        }
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` for UTC, `YYYY-MM-DDTHH:MM:SS±HH:MM` otherwise.
#[must_use]
pub fn format_timestamp(unix_seconds: u64, zone: GlobalTimeZone) -> String {
    if zone == GlobalTimeZone::Utc {
        return circuitfabric_project::rfc3339(unix_seconds);
    }
    in_zone(unix_seconds, zone).to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Re-renders a persisted RFC 3339 timestamp (e.g. inside the session audit
/// Markdown) in the display zone. Unparseable input returns `None` so callers
/// can keep the original text.
#[must_use]
pub fn reformat_timestamp(value: &str, zone: GlobalTimeZone) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(value).ok()?;
    let seconds = u64::try_from(parsed.timestamp()).ok()?;
    Some(format_timestamp(seconds, zone))
}

fn local_date(unix_seconds: u64, zone: GlobalTimeZone) -> NaiveDate {
    in_zone(unix_seconds, zone).date_naive()
}

/// Days since 1970-01-01 of the timestamp's *local* calendar date, so daily
/// charts bucket and label by the display zone rather than UTC days.
#[must_use]
pub fn day_index(unix_seconds: u64, zone: GlobalTimeZone) -> u64 {
    let days = local_date(unix_seconds, zone)
        .signed_duration_since(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
        .num_days();
    u64::try_from(days).unwrap_or(0)
}

/// The calendar date a day bucket refers to; the bucket index is already
/// zone-adjusted, so the date itself needs no further conversion.
#[must_use]
pub fn day_index_date(day: u64) -> NaiveDate {
    // Display-only: clamp far-future bucket indices instead of overflowing the date.
    let days = i64::try_from(day.min(1_000_000)).unwrap_or(0);
    NaiveDate::from_ymd_opt(1970, 1, 1).unwrap() + chrono::Duration::days(days)
}

/// `YYYY-MM-DD` grouping key in the display zone.
#[must_use]
pub fn day_key(unix_seconds: u64, zone: GlobalTimeZone) -> String {
    local_date(unix_seconds, zone).format("%Y-%m-%d").to_string()
}

/// `UTC+08:00`-style label. `System` reports the operating system's *current*
/// offset, which may differ from the offset of a historical timestamp.
#[must_use]
pub fn zone_label(zone: GlobalTimeZone) -> String {
    let seconds = match zone {
        GlobalTimeZone::Utc => 0,
        GlobalTimeZone::Offset(seconds) => seconds,
        GlobalTimeZone::System => Local::now().offset().local_minus_utc(),
    };
    if seconds == 0 {
        return "UTC".to_owned();
    }
    let sign = if seconds < 0 { '-' } else { '+' };
    let magnitude = seconds.unsigned_abs();
    format!("UTC{sign}{:02}:{:02}", magnitude / 3600, magnitude % 3600 / 60)
}

/// The choices offered by the settings page: the OS zone, UTC, then the fixed
/// offsets in real use worldwide (including the half- and quarter-hour zones).
#[must_use]
pub fn settings_options() -> Vec<GlobalTimeZone> {
    let mut options = vec![GlobalTimeZone::System, GlobalTimeZone::Utc];
    for minutes in [
        -720, -660, -600, -570, -540, -480, -420, -360, -300, -240, -210, -180, -120, -60, 60,
        120, 180, 210, 240, 270, 300, 330, 345, 360, 390, 420, 480, 525, 540, 570, 600, 630, 660,
        720, 765, 780, 840,
    ] {
        options.push(GlobalTimeZone::Offset(minutes * 60));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_matches_the_persisted_format_exactly() {
        assert_eq!(format_timestamp(0, GlobalTimeZone::Utc), "1970-01-01T00:00:00Z");
        assert_eq!(
            format_timestamp(1_709_164_800, GlobalTimeZone::Utc),
            "2024-02-29T00:00:00Z"
        );
    }

    #[test]
    fn fixed_offset_shifts_clock_time_and_date() {
        let zone = GlobalTimeZone::Offset(8 * 3600);
        assert_eq!(format_timestamp(0, zone), "1970-01-01T08:00:00+08:00");
        // 2026-10-27T00:00:00Z.
        assert_eq!(
            format_timestamp(1_793_059_200, zone),
            "2026-10-27T08:00:00+08:00"
        );
        let west = GlobalTimeZone::Offset(-(5 * 3600 + 1800));
        assert_eq!(format_timestamp(0, west), "1969-12-31T18:30:00-05:30");
    }

    #[test]
    fn reformat_only_touches_parseable_timestamps() {
        let zone = GlobalTimeZone::Offset(8 * 3600);
        assert_eq!(
            reformat_timestamp("2026-10-27T00:30:00Z", zone).as_deref(),
            Some("2026-10-27T08:30:00+08:00")
        );
        assert_eq!(reformat_timestamp("正在思考", zone), None);
        assert_eq!(
            reformat_timestamp("2026-10-27T00:30:00Z", GlobalTimeZone::Utc).as_deref(),
            Some("2026-10-27T00:30:00Z")
        );
    }

    #[test]
    fn day_buckets_follow_the_zone_calendar() {
        // 2026-10-27T16:30:00Z is already the next morning at UTC+08:00.
        let seconds = 1_793_118_600;
        assert_eq!(day_key(seconds, GlobalTimeZone::Utc), "2026-10-27");
        assert_eq!(day_key(seconds, GlobalTimeZone::Offset(8 * 3600)), "2026-10-28");
        let utc_day = day_index(seconds, GlobalTimeZone::Utc);
        let east_day = day_index(seconds, GlobalTimeZone::Offset(8 * 3600));
        assert_eq!(east_day, utc_day + 1);
        assert_eq!(day_index_date(east_day).format("%Y-%m-%d").to_string(), "2026-10-28");
        assert_eq!(day_index_date(utc_day).format("%m-%d").to_string(), "10-27");
    }

    #[test]
    fn settings_options_cover_system_utc_and_quarter_hour_zones() {
        let options = settings_options();
        assert_eq!(options[0], GlobalTimeZone::System);
        assert_eq!(options[1], GlobalTimeZone::Utc);
        assert!(options.contains(&GlobalTimeZone::Offset(8 * 3600)));
        assert!(options.contains(&GlobalTimeZone::Offset(5 * 3600 + 45 * 60)));
        assert!(options.contains(&GlobalTimeZone::Offset(-3 * 3600 - 1800)));
        for zone in options {
            assert_eq!(GlobalTimeZone::from_preference(&zone.preference()), zone);
        }
        assert_eq!(GlobalTimeZone::from_preference("garbage"), GlobalTimeZone::System);
    }

    #[test]
    fn system_zone_formats_with_a_numeric_offset() {
        let formatted = format_timestamp(1_793_059_200, GlobalTimeZone::System);
        assert!(formatted.starts_with("2026-"), "{formatted}");
        assert!(!formatted.is_empty());
    }
}
