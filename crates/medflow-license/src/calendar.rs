//! License durations on the UTC calendar.
//!
//! Every timestamp is unix milliseconds. Month and year arithmetic is
//! calendar-correct: adding one month keeps the day of month and the time of
//! day, clamping to the last day of a shorter month (Jan 31 + 1 month = Feb 28,
//! or Feb 29 in a leap year; Feb 29 + 1 year = Feb 28). No calendar crate — the
//! civil-date conversions are Howard Hinnant's algorithms, exact for every
//! proleptic Gregorian date.

pub const MS_PER_DAY: i64 = 86_400_000;

/// How long a key lasts, as issued by the Key Maker form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term {
    Months(u32),
    Years(u32),
    /// A custom length in whole days.
    Days(u32),
    Lifetime,
}

impl Term {
    /// Stable machine-readable kind for UIs: `months` / `years` / `days` / `lifetime`.
    pub fn kind(self) -> &'static str {
        match self {
            Term::Months(_) => "months",
            Term::Years(_) => "years",
            Term::Days(_) => "days",
            Term::Lifetime => "lifetime",
        }
    }

    /// The count that goes with `kind()`; `None` for lifetime.
    pub fn count(self) -> Option<u32> {
        match self {
            Term::Months(n) | Term::Years(n) | Term::Days(n) => Some(n),
            Term::Lifetime => None,
        }
    }

    /// `expiresAt` for a key issued at `issued_at_ms`; `None` means lifetime.
    pub fn expires_at(self, issued_at_ms: i64) -> Option<i64> {
        match self {
            Term::Months(n) => Some(add_months(issued_at_ms, n)),
            Term::Years(n) => Some(add_years(issued_at_ms, n)),
            Term::Days(n) => Some(issued_at_ms + i64::from(n) * MS_PER_DAY),
            Term::Lifetime => None,
        }
    }

    /// Recover the term a key was issued with from its two timestamps. Keys cut
    /// by `Term::expires_at` round-trip exactly; anything else is reported as a
    /// custom length in days (rounded up, so "2.5 days" reads as 3).
    pub fn classify(issued_at_ms: i64, expires_at_ms: Option<i64>) -> Term {
        let Some(expires) = expires_at_ms else {
            return Term::Lifetime;
        };
        for years in 1..=100u32 {
            let candidate = add_years(issued_at_ms, years);
            if candidate == expires {
                return Term::Years(years);
            }
            if candidate > expires {
                break;
            }
        }
        for months in 1..=1200u32 {
            let candidate = add_months(issued_at_ms, months);
            if candidate == expires {
                return Term::Months(months);
            }
            if candidate > expires {
                break;
            }
        }
        let span = (expires - issued_at_ms).max(0);
        let days = (span + MS_PER_DAY - 1) / MS_PER_DAY;
        Term::Days(u32::try_from(days).unwrap_or(u32::MAX))
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (month 1–12).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of `days_from_civil`: (year, month 1–12, day 1–31).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap_year(year) => 29,
        _ => 28,
    }
}

/// Unix ms for a UTC wall-clock instant. `None` for an impossible date.
pub fn unix_ms_from_utc(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let seconds = days_from_civil(year, month, day) * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second);
    Some(seconds * 1_000)
}

/// Calendar-correct month addition in UTC (see module docs for clamping).
pub fn add_months(unix_ms: i64, months: u32) -> i64 {
    let days = unix_ms.div_euclid(MS_PER_DAY);
    let time_of_day = unix_ms.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let month_index = year * 12 + i64::from(month - 1) + i64::from(months);
    let new_year = month_index.div_euclid(12);
    let new_month = (month_index.rem_euclid(12) + 1) as u32;
    let new_day = day.min(days_in_month(new_year, new_month));
    days_from_civil(new_year, new_month, new_day) * MS_PER_DAY + time_of_day
}

pub fn add_years(unix_ms: i64, years: u32) -> i64 {
    add_months(unix_ms, years.saturating_mul(12))
}

/// Parse an RFC 9110 IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`) — the form
/// every mainstream HTTPS server sends in its `Date` header — to unix ms.
pub fn parse_http_date(value: &str) -> Option<i64> {
    let rest = value.trim();
    let (_weekday, rest) = rest.split_once(", ")?;
    let mut parts = rest.split(' ');
    let day: u32 = parts.next()?.parse().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: i64 = parts.next()?.parse().ok()?;
    let mut clock = parts.next()?.split(':');
    let hour: u32 = clock.next()?.parse().ok()?;
    let minute: u32 = clock.next()?.parse().ok()?;
    let second: u32 = clock.next()?.parse().ok()?;
    if parts.next()? != "GMT" || parts.next().is_some() || clock.next().is_some() {
        return None;
    }
    unix_ms_from_utc(year, month, day, hour, minute, second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i64, month: u32, day: u32, hour: u32) -> i64 {
        unix_ms_from_utc(year, month, day, hour, 0, 0).expect("valid date")
    }

    #[test]
    fn civil_round_trip_covers_leap_and_pre_epoch_days() {
        for days in [-800_000i64, -1, 0, 1, 11_016, 20_000, 2_932_896] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1) - days_from_civil(2000, 2, 28), 2);
    }

    #[test]
    fn month_addition_keeps_day_and_time_and_clamps_short_months() {
        assert_eq!(add_months(at(2026, 9, 25, 14), 1), at(2026, 10, 25, 14));
        assert_eq!(add_months(at(2026, 1, 31, 9), 1), at(2026, 2, 28, 9));
        assert_eq!(add_months(at(2028, 1, 31, 9), 1), at(2028, 2, 29, 9));
        assert_eq!(add_months(at(2026, 11, 30, 0), 3), at(2027, 2, 28, 0));
        assert_eq!(add_months(at(2026, 12, 15, 0), 1), at(2027, 1, 15, 0));
        assert_eq!(add_months(at(2026, 5, 10, 0), 24), at(2028, 5, 10, 0));
    }

    #[test]
    fn year_addition_is_twelve_months_and_handles_feb_29() {
        assert_eq!(add_years(at(2028, 2, 29, 8), 1), at(2029, 2, 28, 8));
        assert_eq!(add_years(at(2028, 2, 29, 8), 4), at(2032, 2, 29, 8));
        assert_eq!(add_years(at(2026, 9, 25, 0), 2), at(2028, 9, 25, 0));
    }

    #[test]
    fn terms_round_trip_through_classify() {
        let issued = at(2026, 1, 31, 10);
        for term in [
            Term::Months(1),
            Term::Months(3),
            Term::Months(6),
            Term::Months(18),
            Term::Years(1),
            Term::Years(2),
            Term::Days(45),
            Term::Lifetime,
        ] {
            assert_eq!(Term::classify(issued, term.expires_at(issued)), term, "{term:?}");
        }
        // 12 months is reported as a year — the same instant either way.
        assert_eq!(
            Term::classify(issued, Term::Months(12).expires_at(issued)),
            Term::Years(1)
        );
        assert_eq!(Term::classify(issued, Some(issued + MS_PER_DAY / 2)), Term::Days(1));
    }

    #[test]
    fn http_date_parses_imf_fixdate_only() {
        assert_eq!(
            parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(784_111_777_000)
        );
        assert_eq!(parse_http_date("Sunday, 06-Nov-94 08:49:37 GMT"), None);
        assert_eq!(parse_http_date("Sun, 31 Feb 1994 08:49:37 GMT"), None);
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 08:49:37 UTC"), None);
        assert_eq!(parse_http_date(""), None);
    }
}
