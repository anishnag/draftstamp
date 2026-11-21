//! The DraftStamp status link (spec/0000_ots.md, "Status link").
//!
//! ```text
//! https://draftstamp.org/status#v=1&cal=alice:<message>&cal=bob:<message>
//! ```

use crate::proof::{CalendarEntry, Status};
use crate::servers::allowlisted_host;
use crate::util::{b64url_decode, b64url_encode};
use crate::{Error, Http, flows};

/// Where the static status page lives.
pub const STATUS_PAGE: &str = "https://draftstamp.org/status";

/// Short names for calendars. Frozen when v1 ships: never extended or changed.
/// Calendars not listed are written as full hosts.
const NAMES: [(&str, &str); 4] = [
    ("alice", "alice.btc.calendar.opentimestamps.org"),
    ("bob", "bob.btc.calendar.opentimestamps.org"),
    ("finney", "finney.calendar.eternitywall.com"),
    ("catallaxy", "btc.calendar.catallaxy.com"),
];

const MAX_MESSAGE: usize = 4096;

/// Builds the status link from a proof's calendar entries (`proof.calendars()`).
/// Entries whose calendar is not on the allowlist are left out; fails if none remain.
pub fn encode(calendars: &[CalendarEntry]) -> Result<String, Error> {
    let mut params: Vec<String> = calendars
        .iter()
        .filter_map(|e| {
            let host = e
                .url
                .strip_prefix("https://")
                .filter(|h| allowlisted_host(h))?;
            let label = NAMES
                .iter()
                .find(|(_, h)| *h == host)
                .map_or(host, |(n, _)| n);
            Some(format!("cal={label}:{}", b64url_encode(&e.message)))
        })
        .collect();
    if params.is_empty() {
        return Err(Error::BadLink("no allowlisted calendars"));
    }
    params.sort();
    params.dedup();
    Ok(format!("{STATUS_PAGE}#v=1&{}", params.join("&")))
}

/// Reads a status link (a full URL, or just the part after `#`) back into calendar entries.
pub fn decode(link: &str) -> Result<Vec<CalendarEntry>, Error> {
    let fragment = link.split_once('#').map_or(link, |(_, f)| f);
    let mut version = None;
    let mut entries = Vec::new();
    for param in fragment.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = param
            .split_once('=')
            .ok_or(Error::BadLink("parameter without '='"))?;
        match key {
            "v" if version.is_some() => return Err(Error::BadLink("repeated version")),
            "v" => version = Some(value),
            "cal" => {
                let (label, b64) = value
                    .split_once(':')
                    .ok_or(Error::BadLink("calendar without ':'"))?;
                let host = if label.contains('.') {
                    label
                } else {
                    NAMES
                        .iter()
                        .find(|(n, _)| *n == label)
                        .map(|(_, h)| *h)
                        .ok_or(Error::BadLink("unknown calendar name"))?
                };
                if !allowlisted_host(host) {
                    return Err(Error::BadLink("calendar not on the allowlist"));
                }
                let message =
                    b64url_decode(b64).ok_or(Error::BadLink("message is not base64url"))?;
                if message.is_empty() || message.len() > MAX_MESSAGE {
                    return Err(Error::BadLink("message length"));
                }
                entries.push(CalendarEntry {
                    url: format!("https://{host}"),
                    message,
                });
            }
            _ => {} // unknown parameters are ignored
        }
    }
    match version {
        Some("1") => {}
        Some(_) => return Err(Error::BadLink("unsupported version")),
        None => return Err(Error::BadLink("missing version")),
    }
    if entries.is_empty() {
        return Err(Error::BadLink("no calendars"));
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

/// Live status of a link: asks its calendars and checks Bitcoin. Read-only.
/// Shows that the stamp is anchored, not which commitment it covers.
pub async fn status(http: &impl Http, link: &str) -> Result<Status, Error> {
    let entries = decode(link)?;
    flows::status_of(http, &entries).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(host: &str, fill: u8) -> CalendarEntry {
        CalendarEntry {
            url: format!("https://{host}"),
            message: vec![fill; 44],
        }
    }

    #[test]
    fn round_trip_with_names_and_full_hosts() {
        let entries = vec![
            entry("bob.btc.calendar.opentimestamps.org", 2),
            entry("alice.btc.calendar.opentimestamps.org", 1),
            entry("new.calendar.opentimestamps.org", 3),
        ];
        let link = encode(&entries).unwrap();
        assert!(
            link.starts_with("https://draftstamp.org/status#v=1&cal=alice:AQEB"),
            "{link}"
        );
        assert!(link.contains("&cal=new.calendar.opentimestamps.org:"));
        let mut expected = entries.clone();
        expected.sort();
        assert_eq!(decode(&link).unwrap(), expected);
        assert_eq!(
            decode(link.split_once('#').unwrap().1).unwrap(),
            expected,
            "fragment alone"
        );
        // "#v=1" + "&cal=alice:" + 59 + "&cal=bob:" + 59 + "&cal=<31-char host>:" + 59
        assert_eq!(
            link.len(),
            STATUS_PAGE.len() + 4 + (11 + 59) + (9 + 59) + (6 + 31 + 59)
        );
    }

    #[test]
    fn encode_skips_non_allowlisted() {
        assert!(encode(&[entry("evil.com", 1)]).is_err());
        let link = encode(&[
            entry("evil.com", 1),
            entry("bob.btc.calendar.opentimestamps.org", 2),
        ])
        .unwrap();
        assert!(!link.contains("evil"));
    }

    #[test]
    fn decode_rejects() {
        let msg = b64url_encode(&[1; 44]);
        let bad = [
            format!("#cal=alice:{msg}"),
            format!("#v=2&cal=alice:{msg}"),
            format!("#v=1&v=1&cal=alice:{msg}"),
            format!("#v=1&cal=zeta:{msg}"),
            format!("#v=1&cal=evil.com:{msg}"),
            "#v=1&cal=alice:not*base64".to_string(),
            "#v=1&cal=alice".to_string(),
            "#v=1".to_string(),
        ];
        for link in bad {
            assert_eq!(decode(&link).unwrap_err().code(), "bad_link", "{link}");
        }
        assert!(
            decode(&format!("#v=1&future=x&cal=alice:{msg}")).is_ok(),
            "unknown params ignored"
        );
    }
}
