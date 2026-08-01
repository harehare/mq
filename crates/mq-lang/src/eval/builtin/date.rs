use super::Error;
use chrono::{DateTime, Datelike, Duration, Months, NaiveDateTime, Utc, Weekday};

/// Date/time units used by `date_add` and `date_diff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DateUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Weeks,
    Months,
    Years,
}

impl TryFrom<&str> for DateUnit {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "seconds" => Ok(Self::Seconds),
            "minutes" => Ok(Self::Minutes),
            "hours" => Ok(Self::Hours),
            "days" => Ok(Self::Days),
            "weeks" => Ok(Self::Weeks),
            "months" => Ok(Self::Months),
            "years" => Ok(Self::Years),
            _ => Err(Error::Runtime(format!(
                "unknown date unit {:?}, expected \"seconds\", \"minutes\", \"hours\", \"days\", \"weeks\", \"months\", or \"years\"",
                s
            ))),
        }
    }
}

impl DateUnit {
    /// Adds `amount` of this unit to `dt`. Returns `None` on overflow.
    pub fn apply_add(self, dt: DateTime<Utc>, amount: i64) -> Option<DateTime<Utc>> {
        match self {
            Self::Seconds => dt.checked_add_signed(Duration::seconds(amount)),
            Self::Minutes => dt.checked_add_signed(Duration::minutes(amount)),
            Self::Hours => dt.checked_add_signed(Duration::hours(amount)),
            Self::Days => dt.checked_add_signed(Duration::days(amount)),
            Self::Weeks => dt.checked_add_signed(Duration::weeks(amount)),
            Self::Months => {
                if amount >= 0 {
                    dt.checked_add_months(Months::new(amount as u32))
                } else {
                    dt.checked_sub_months(Months::new((-amount) as u32))
                }
            }
            Self::Years => {
                if amount >= 0 {
                    dt.checked_add_months(Months::new(amount as u32 * 12))
                } else {
                    dt.checked_sub_months(Months::new((-amount) as u32 * 12))
                }
            }
        }
    }

    /// Returns the signed difference in this unit. Errors for `Months` and `Years` as
    /// `chrono::Duration` does not represent variable-length calendar units.
    pub fn apply_diff(self, diff: Duration) -> Result<i64, Error> {
        match self {
            Self::Seconds => Ok(diff.num_seconds()),
            Self::Minutes => Ok(diff.num_minutes()),
            Self::Hours => Ok(diff.num_hours()),
            Self::Days => Ok(diff.num_days()),
            Self::Weeks => Ok(diff.num_weeks()),
            Self::Months | Self::Years => Err(Error::Runtime(format!(
                "date_diff does not support unit {:?}, expected \"seconds\", \"minutes\", \"hours\", \"days\", or \"weeks\"",
                match self {
                    Self::Months => "months",
                    _ => "years",
                }
            ))),
        }
    }
}

/// Public helper called from `builtin.rs` for `date_add`.
pub(super) fn add(dt: DateTime<Utc>, amount: i64, unit: &str) -> Result<DateTime<Utc>, Error> {
    let unit = DateUnit::try_from(unit)?;
    unit.apply_add(dt, amount)
        .ok_or_else(|| Error::Runtime("date_add: arithmetic overflow or invalid date".to_string()))
}

/// Public helper called from `builtin.rs` for `date_diff`.
pub(super) fn diff(diff: Duration, unit: &str) -> Result<i64, Error> {
    let unit = DateUnit::try_from(unit)?;
    unit.apply_diff(diff)
}

/// Like `DateUnit::try_from`, but also accepts singular unit words (e.g. "day"), since
/// relative-date phrases such as "1 day ago" use the singular form.
fn parse_relative_unit(word: &str) -> Option<DateUnit> {
    match word {
        "second" | "seconds" => Some(DateUnit::Seconds),
        "minute" | "minutes" => Some(DateUnit::Minutes),
        "hour" | "hours" => Some(DateUnit::Hours),
        "day" | "days" => Some(DateUnit::Days),
        "week" | "weeks" => Some(DateUnit::Weeks),
        "month" | "months" => Some(DateUnit::Months),
        "year" | "years" => Some(DateUnit::Years),
        _ => None,
    }
}

fn parse_weekday(word: &str) -> Option<Weekday> {
    match word {
        "sunday" => Some(Weekday::Sun),
        "monday" => Some(Weekday::Mon),
        "tuesday" => Some(Weekday::Tue),
        "wednesday" => Some(Weekday::Wed),
        "thursday" => Some(Weekday::Thu),
        "friday" => Some(Weekday::Fri),
        "saturday" => Some(Weekday::Sat),
        _ => None,
    }
}

