use super::Error;
use jiff::civil::{DateTime, Time, Weekday};
use jiff::fmt::strtime::BrokenDownTime;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp, Unit};

/// Converts Unix seconds to a UTC civil datetime.
pub(super) fn from_unix_secs(secs: i64) -> Option<DateTime> {
    Timestamp::from_second(secs)
        .ok()
        .map(|ts| TimeZone::UTC.to_datetime(ts))
}

/// Returns the whole Unix second containing `ts`, rounding toward negative infinity.
/// `Timestamp::as_second` truncates toward zero, which is one second too late before the epoch.
pub(super) fn floor_unix_secs(ts: Timestamp) -> i64 {
    ts.as_second() - i64::from(ts.subsec_nanosecond() < 0)
}

/// Converts a UTC civil datetime to Unix seconds.
pub(super) fn to_unix_secs(dt: DateTime) -> Option<i64> {
    TimeZone::UTC.to_timestamp(dt).ok().map(floor_unix_secs)
}

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
    pub fn apply_add(self, dt: DateTime, amount: i64) -> Option<DateTime> {
        let span = match self {
            Self::Seconds => Span::new().try_seconds(amount),
            Self::Minutes => Span::new().try_minutes(amount),
            Self::Hours => Span::new().try_hours(amount),
            Self::Days => Span::new().try_days(amount),
            Self::Weeks => Span::new().try_weeks(amount),
            Self::Months => Span::new().try_months(amount),
            Self::Years => Span::new().try_years(amount),
        }
        .ok()?;
        dt.checked_add(span).ok()
    }

    fn unit(self) -> Unit {
        match self {
            Self::Seconds => Unit::Second,
            Self::Minutes => Unit::Minute,
            Self::Hours => Unit::Hour,
            Self::Days => Unit::Day,
            Self::Weeks => Unit::Week,
            Self::Months => Unit::Month,
            Self::Years => Unit::Year,
        }
    }

    /// Returns the number of whole units from `from` to `to`, truncated toward zero.
    pub fn apply_diff(self, from: DateTime, to: DateTime) -> Result<i64, Error> {
        let span = from
            .until((self.unit(), to))
            .map_err(|e| Error::Runtime(format!("date_diff: {}", e)))?;
        Ok(match self {
            Self::Seconds => span.get_seconds(),
            Self::Minutes => span.get_minutes(),
            Self::Hours => i64::from(span.get_hours()),
            Self::Days => i64::from(span.get_days()),
            Self::Weeks => i64::from(span.get_weeks()),
            Self::Months => i64::from(span.get_months()),
            Self::Years => i64::from(span.get_years()),
        })
    }
}

/// Public helper called from `builtin.rs` for `date_add`.
pub(super) fn add(dt: DateTime, amount: i64, unit: &str) -> Result<DateTime, Error> {
    let unit = DateUnit::try_from(unit)?;
    unit.apply_add(dt, amount)
        .ok_or_else(|| Error::Runtime("date_add: arithmetic overflow or invalid date".to_string()))
}

/// Public helper called from `builtin.rs` for `date_diff`.
pub(super) fn diff(from: DateTime, to: DateTime, unit: &str) -> Result<i64, Error> {
    DateUnit::try_from(unit)?.apply_diff(from, to)
}

/// Rewrites directives that jiff's strptime rejects into equivalent forms. `%z` has a plain and a
/// colon form (`+05:30`), and `%+` an offset and a trailing `Z` form. Each occurrence picks the
/// first form that matches the input at that point, found by parsing the format prefix up to it.
/// The work is quadratic in the number of such directives at worst.
fn strptime_format(fmt: &str, input: &str) -> String {
    const Z: [&str; 2] = ["%z", "%:z"];
    const PLUS: [&str; 2] = ["%Y-%m-%dT%H:%M:%S%.f%:z", "%Y-%m-%dT%H:%M:%S%.fZ"];

    let mut out = String::with_capacity(fmt.len());
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let forms: &[&str] = match chars.next() {
            Some('c') => &["%a %b %e %H:%M:%S %Y"],
            Some('x') => &["%m/%d/%y"],
            Some('X') => &["%H:%M:%S"],
            Some('r') => &["%I:%M:%S %p"],
            Some('z') => &Z,
            Some('+') => &PLUS,
            Some(spec) => {
                out.push('%');
                out.push(spec);
                continue;
            }
            None => {
                out.push('%');
                break;
            }
        };
        let form = forms
            .iter()
            .find(|form| BrokenDownTime::parse_prefix(format!("{out}{form}"), input).is_ok())
            .unwrap_or(&forms[0]);
        out.push_str(form);
    }
    out
}

