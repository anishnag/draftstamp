//! Stamp ids: `YYYYMMDDTHHMMSSZ-<first 6 hex characters of the commitment>`.

use std::fmt;
use std::str::FromStr;

use crate::Error;
use crate::commitment::Digest;

/// Identifies one stamp. Sorts by time.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StampId(String);

impl StampId {
    pub(crate) fn new(unix_seconds: u64, commitment: &Digest) -> StampId {
        let (y, mo, d, h, mi, s) = utc(unix_seconds);
        StampId(format!(
            "{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z-{}",
            &commitment.to_string()[..6]
        ))
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StampId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for StampId {
    type Err = Error;
    fn from_str(s: &str) -> Result<StampId, Error> {
        let b = s.as_bytes();
        let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
        let ok = b.len() == 23
            && digits(0..8)
            && b[8] == b'T'
            && digits(9..15)
            && b[15] == b'Z'
            && b[16] == b'-'
            && b[17..]
                .iter()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c));
        if ok {
            Ok(StampId(s.to_owned()))
        } else {
            Err(Error::NotFound(s.to_owned()))
        }
    }
}

/// ISO 8601 UTC time, e.g. `2026-11-21T14:03:12Z`.
pub fn iso_time(unix_seconds: u64) -> String {
    let (y, mo, d, h, mi, s) = utc(unix_seconds);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Unix seconds to (year, month, day, hour, minute, second) in UTC (civil-from-days algorithm).
fn utc(t: u64) -> (u64, u64, u64, u64, u64, u64) {
    let (days, secs) = (t / 86_400, t % 86_400);
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    (y, m, d, secs / 3600, secs % 3600 / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses() {
        let c: Digest = format!("e72b05{}", "0".repeat(58)).parse().unwrap();
        let id = StampId::new(1_763_733_792, &c);
        assert_eq!(id.as_str(), "20251121T140312Z-e72b05");
        assert_eq!(id.as_str().parse::<StampId>().unwrap(), id);
        assert_eq!(iso_time(1_763_733_792), "2025-11-21T14:03:12Z");
        assert_eq!(iso_time(951_782_400), "2000-02-29T00:00:00Z", "leap day");
        for bad in [
            "",
            "x",
            "20251121T140312Z-E72B05",
            "20251121X140312Z-e72b05",
            ".tmp-20251121T140312Z-e72b05",
        ] {
            assert!(bad.parse::<StampId>().is_err(), "{bad}");
        }
    }
}
