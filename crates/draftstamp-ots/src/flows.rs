//! The network flows: stamp, upgrade, and live link status. Every protocol rule lives here.

use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;

use crate::bitcoin::{block_hash_hex, check_header};
use crate::format::op::Op;
use crate::format::tree::{Node, read_fragment};
use crate::proof::{CalendarEntry, Proof, Status};
use crate::servers::{AGGREGATORS, EXPLORERS, MAX_REPLY_BYTES, MIN_CALENDARS, allowlisted};
use crate::util::{hex_decode, hex_encode, sha256};
use crate::{Error, Http, Response};

/// Stamps a commitment: blinds it with a random nonce, submits to all four aggregators,
/// and returns a pending proof. Fails unless at least two aggregators accept.
pub async fn stamp(http: &impl Http, commitment: [u8; 32]) -> Result<Proof, Error> {
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(|_| Error::Random)?;
    stamp_with_nonce(http, commitment, nonce).await
}

pub(crate) async fn stamp_with_nonce(
    http: &impl Http,
    commitment: [u8; 32],
    nonce: [u8; 16],
) -> Result<Proof, Error> {
    let submitted = sha256(&[&commitment, &nonce]);
    let urls: Vec<String> = AGGREGATORS.iter().map(|a| format!("{a}/digest")).collect();
    let replies = join_all(urls.iter().map(|u| http.post(u, &submitted))).await;

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
    Ok(Proof::new(commitment, tree))
}

/// Asks each unanswered calendar for its path to Bitcoin, merges what arrives (adding only),
/// then checks the result from the commitment against Bitcoin.
pub async fn upgrade(http: &impl Http, proof: &mut Proof) -> Result<Status, Error> {
    let todo: Vec<CalendarEntry> = proof
        .unanswered()
        .into_iter()
        .filter(|e| allowlisted(&e.url))
        .collect();
    let urls: Vec<String> = todo.iter().map(timestamp_url).collect();
    let replies = join_all(urls.iter().map(|u| http.get(u))).await;
    for (entry, reply) in todo.iter().zip(replies) {
        if let Some(fragment) = ok_fragment(reply, &entry.message) {
            proof.graft(&entry.message, &fragment);
        }
    }
    verify_claims(http, &proof.bitcoin_claims()).await
}

/// Live status for calendar entries from a link: the same calendar requests and header checks
/// as `upgrade`, starting from each calendar's message, saving nothing.
pub(crate) async fn status_of(
    http: &impl Http,
    entries: &[CalendarEntry],
) -> Result<Status, Error> {
    let entries: Vec<&CalendarEntry> = entries.iter().filter(|e| allowlisted(&e.url)).collect();
    let urls: Vec<String> = entries.iter().map(|e| timestamp_url(e)).collect();
    let replies = join_all(urls.iter().map(|u| http.get(u))).await;
    let mut claims = Vec::new();
    for (entry, reply) in entries.iter().zip(replies) {
        if let Some(fragment) = ok_fragment(reply, &entry.message) {
            claims.extend(fragment.bitcoin_claims(&entry.message));
        }
    }
    claims.sort();
    claims.dedup();
    verify_claims(http, &claims).await
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
            .any(|a| matches!(a, crate::format::attestation::Attestation::Pending { .. }));
    });
    found
}

/// Checks Bitcoin claims, lowest height first. `Anchored` for the first that both explorers
/// confirm; an error if explorers agree on a header that contradicts every claim; otherwise
/// `Claimed` (headers unavailable or explorers disagree), or `Pending` if there are no claims.
async fn verify_claims(http: &impl Http, claims: &[(u64, [u8; 32])]) -> Result<Status, Error> {
    let Some(&(lowest, _)) = claims.first() else {
        return Ok(Status::Pending);
    };
    let mut contradicted = false;
    for &(height, message) in claims {
        if let Some(header) = agreed_header(http, height).await {
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
async fn agreed_header(http: &impl Http, height: u64) -> Option<[u8; 80]> {
    let headers = join_all(EXPLORERS.iter().map(|e| explorer_header(http, e, height))).await;
    let first = headers.first()?.as_ref()?;
    headers
        .iter()
        .all(|h| h.as_ref() == Some(first))
        .then_some(*first)
}

async fn explorer_header(http: &impl Http, explorer: &str, height: u64) -> Option<[u8; 80]> {
    let hash = text(http.get(&format!("{explorer}/block-height/{height}")).await)?;
    if hash.len() != 64 {
        return None;
    }
    let hex = text(http.get(&format!("{explorer}/block/{hash}/header")).await)?;
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

/// Runs futures concurrently on whatever executor drives the caller; no runtime dependency.
async fn join_all<F: Future>(futures: impl IntoIterator<Item = F>) -> Vec<F::Output> {
    let mut futures: Vec<Pin<Box<F>>> = futures.into_iter().map(Box::pin).collect();
    let mut outputs: Vec<Option<F::Output>> = futures.iter().map(|_| None).collect();
    poll_fn(|cx| {
        let mut done = true;
        for (f, out) in futures.iter_mut().zip(outputs.iter_mut()) {
            if out.is_none() {
                match f.as_mut().poll(cx) {
                    Poll::Ready(v) => *out = Some(v),
                    Poll::Pending => done = false,
                }
            }
        }
        if done { Poll::Ready(()) } else { Poll::Pending }
    })
    .await;
    outputs
        .into_iter()
        .map(|o| o.expect("all futures completed"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/aggregator-replies"
    );

    /// Real replies recorded from the four aggregators on 2026-11-20 for one submitted digest.
    /// They start from the submitted bytes, which no commitment can be rebuilt from, so they are
    /// checked as a proof rooted at that digest: parse, re-serialize, merge, write, re-read.
    #[test]
    fn real_aggregator_replies() {
        let digest = std::fs::read(format!("{FIXTURES}/submitted-digest.bin")).unwrap();
        let mut branches = Node::default();
        for host in AGGREGATORS.map(|a| a.trim_start_matches("https://")) {
            let bytes = std::fs::read(format!("{FIXTURES}/{host}.bin")).unwrap();
            let fragment = read_fragment(&bytes, &digest).unwrap();
            assert!(has_pending(&fragment), "{host}");
            let mut rewritten = Vec::new();
            fragment.write(&mut rewritten);
            assert_eq!(
                rewritten, bytes,
                "{host}: reply re-serializes byte for byte"
            );
            branches.merge(fragment);
        }
        let proof = Proof::new(digest.try_into().unwrap(), branches);
        assert_eq!(Proof::from_bytes(&proof.to_bytes()).unwrap(), proof);
        assert_eq!(proof.status(), Status::Pending);

        let calendars = proof.calendars();
        let hosts: Vec<&str> = calendars.iter().map(|c| c.url.as_str()).collect();
        assert_eq!(
            hosts,
            [
                "https://alice.btc.calendar.opentimestamps.org",
                "https://bob.btc.calendar.opentimestamps.org",
                "https://btc.calendar.catallaxy.com",
                "https://finney.calendar.eternitywall.com",
            ],
            "matches the frozen name table"
        );
        assert!(
            calendars.iter().all(|c| c.message.len() == 44),
            "pending messages are 44 bytes"
        );

        let link = crate::link::encode(&calendars).unwrap();
        for name in ["alice", "bob", "catallaxy", "finney"] {
            assert!(link.contains(&format!("cal={name}:")), "{link}");
        }
        assert_eq!(crate::link::decode(&link).unwrap(), calendars);
    }
}