/// Parses `input` with a strptime `fmt` and returns Unix seconds. An explicit epoch (`%s`) is used
/// as is. Otherwise the calendar fields are read as UTC, ignoring any offset, and a missing time
/// defaults to midnight.
pub(super) fn strptime(input: &str, fmt: &str) -> Result<i64, Error> {
    let tm = BrokenDownTime::parse(strptime_format(fmt, input), input)
        .map_err(|e| Error::Runtime(format!("strptime: {}", e)))?;
    strptime_secs(&tm)
}

fn strptime_secs(tm: &BrokenDownTime) -> Result<i64, Error> {
    if let Some(ts) = tm.timestamp() {
        return Ok(floor_unix_secs(ts));
    }
    let dt = tm
        .to_datetime()
        .or_else(|e| tm.to_date().map(|d| d.to_datetime(Time::midnight())).map_err(|_| e))
        .map_err(|e| Error::Runtime(format!("strptime: {}", e)))?;
    to_unix_secs(dt).ok_or_else(|| Error::Runtime("strptime: timestamp out of range".to_string()))
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
        "sunday" => Some(Weekday::Sunday),
        "monday" => Some(Weekday::Monday),
        "tuesday" => Some(Weekday::Tuesday),
        "wednesday" => Some(Weekday::Wednesday),
        "thursday" => Some(Weekday::Thursday),
        "friday" => Some(Weekday::Friday),
        "saturday" => Some(Weekday::Saturday),
        _ => None,
    }
}

/// Walks forward (or backward) from `base` a day at a time until `target` weekday is
/// reached, preserving `base`'s time-of-day. `base`'s own weekday never matches, so this
/// always lands on the next/previous occurrence, never today.
fn nearest_weekday(base: DateTime, target: Weekday, forward: bool) -> Option<DateTime> {
    let time = base.time();
    let mut date = base.date();

    loop {
        date = if forward {
            date.tomorrow().ok()?
        } else {
            date.yesterday().ok()?
        };
        if date.weekday() == target {
            return Some(DateTime::from_parts(date, time));
        }
    }
}

/// A relative-date expression, parsed out of its source string ahead of evaluation.
///
/// Splitting parsing (`&str` -> `RelativeExpr`) from evaluation (`RelativeExpr` -> `DateTime`)
/// means the two can't drift apart: every supported form is named once here, and `eval` is an
/// exhaustive match the compiler checks, rather than a second ad-hoc string match that has to be
/// kept in sync with the parser by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelativeExpr {
    Now,
    Ago { amount: i64, unit: DateUnit },
    In { amount: i64, unit: DateUnit },
    NextWeekday(Weekday),
    LastWeekday(Weekday),
}

impl RelativeExpr {
    /// Supported forms: "now", "today", "yesterday", "tomorrow", "<n> <unit> ago",
    /// "in <n> <unit>", "next <weekday>", "last <weekday>". Units accept singular or plural
    /// ("day"/"days", ...); weekdays use their full English name ("monday", ...).
    fn parse(s: &str) -> Result<Self, Error> {
        let unrecognized = || Error::Runtime(format!("date_relative: unrecognized relative date expression {:?}", s));
        let lower = s.trim().to_lowercase();

        match lower.as_str() {
            "now" | "today" => return Ok(Self::Now),
            "yesterday" => {
                return Ok(Self::Ago {
                    amount: 1,
                    unit: DateUnit::Days,
                });
            }
            "tomorrow" => {
                return Ok(Self::In {
                    amount: 1,
                    unit: DateUnit::Days,
                });
            }
            _ => {}
        }

        match lower.split_whitespace().collect::<Vec<_>>().as_slice() {
            [amount, unit_word, "ago"] => {
                let amount: i64 = amount.parse().map_err(|_| unrecognized())?;
                let unit = parse_relative_unit(unit_word).ok_or_else(unrecognized)?;
                Ok(Self::Ago { amount, unit })
            }
            ["in", amount, unit_word] => {
                let amount: i64 = amount.parse().map_err(|_| unrecognized())?;
                let unit = parse_relative_unit(unit_word).ok_or_else(unrecognized)?;
                Ok(Self::In { amount, unit })
            }
            ["next", weekday_word] => {
                let weekday = parse_weekday(weekday_word).ok_or_else(unrecognized)?;
                Ok(Self::NextWeekday(weekday))
            }
            ["last", weekday_word] => {
                let weekday = parse_weekday(weekday_word).ok_or_else(unrecognized)?;
                Ok(Self::LastWeekday(weekday))
            }
            _ => Err(unrecognized()),
        }
    }

