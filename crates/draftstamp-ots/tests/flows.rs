//! stamp, upgrade and link::status against a simulated network of aggregators, calendars and explorers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use draftstamp_ots::{Error, Http, Proof, Response, Status, link, stamp, upgrade};
use sha2::{Digest, Sha256};

const CALENDARS: [&str; 4] = [
    "alice.btc.calendar.opentimestamps.org",
    "bob.btc.calendar.opentimestamps.org",
    "finney.calendar.eternitywall.com",
    "btc.calendar.catallaxy.com",
];
const PENDING: [u8; 8] = [0x83, 0xdf, 0xe3, 0x0d, 0x2e, 0xf9, 0x0c, 0x8e];
const BITCOIN: [u8; 8] = [0x05, 0x88, 0x96, 0x0d, 0x73, 0xd7, 0x19, 0x01];
const BASE_HEIGHT: u64 = 900_000;
const BASE_TIME: u32 = 1_763_000_000;

#[derive(Clone, Copy, PartialEq)]
enum Explorers {
    Agree,
    Disagree,
    Down,
    WrongRoot,
}

struct Network {
    aggregators_up: usize,
    garbage_from_first: bool,
    anchored: bool,
    explorers: Explorers,
    /// timestamp URL → calendar index and message, filled in as stamps are submitted
    filed: RefCell<HashMap<String, (usize, Vec<u8>)>>,
}

impl Network {
    fn new() -> Self {
        Network {
            aggregators_up: 4,
            garbage_from_first: false,
            anchored: false,
            explorers: Explorers::Agree,
            filed: RefCell::default(),
        }
    }

    fn header(&self, height: u64, explorer: usize) -> Option<[u8; 80]> {
        let i = (height.checked_sub(BASE_HEIGHT)?) as usize;
        let message = self
            .filed
            .borrow()
            .values()
            .find(|(c, _)| *c == i)?
            .1
            .clone();
        let mut h = [0u8; 80];
        h[0] = 4;
        if self.explorers != Explorers::WrongRoot {
            h[36..68].copy_from_slice(&sha(&message));
        }
        let skew = if self.explorers == Explorers::Disagree {
            explorer as u32
        } else {
            0
        };
        h[68..72].copy_from_slice(&(BASE_TIME + i as u32 + skew).to_le_bytes());
        Some(h)
    }
}

fn sha(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn block_hash(h: &[u8; 80]) -> String {
    let mut d = sha(&sha(h));
    d.reverse();
    hex(&d)
}

fn ok(body: Vec<u8>) -> Result<Response, Error> {
    Ok(Response { status: 200, body })
}

fn status(code: u16) -> Result<Response, Error> {
    Ok(Response {
        status: code,
        body: Vec::new(),
    })
}

impl Http for Network {
    async fn post(&self, url: &str, body: &[u8]) -> Result<Response, Error> {
        let i = [
            "a.pool.opentimestamps.org",
            "b.pool.opentimestamps.org",
            "a.pool.eternitywall.com",
            "ots.btc.catallaxy.com",
        ]
        .iter()
        .position(|a| url == format!("https://{a}/digest"))
        .expect("known aggregator");
        if i >= self.aggregators_up {
            return status(503);
        }
        if i == 0 && self.garbage_from_first {
            return ok(vec![0xf0, 0xff, 0xff]);
        }
        // Calendar steps: prepend 4-byte time, append 8-byte MAC, then the pending attestation.
        let time = [0, 0, 0, i as u8];
        let mac = [i as u8; 8];
        let message = [&time[..], body, &mac].concat();
        let uri = format!("https://{}", CALENDARS[i]);
        let mut reply = vec![0xf1, 4];
        reply.extend_from_slice(&time);
        reply.extend_from_slice(&[0xf0, 8]);
        reply.extend_from_slice(&mac);
        reply.push(0x00);
        reply.extend_from_slice(&PENDING);
        reply.extend_from_slice(&[uri.len() as u8 + 1, uri.len() as u8]);
        reply.extend_from_slice(uri.as_bytes());
        self.filed
            .borrow_mut()
            .insert(format!("{uri}/timestamp/{}", hex(&message)), (i, message));
        ok(reply)
    }

    async fn get(&self, url: &str) -> Result<Response, Error> {
        if url.contains("/timestamp/") {
            let filed = self.filed.borrow();
            let Some((i, _)) = filed.get(url) else {
                return status(404);
            };
            if !self.anchored {
                return status(404);
            }
            // sha256 of the message, then the Bitcoin attestation at this calendar's block.
            let height = varuint(BASE_HEIGHT + *i as u64);
            return ok([&[0x08, 0x00][..], &BITCOIN, &[height.len() as u8], &height].concat());
        }
        let explorer = if url.starts_with("https://blockstream.info/api") {
            0
        } else {
            1
        };
        if self.explorers == Explorers::Down {
            return status(503);
        }
        if let Some(h) = url.split("/block-height/").nth(1) {
            let header = self.header(h.parse().unwrap(), explorer);
            return header.map_or(status(404), |hd| ok(block_hash(&hd).into_bytes()));
        }
        if let Some(rest) = url.split("/block/").nth(1) {
            let hash = rest.trim_end_matches("/header");
            for i in 0..4u64 {
                if let Some(hd) = self.header(BASE_HEIGHT + i, explorer)
                    && block_hash(&hd) == hash
                {
                    return ok(hex(&hd).into_bytes());
                }
            }
        }
        status(404)
    }
}

fn varuint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return out;
        }
        out.push(b | 0x80);
    }
}

fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

#[test]
fn full_lifecycle() {
    let mut net = Network::new();
    let commitment = [42u8; 32];

    let mut proof = block_on(stamp(&net, commitment)).unwrap();
    assert_eq!(proof.commitment(), commitment);
    assert_eq!(proof.status(), Status::Pending);
    assert_eq!(proof.calendars().len(), 4);
    assert!(proof.calendars().iter().all(|c| c.message.len() == 44));
    assert_eq!(Proof::from_bytes(&proof.to_bytes()).unwrap(), proof);

    let link_before = link::encode(&proof.calendars()).unwrap();
    assert_eq!(
        block_on(link::status(&net, &link_before)).unwrap(),
        Status::Pending
    );

    // Calendars not anchored yet: nothing changes.
    let before = proof.clone();
    assert_eq!(
        block_on(upgrade(&net, &mut proof)).unwrap(),
        Status::Pending
    );
    assert_eq!(proof, before);

    // Anchored: upgrade adds the Bitcoin paths and verifies from the commitment.
    net.anchored = true;
    let anchored = Status::Anchored {
        height: BASE_HEIGHT,
        time: BASE_TIME,
    };
    assert_eq!(block_on(upgrade(&net, &mut proof)).unwrap(), anchored);
    assert_eq!(
        proof.status(),
        Status::Claimed {
            height: BASE_HEIGHT
        },
        "offline status does not verify"
    );
    assert_eq!(Proof::from_bytes(&proof.to_bytes()).unwrap(), proof);

    // The link is unchanged by the upgrade and now reports anchored.
    assert_eq!(link::encode(&proof.calendars()).unwrap(), link_before);
    assert_eq!(
        block_on(link::status(&net, &link_before)).unwrap(),
        anchored
    );

    // Upgrading again adds nothing.
    let done = proof.clone();
    assert_eq!(block_on(upgrade(&net, &mut proof)).unwrap(), anchored);
    assert_eq!(proof, done);
}

#[test]
fn needs_two_aggregators() {
    let mut net = Network::new();
    net.aggregators_up = 1;
    assert_eq!(
        block_on(stamp(&net, [1; 32])),
        Err(Error::TooFewCalendars { accepted: 1 })
    );
    net.aggregators_up = 2;
    assert_eq!(block_on(stamp(&net, [1; 32])).unwrap().calendars().len(), 2);
}

#[test]
fn malformed_replies_are_ignored() {
    let mut net = Network::new();
    net.garbage_from_first = true;
    let proof = block_on(stamp(&net, [2; 32])).unwrap();
    assert_eq!(proof.calendars().len(), 3);
}

#[test]
fn explorer_problems() {
    for (mode, expected) in [
        (
            Explorers::Disagree,
            Ok(Status::Claimed {
                height: BASE_HEIGHT,
            }),
        ),
        (
            Explorers::Down,
            Ok(Status::Claimed {
                height: BASE_HEIGHT,
            }),
        ),
        (Explorers::WrongRoot, Err(Error::HeaderMismatch)),
    ] {
        let mut net = Network::new();
        let mut proof = block_on(stamp(&net, [3; 32])).unwrap();
        net.anchored = true;
        net.explorers = mode;
        assert_eq!(block_on(upgrade(&net, &mut proof)), expected);
    }
}
