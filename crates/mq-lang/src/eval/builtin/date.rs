use super::Error;
use chrono::{DateTime, Duration, Months, Utc};

/// Date/time units used by `date_add` and `date_diff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateUnit {
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
pub fn add(dt: DateTime<Utc>, amount: i64, unit: &str) -> Result<DateTime<Utc>, Error> {
    let unit = DateUnit::try_from(unit)?;
    unit.apply_add(dt, amount)
        .ok_or_else(|| Error::Runtime("date_add: arithmetic overflow or invalid date".to_string()))
}

/// Public helper called from `builtin.rs` for `date_diff`.
pub fn diff(diff: Duration, unit: &str) -> Result<i64, Error> {
    let unit = DateUnit::try_from(unit)?;
    unit.apply_diff(diff)
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
}