/// Walks forward (or backward) from `base` a day at a time until `target` weekday is
/// reached, preserving `base`'s time-of-day. `base`'s own weekday never matches, so this
/// always lands on the next/previous occurrence, never today.
fn nearest_weekday(base: DateTime<Utc>, target: Weekday, forward: bool) -> DateTime<Utc> {
    let time = base.time();
    let mut date = base.date_naive();

    loop {
        date = if forward {
            date.succ_opt().expect("date arithmetic within a week cannot overflow")
        } else {
            date.pred_opt().expect("date arithmetic within a week cannot overflow")
        };
        if date.weekday() == target {
            return NaiveDateTime::new(date, time).and_utc();
        }
    }
}

/// Parses a natural-language relative date expression relative to `base` and returns the
/// resulting UTC datetime.
///
/// Supported forms: "now", "today", "yesterday", "tomorrow", "<n> <unit> ago",
/// "in <n> <unit>", "next <weekday>", "last <weekday>". Units accept singular or plural
/// ("day"/"days", ...); weekdays use their full English name ("monday", ...).
pub(super) fn parse_relative(s: &str, base: DateTime<Utc>) -> Result<DateTime<Utc>, Error> {
    let unrecognized = || Error::Runtime(format!("date_relative: unrecognized relative date expression {:?}", s));
    let lower = s.trim().to_lowercase();

    match lower.as_str() {
        "now" | "today" => return Ok(base),
        "yesterday" => return Ok(base - Duration::days(1)),
        "tomorrow" => return Ok(base + Duration::days(1)),
        _ => {}
    }

    match lower.split_whitespace().collect::<Vec<_>>().as_slice() {
        [amount, unit_word, "ago"] => {
            let amount: i64 = amount.parse().map_err(|_| unrecognized())?;
            let unit = parse_relative_unit(unit_word).ok_or_else(unrecognized)?;
            unit.apply_add(base, -amount).ok_or_else(unrecognized)
        }
        ["in", amount, unit_word] => {
            let amount: i64 = amount.parse().map_err(|_| unrecognized())?;
            let unit = parse_relative_unit(unit_word).ok_or_else(unrecognized)?;
            unit.apply_add(base, amount).ok_or_else(unrecognized)
        }
        ["next", weekday_word] => {
            let weekday = parse_weekday(weekday_word).ok_or_else(unrecognized)?;
            Ok(nearest_weekday(base, weekday, true))
        }
        ["last", weekday_word] => {
            let weekday = parse_weekday(weekday_word).ok_or_else(unrecognized)?;
            Ok(nearest_weekday(base, weekday, false))
        }
        _ => Err(unrecognized()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rstest::rstest;

    fn utc(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
    }

    // --- TryFrom<&str> ---

    #[rstest]
    #[case("seconds", DateUnit::Seconds)]
    #[case("minutes", DateUnit::Minutes)]
    #[case("hours", DateUnit::Hours)]
    #[case("days", DateUnit::Days)]
    #[case("weeks", DateUnit::Weeks)]
    #[case("months", DateUnit::Months)]
    #[case("years", DateUnit::Years)]
    fn test_try_from_valid(#[case] input: &str, #[case] expected: DateUnit) {
        assert_eq!(DateUnit::try_from(input).unwrap(), expected);
    }

    #[rstest]
    #[case("second")]
    #[case("Seconds")]
    #[case("DAYS")]
    #[case("")]
    #[case(" days")]
    #[case("nanoseconds")]
    fn test_try_from_invalid(#[case] input: &str) {
        assert!(DateUnit::try_from(input).is_err());
    }

    #[test]
    fn test_try_from_error_message() {
        let err = DateUnit::try_from("bad").unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("bad"), "error should mention the bad unit");
        assert!(msg.contains("seconds"), "error should list supported units");
    }

    // --- apply_add ---

    #[rstest]
    #[case(DateUnit::Seconds, utc(2024, 1, 1), 86400, utc(2024, 1, 2))]
    #[case(DateUnit::Minutes, utc(2024, 1, 1), 1440, utc(2024, 1, 2))]
    #[case(DateUnit::Hours, utc(2024, 1, 1), 24, utc(2024, 1, 2))]
    #[case(DateUnit::Days, utc(2024, 1, 1), 31, utc(2024, 2, 1))]
    #[case(DateUnit::Weeks, utc(2024, 1, 1), 1, utc(2024, 1, 8))]
    #[case(DateUnit::Months, utc(2024, 1, 31), 1, utc(2024, 2, 29))] // 2024 is leap year
    #[case(DateUnit::Years, utc(2024, 2, 29), 1, utc(2025, 2, 28))]
    fn test_apply_add(
        #[case] unit: DateUnit,
        #[case] dt: DateTime<Utc>,
        #[case] amount: i64,
        #[case] expected: DateTime<Utc>,
    ) {
        assert_eq!(unit.apply_add(dt, amount).unwrap(), expected);
    }

    #[test]
    fn test_apply_add_negative() {
        let dt = utc(2024, 3, 1);
        assert_eq!(DateUnit::Months.apply_add(dt, -1).unwrap(), utc(2024, 2, 1));
        assert_eq!(DateUnit::Years.apply_add(dt, -1).unwrap(), utc(2023, 3, 1));
        assert_eq!(DateUnit::Days.apply_add(dt, -1).unwrap(), utc(2024, 2, 29));
    }

    // --- apply_diff ---

    #[rstest]
    #[case(DateUnit::Seconds, 120, 120)]
    #[case(DateUnit::Minutes, 120, 2)]
    #[case(DateUnit::Hours, 7200, 2)]
    #[case(DateUnit::Days, 172800, 2)]
    #[case(DateUnit::Weeks, 1209600, 2)]
    fn test_apply_diff(#[case] unit: DateUnit, #[case] secs: i64, #[case] expected: i64) {
        let d = Duration::seconds(secs);
        assert_eq!(unit.apply_diff(d).unwrap(), expected);
    }

    #[rstest]
    #[case(DateUnit::Months)]
    #[case(DateUnit::Years)]
    fn test_apply_diff_unsupported(#[case] unit: DateUnit) {
        let d = Duration::days(30);
        assert!(unit.apply_diff(d).is_err());
    }

    // --- add / diff wrappers ---

    #[test]
    fn test_add_wrapper() {
        let dt = utc(2024, 1, 1);
        assert_eq!(add(dt, 1, "days").unwrap(), utc(2024, 1, 2));
    }

    #[test]
    fn test_add_wrapper_invalid_unit() {
        let dt = utc(2024, 1, 1);
        assert!(add(dt, 1, "fortnight").is_err());
    }

    #[test]
    fn test_diff_wrapper() {
        let d = Duration::days(7);
        assert_eq!(diff(d, "weeks").unwrap(), 1);
    }

    #[test]
    fn test_diff_wrapper_invalid_unit() {
        let d = Duration::days(30);
        assert!(diff(d, "months").is_err());
    }

    // --- parse_relative ---
    // Base is 2024-01-15, a Monday.

    #[rstest]
    #[case("now", utc(2024, 1, 15))]
    #[case("today", utc(2024, 1, 15))]
    #[case("yesterday", utc(2024, 1, 14))]
    #[case("tomorrow", utc(2024, 1, 16))]
    #[case("Tomorrow", utc(2024, 1, 16))]
    #[case("  tomorrow  ", utc(2024, 1, 16))]
    #[case("1 day ago", utc(2024, 1, 14))]
    #[case("3 days ago", utc(2024, 1, 12))]
    #[case("2 weeks ago", utc(2024, 1, 1))]
    #[case("1 month ago", utc(2023, 12, 15))]
    #[case("1 year ago", utc(2023, 1, 15))]
    #[case("in 3 days", utc(2024, 1, 18))]
    #[case("in 2 weeks", utc(2024, 1, 29))]
    #[case("in 1 month", utc(2024, 2, 15))]
    #[case("In 1 Month", utc(2024, 2, 15))]
    #[case("next monday", utc(2024, 1, 22))]
    #[case("next friday", utc(2024, 1, 19))]
    #[case("last monday", utc(2024, 1, 8))]
    #[case("last friday", utc(2024, 1, 12))]
    fn test_parse_relative_valid(#[case] input: &str, #[case] expected: DateTime<Utc>) {
        let base = utc(2024, 1, 15);
        assert_eq!(parse_relative(input, base).unwrap(), expected);
    }

    #[rstest]
    #[case("")]
    #[case("someday")]
    #[case("3 decades ago")]
    #[case("in three days")]
    #[case("next someday")]
    #[case("last")]
    #[case("ago 3 days")]
    fn test_parse_relative_invalid(#[case] input: &str) {
        let base = utc(2024, 1, 15);
        assert!(parse_relative(input, base).is_err());
    }

    #[test]
    fn test_parse_relative_preserves_time_of_day() {
        let base = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 1, 22, 9, 30, 0).unwrap();
        assert_eq!(parse_relative("next monday", base).unwrap(), expected);
    }
}
