/// Formats a FILETIME (100-nanosecond intervals since 1601-01-01 UTC) as
/// ISO 8601, or `None` for zero, which property sets use for "unset".
/// [MS-DTYP] FILETIME, as used by [MS-OLEPS] §2.15 VT_FILETIME.
pub fn filetime_to_iso8601(filetime: u64) -> Option<String> {
    if filetime == 0 {
        return None;
    }
    let seconds = filetime / 10_000_000;
    const UNIX_OFFSET: u64 = 11_644_473_600;
    let unix = seconds.checked_sub(UNIX_OFFSET)? as i64;
    let days = unix.div_euclid(86_400);
    let rem = unix.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    ))
}

/// Days since 1970-01-01 to a proleptic Gregorian date.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_epoch_and_known_dates() {
        assert_eq!(filetime_to_iso8601(0), None);
        assert_eq!(
            filetime_to_iso8601(116_444_736_000_000_000).as_deref(),
            Some("1970-01-01T00:00:00Z")
        );
        assert_eq!(
            filetime_to_iso8601(132_539_328_000_000_000).as_deref(),
            Some("2021-01-01T00:00:00Z")
        );
    }
}
