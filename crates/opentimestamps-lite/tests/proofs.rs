//! Proofs built from real aggregator replies: round trips and hostile input.

use opentimestamps_lite::{Error, Http, Proof, Response, Status, stamp};

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/aggregator-replies"
);

/// Serves the replies recorded from the four real aggregators. A reply is a list of operations
/// plus a pending attestation, so it attaches to whatever digest was submitted.
struct Recorded;

impl Http for Recorded {
    fn get(&self, _: &str) -> Result<Response, Error> {
        Ok(Response {
            status: 404,
            body: Vec::new(),
        })
    }
    fn post(&self, url: &str, _: &[u8]) -> Result<Response, Error> {
        let host = url
            .trim_start_matches("https://")
            .trim_end_matches("/digest");
        let body = std::fs::read(format!("{FIXTURES}/{host}.bin"))
            .map_err(|e| Error::Http(e.to_string()))?;
        Ok(Response { status: 200, body })
    }
}

fn real_proof() -> Proof {
    stamp(&Recorded, [0x5a; 32]).unwrap()
}

#[test]
fn round_trip_and_status() {
    let proof = real_proof();
    let bytes = proof.to_bytes();
    assert_eq!(
        &bytes[..31],
        b"\x00OpenTimestamps\x00\x00Proof\x00\xbf\x89\xe2\xe8\x84\xe8\x92\x94"
    );
    assert_eq!(&bytes[31..33], &[0x01, 0x08]);
    assert_eq!(&bytes[33..65], &[0x5a; 32]);
    let again = Proof::from_bytes(&bytes).unwrap();
    assert_eq!(again, proof);
    assert_eq!(again.to_bytes(), bytes);
    assert_eq!(proof.status(), Status::Pending);
}

#[test]
fn hostile_input_never_panics() {
    let bytes = real_proof().to_bytes();
    for len in 0..bytes.len() {
        assert!(
            Proof::from_bytes(&bytes[..len]).is_err(),
            "truncated to {len}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        Proof::from_bytes(&trailing),
        Err(Error::Malformed("trailing bytes"))
    );
    for i in 0..bytes.len() {
        for flip in [0x01, 0x80, 0xff] {
            let mut b = bytes.clone();
            b[i] ^= flip;
            let _ = Proof::from_bytes(&b); // must return, not panic
        }
    }
}