    fn eval(self, base: DateTime) -> Result<DateTime, Error> {
        let overflow = || Error::Runtime("date_relative: arithmetic overflow or invalid date".to_string());

        match self {
            Self::Now => Ok(base),
            Self::Ago { amount, unit } => unit.apply_add(base, -amount).ok_or_else(overflow),
            Self::In { amount, unit } => unit.apply_add(base, amount).ok_or_else(overflow),
            Self::NextWeekday(weekday) => nearest_weekday(base, weekday, true).ok_or_else(overflow),
            Self::LastWeekday(weekday) => nearest_weekday(base, weekday, false).ok_or_else(overflow),
        }
    }
}

/// Parses a natural-language relative date expression relative to `base` and returns the
/// resulting UTC datetime.
///
/// Supported forms: "now", "today", "yesterday", "tomorrow", "<n> <unit> ago",
/// "in <n> <unit>", "next <weekday>", "last <weekday>". Units accept singular or plural
/// ("day"/"days", ...); weekdays use their full English name ("monday", ...).
pub(super) fn parse_relative(s: &str, base: DateTime) -> Result<DateTime, Error> {
    RelativeExpr::parse(s)?.eval(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn utc(year: i16, month: i8, day: i8) -> DateTime {
        jiff::civil::datetime(year, month, day, 0, 0, 0, 0)
    }

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

    #[rstest]
    #[case(DateUnit::Seconds, utc(2024, 1, 1), 86400, utc(2024, 1, 2))]
    #[case(DateUnit::Minutes, utc(2024, 1, 1), 1440, utc(2024, 1, 2))]
    #[case(DateUnit::Hours, utc(2024, 1, 1), 24, utc(2024, 1, 2))]
    #[case(DateUnit::Days, utc(2024, 1, 1), 31, utc(2024, 2, 1))]
    #[case(DateUnit::Weeks, utc(2024, 1, 1), 1, utc(2024, 1, 8))]
    #[case(DateUnit::Months, utc(2024, 1, 31), 1, utc(2024, 2, 29))] // 2024 is leap year
    #[case(DateUnit::Years, utc(2024, 2, 29), 1, utc(2025, 2, 28))]
    fn test_apply_add(#[case] unit: DateUnit, #[case] dt: DateTime, #[case] amount: i64, #[case] expected: DateTime) {
        assert_eq!(unit.apply_add(dt, amount).unwrap(), expected);
    }

    #[test]
    fn test_apply_add_negative() {
        let dt = utc(2024, 3, 1);
        assert_eq!(DateUnit::Months.apply_add(dt, -1).unwrap(), utc(2024, 2, 1));
        assert_eq!(DateUnit::Years.apply_add(dt, -1).unwrap(), utc(2023, 3, 1));
        assert_eq!(DateUnit::Days.apply_add(dt, -1).unwrap(), utc(2024, 2, 29));
    }

    #[rstest]
    #[case(DateUnit::Seconds, utc(2024, 1, 1), utc(2024, 1, 3), 172_800)]
    #[case(DateUnit::Minutes, utc(2024, 1, 1), utc(2024, 1, 3), 2_880)]
    #[case(DateUnit::Hours, utc(2024, 1, 1), utc(2024, 1, 3), 48)]
    #[case(DateUnit::Days, utc(2024, 1, 1), utc(2024, 1, 3), 2)]
    #[case(DateUnit::Weeks, utc(2024, 1, 1), utc(2024, 1, 15), 2)]
    #[case(DateUnit::Days, utc(2024, 1, 3), utc(2024, 1, 1), -2)]
    #[case(DateUnit::Months, utc(2024, 1, 15), utc(2024, 3, 15), 2)]
    #[case(DateUnit::Months, utc(2024, 1, 31), utc(2024, 2, 29), 0)]
    #[case(DateUnit::Months, utc(2024, 3, 15), utc(2024, 1, 15), -2)]
    #[case(DateUnit::Years, utc(2020, 2, 29), utc(2024, 2, 29), 4)]
    #[case(DateUnit::Years, utc(2020, 2, 29), utc(2024, 2, 28), 3)]
    fn test_apply_diff(#[case] unit: DateUnit, #[case] from: DateTime, #[case] to: DateTime, #[case] expected: i64) {
        assert_eq!(unit.apply_diff(from, to).unwrap(), expected);
    }

    #[test]
    fn test_apply_diff_truncates_toward_zero() {
        let from = jiff::civil::datetime(2024, 1, 1, 0, 0, 0, 0);
        let to = jiff::civil::datetime(2024, 1, 2, 12, 0, 0, 0);
        assert_eq!(DateUnit::Days.apply_diff(from, to).unwrap(), 1);
        assert_eq!(DateUnit::Days.apply_diff(to, from).unwrap(), -1);
    }

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
        assert_eq!(diff(utc(2024, 1, 1), utc(2024, 1, 8), "weeks").unwrap(), 1);
    }

    #[test]
    fn test_diff_wrapper_invalid_unit() {
        assert!(diff(utc(2024, 1, 1), utc(2024, 1, 8), "fortnights").is_err());
    }

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
    fn test_parse_relative_valid(#[case] input: &str, #[case] expected: DateTime) {
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
        let base = jiff::civil::datetime(2024, 1, 15, 9, 30, 0, 0);
        let expected = jiff::civil::datetime(2024, 1, 22, 9, 30, 0, 0);
        assert_eq!(parse_relative("next monday", base).unwrap(), expected);
    }

    #[rstest]
    #[case("1970-01-01T00:00:00Z", 0)]
    #[case("1969-12-31T23:59:59.5Z", -1)]
    #[case("1969-12-31T23:59:59Z", -1)]
    #[case("1970-01-01T00:00:00.5Z", 0)]
    fn test_floor_unix_secs(#[case] input: &str, #[case] expected: i64) {
        assert_eq!(floor_unix_secs(input.parse().unwrap()), expected);
    }

    #[rstest]
    #[case("1704067200", "%s", 1_704_067_200)]
    #[case("-1", "%s", -1)]
    #[case("2024-01-01", "%Y-%m-%d", 1_704_067_200)]
    #[case("1969-12-31 23:59:59.5", "%Y-%m-%d %H:%M:%S%.f", -1)]
    #[case("2024-01-01 00:00:00 +0530", "%Y-%m-%d %H:%M:%S %z", 1_704_067_200)]
    #[case("2024-01-01 00:00:00 +05:30", "%Y-%m-%d %H:%M:%S %z", 1_704_067_200)]
    #[case("2024-01-01T00:00:00+05:30", "%+", 1_704_067_200)]
    #[case("2024-01-01T00:00:00Z", "%+", 1_704_067_200)]
    #[case("2024-01-01 local=+0530 source=+05:30", "%Y-%m-%d local=%z source=%z", 1_704_067_200)]
    #[case("2024-01-01 a=+05:30 b=+0530", "%Y-%m-%d a=%z b=%z", 1_704_067_200)]
    #[case("2024-01-01T00:00:00Z|2024-01-01T00:00:00+05:30", "%+|%+", 1_704_067_200)]
    #[case("2024-01-01T00:00:00+05:30|2024-01-01T00:00:00Z", "%+|%+", 1_704_067_200)]
    #[case("Mon Jan  1 00:00:00 2024", "%c", 1_704_067_200)]
    #[case("01/02/24", "%x", 1_704_153_600)]
    #[case("2024-01-01 10:20:30", "%Y-%m-%d %X", 1_704_104_430)]
    #[case("2024-01-01 10:20:30 PM", "%Y-%m-%d %r", 1_704_147_630)]
    fn test_strptime(#[case] input: &str, #[case] fmt: &str, #[case] expected: i64) {
        assert_eq!(strptime(input, fmt).unwrap(), expected);
    }

    #[rstest]
    #[case("not a date", "%Y-%m-%d")]
    #[case("2024-01-01", "%Y-%m-%d %H")]
    #[case("10:20:30", "%H:%M:%S")]
    fn test_strptime_invalid(#[case] input: &str, #[case] fmt: &str) {
        assert!(strptime(input, fmt).is_err());
    }

    #[test]
    fn test_strptime_format_keeps_escaped_percent() {
        assert_eq!(strptime_format("%%z %c", ""), "%%z %a %b %e %H:%M:%S %Y");
    }

    #[test]
    fn test_strptime_many_offset_directives() {
        let fmt = "%z".repeat(64) + &"%+".repeat(64);
        assert!(strptime("", &fmt).is_err());
    }
}
