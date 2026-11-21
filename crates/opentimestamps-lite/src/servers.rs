//! The public servers opentimestamps-lite talks to.

/// Aggregators that accept new stamps (the official client's defaults).
pub(crate) const AGGREGATORS: [&str; 4] = [
    "https://a.pool.opentimestamps.org",
    "https://b.pool.opentimestamps.org",
    "https://a.pool.eternitywall.com",
    "https://ots.btc.catallaxy.com",
];

/// Block explorers sharing the Esplora API. Both must agree on a header.
pub(crate) const EXPLORERS: [&str; 2] =
    ["https://blockstream.info/api", "https://mempool.space/api"];

/// The official upgrade allowlist: `https://*.<domain>`.
const CALENDAR_DOMAINS: [&str; 3] = [
    ".calendar.opentimestamps.org",
    ".calendar.eternitywall.com",
    ".calendar.catallaxy.com",
];

/// Stamps need at least this many aggregators to accept.
pub(crate) const MIN_CALENDARS: usize = 2;

/// Largest calendar reply read, as the official client.
pub(crate) const MAX_REPLY_BYTES: usize = 10_000;

/// True for `https://<host>` (no port, path or user info) where host is on the allowlist.
pub(crate) fn allowlisted(url: &str) -> bool {
    url.strip_prefix("https://").is_some_and(allowlisted_host)
}

pub(crate) fn allowlisted_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'-')
        && CALENDAR_DOMAINS
            .iter()
            .any(|d| host.len() > d.len() && host.ends_with(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist() {
        assert!(allowlisted("https://alice.btc.calendar.opentimestamps.org"));
        assert!(allowlisted("https://finney.calendar.eternitywall.com"));
        assert!(allowlisted("https://btc.calendar.catallaxy.com"));
        for bad in [
            "http://alice.btc.calendar.opentimestamps.org",
            "https://a.pool.opentimestamps.org",
            "https://alice.btc.calendar.opentimestamps.org/x",
            "https://alice.btc.calendar.opentimestamps.org:8443",
            "https://evil.com/.calendar.opentimestamps.org",
            "https://.calendar.opentimestamps.org",
            "https://calendar.opentimestamps.org.evil.com",
        ] {
            assert!(!allowlisted(bad), "{bad}");
        }
    }
}
