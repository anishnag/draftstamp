//! The network flows: stamp and upgrade. Every protocol rule lives here.

use std::thread;

use crate::bitcoin::{block_hash_hex, check_header};
use crate::format::attestation::Attestation;
use crate::format::op::Op;
use crate::format::tree::{Node, read_fragment};
use crate::proof::{CalendarEntry, Proof, Status};
use crate::servers::{AGGREGATORS, EXPLORERS, MAX_REPLY_BYTES, MIN_CALENDARS, allowlisted};
use crate::util::{hex_decode, hex_encode, sha256};
use crate::{Error, Http, Response};

/// Stamps a digest: blinds it with a random nonce, submits it to all four aggregators in
/// parallel, and returns a pending proof. Fails unless at least two aggregators accept.
pub fn stamp(http: &impl Http, digest: [u8; 32]) -> Result<Proof, Error> {
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(|_| Error::Random)?;
    stamp_with_nonce(http, digest, nonce)
}

pub(crate) fn stamp_with_nonce(
    http: &impl Http,
    digest: [u8; 32],
    nonce: [u8; 16],
) -> Result<Proof, Error> {
    let submitted = sha256(&[&digest, &nonce]);
    let urls: Vec<String> = AGGREGATORS.iter().map(|a| format!("{a}/digest")).collect();
    let replies = parallel(&urls, |url| http.post(url, &submitted));

    let mut branches = Node::default();
    let mut accepted = 0;
    for reply in replies {
        if let Some(fragment) = ok_fragment(reply, &submitted)
            && has_pending(&fragment)
        {
            branches.merge(fragment);
            accepted += 1;
        }
    }
    if accepted < MIN_CALENDARS {
        return Err(Error::TooFewCalendars { accepted });
    }
    let hashed = Node {
        ops: [(Op::Sha256, branches)].into(),
        ..Node::default()
    };
    let tree = Node {
        ops: [(Op::Append(nonce.to_vec()), hashed)].into(),
        ..Node::default()
    };
    Ok(Proof::new(digest, tree))
}

/// Asks each unanswered calendar for its path to Bitcoin in parallel, merges what arrives
/// (adding only), then checks the result from the digest against Bitcoin.
pub fn upgrade(http: &impl Http, proof: &mut Proof) -> Result<Status, Error> {
    let todo: Vec<CalendarEntry> = proof
        .unanswered()
        .into_iter()
        .filter(|e| allowlisted(&e.url))
        .collect();
    let replies = parallel(&todo, |entry| http.get(&timestamp_url(entry)));
    for (entry, reply) in todo.iter().zip(replies) {
        if let Some(fragment) = ok_fragment(reply, &entry.message) {
            proof.graft(&entry.message, &fragment);
        }
    }
    verify_claims(http, &proof.bitcoin_claims())
}

fn timestamp_url(e: &CalendarEntry) -> String {
    format!("{}/timestamp/{}", e.url, hex_encode(&e.message))
}

fn ok_fragment(reply: Result<Response, Error>, msg: &[u8]) -> Option<Node> {
    let resp = reply.ok()?;
    if resp.status != 200 || resp.body.len() > MAX_REPLY_BYTES {
        return None;
    }
    read_fragment(&resp.body, msg).ok()
}

fn has_pending(node: &Node) -> bool {
    let mut found = false;
    node.walk(None, &mut |n, _| {
        found |= n
            .attestations
            .iter()
            .any(|a| matches!(a, Attestation::Pending { .. }));
    });
    found
}

/// Checks Bitcoin claims, lowest height first. `Anchored` for the first that both explorers
/// confirm; an error if explorers agree on a header that contradicts every claim; otherwise
/// `Claimed` (headers unavailable or explorers disagree), or `Pending` if there are no claims.
fn verify_claims(http: &impl Http, claims: &[(u64, [u8; 32])]) -> Result<Status, Error> {
    let Some(&(lowest, _)) = claims.first() else {
        return Ok(Status::Pending);
    };
    let mut contradicted = false;
    for &(height, message) in claims {
        if let Some(header) = agreed_header(http, height) {
            match check_header(&message, &header) {
                Ok(time) => return Ok(Status::Anchored { height, time }),
                Err(_) => contradicted = true,
            }
        }
    }
    if contradicted {
        Err(Error::HeaderMismatch)
    } else {
        Ok(Status::Claimed { height: lowest })
    }
}

