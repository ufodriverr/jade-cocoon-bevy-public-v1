
use std::time::{SystemTime, UNIX_EPOCH};

/// `2026-10-15 20:12` of this moment, by the machine's clock.
pub fn now() -> String {
    of(SystemTime::now())
}

/// `2026-10-15 20:12` of a moment, by the machine's clock with its offset of this
/// moment.
pub fn of(moment: SystemTime) -> String {
    let seconds = match moment.duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    };
    named(seconds + offset())
}

/// `2026-10-15 20:12` of a count of seconds since 1970-01-01.
fn named(seconds: i64) -> String {
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = crate::logging::civil(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// The count of days since 1970-01-01 of a date (Howard Hinnant's `days_from_civil`).
fn days(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The seconds the machine's clock is ahead of the world's.
#[cfg(windows)]
fn offset() -> i64 {
    /// Windows' `SYSTEMTIME`.
    #[repr(C)]
    #[derive(Default)]
    struct Moment {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut Moment);
        fn GetSystemTime(time: *mut Moment);
    }
    let (mut local, mut world) = (Moment::default(), Moment::default());
    // SAFETY: both calls only fill the struct they are handed, which lives here.
    unsafe {
        GetLocalTime(&mut local);
        GetSystemTime(&mut world);
    }
    let seconds = |time: &Moment| {
        let days = days(i64::from(time.year), time.month.into(), time.day.into());
        days * 86_400
            + i64::from(time.hour) * 3600
            + i64::from(time.minute) * 60
            + i64::from(time.second)
    };
    // The two are read one after the other: the offset is a whole quarter of an hour.
    let ahead = seconds(&local) - seconds(&world);
    (ahead as f64 / 900.0).round() as i64 * 900
}

#[cfg(not(windows))]
fn offset() -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moment_is_named_by_its_date_and_its_minute() {
        assert_eq!(named(1_791_986_602), "2026-10-14 14:03");
        assert_eq!(named(0), "1970-01-01 00:00");
        assert_eq!(named(-60), "1969-12-31 23:59");
    }

    #[test]
    fn days_and_dates_go_both_ways() {
        assert_eq!(days(1970, 1, 1), 0);
        assert_eq!(days(2000, 2, 29), 11_016);
        assert_eq!(crate::logging::civil(days(2026, 10, 15)), (2026, 10, 15));
        assert_eq!(offset() % 900, 0);
    }
}