/// The header at `height`, only if every explorer returns the same one and it hashes to the
/// block hash they report.
fn agreed_header(http: &impl Http, height: u64) -> Option<[u8; 80]> {
    let headers = parallel(&EXPLORERS, |explorer| {
        explorer_header(http, explorer, height)
    });
    let first = headers.first()?.as_ref()?;
    headers
        .iter()
        .all(|h| h.as_ref() == Some(first))
        .then_some(*first)
}

fn explorer_header(http: &impl Http, explorer: &str, height: u64) -> Option<[u8; 80]> {
    let hash = text(http.get(&format!("{explorer}/block-height/{height}")))?;
    if hash.len() != 64 {
        return None;
    }
    let hex = text(http.get(&format!("{explorer}/block/{hash}/header")))?;
    let header: [u8; 80] = hex_decode(&hex)?.try_into().ok()?;
    (block_hash_hex(&header) == hash.to_ascii_lowercase()).then_some(header)
}

fn text(reply: Result<Response, Error>) -> Option<String> {
    let resp = reply.ok()?;
    if resp.status != 200 || resp.body.len() > 1024 {
        return None;
    }
    Some(String::from_utf8(resp.body).ok()?.trim().to_string())
}

/// Runs `f` on every item in parallel on scoped threads and returns the results in order.
/// A panic in `f` (for example inside the caller's `Http`) is re-raised here.
fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let f = &f;
    thread::scope(|scope| {
        let handles: Vec<_> = items
            .iter()
            .map(|item| scope.spawn(move || f(item)))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/aggregator-replies"
    );

    /// Real replies recorded from the four aggregators on 2026-11-20 for one submitted digest.
    /// They start from the submitted bytes, which no digest can be rebuilt from, so they are
    /// checked as a proof rooted at that value: parse, re-serialize, merge, write, re-read.
    #[test]
    fn real_aggregator_replies() {
        let submitted = std::fs::read(format!("{FIXTURES}/submitted-digest.bin")).unwrap();
        let mut branches = Node::default();
        for host in AGGREGATORS.map(|a| a.trim_start_matches("https://")) {
            let bytes = std::fs::read(format!("{FIXTURES}/{host}.bin")).unwrap();
            let fragment = read_fragment(&bytes, &submitted).unwrap();
            assert!(has_pending(&fragment), "{host}");
            let mut rewritten = Vec::new();
            fragment.write(&mut rewritten);
            assert_eq!(
                rewritten, bytes,
                "{host}: reply re-serializes byte for byte"
            );
            branches.merge(fragment);
        }
        let proof = Proof::new(submitted.try_into().unwrap(), branches);
        assert_eq!(Proof::from_bytes(&proof.to_bytes()).unwrap(), proof);
        assert_eq!(proof.status(), Status::Pending);

        let pending = proof.unanswered();
        let hosts: Vec<&str> = pending.iter().map(|c| c.url.as_str()).collect();
        assert_eq!(
            hosts,
            [
                "https://alice.btc.calendar.opentimestamps.org",
                "https://bob.btc.calendar.opentimestamps.org",
                "https://btc.calendar.catallaxy.com",
                "https://finney.calendar.eternitywall.com",
            ]
        );
        assert!(pending.iter().all(|c| allowlisted(&c.url)));
        assert!(
            pending.iter().all(|c| c.message.len() == 44),
            "pending messages are 44 bytes"
        );
    }

    #[test]
    fn parallel_keeps_order() {
        let items = [3u64, 1, 2];
        assert_eq!(parallel(&items, |n| n * 10), vec![30, 10, 20]);
    }
}
