//! Shared test helpers: a simulated OpenTimestamps network and temporary folders.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use draftstamp_core::{Http, Response};
use sha2::{Digest, Sha256};

const PENDING: [u8; 8] = [0x83, 0xdf, 0xe3, 0x0d, 0x2e, 0xf9, 0x0c, 0x8e];
const BITCOIN: [u8; 8] = [0x05, 0x88, 0x96, 0x0d, 0x73, 0xd7, 0x19, 0x01];
pub const HEIGHT: u64 = 900_000;
pub const TIME: u32 = 1_763_000_000;

/// Aggregators answer with a calendar promise; once `anchored`, the calendar answers with a
/// path to a Bitcoin block whose header both explorers serve.
#[derive(Default)]
pub struct Network {
    pub anchored: bool,
    filed: Mutex<HashMap<String, Vec<u8>>>,
}

fn sha(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
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

impl Network {
    fn header(&self) -> Option<[u8; 80]> {
        let filed = self.filed.lock().unwrap();
        let (_, message) = filed.iter().min_by_key(|(url, _)| (*url).clone())?;
        let mut h = [0u8; 80];
        h[36..68].copy_from_slice(&sha(message));
        h[68..72].copy_from_slice(&TIME.to_le_bytes());
        Some(h)
    }
}

fn block_hash(h: &[u8; 80]) -> String {
    let mut d = sha(&sha(h));
    d.reverse();
    hex(&d)
}

impl Http for Network {
    fn post(&self, url: &str, body: &[u8]) -> Result<Response, opentimestamps_lite::Error> {
        // Two aggregators answer (the minimum), each under its own calendar.
        let calendar = if url.contains("a.pool.opentimestamps.org") {
            "alice.btc.calendar.opentimestamps.org"
        } else if url.contains("b.pool.opentimestamps.org") {
            "bob.btc.calendar.opentimestamps.org"
        } else {
            return Ok(Response {
                status: 503,
                body: vec![],
            });
        };
        let time = [0, 0, 0, 1];
        let mac = [7u8; 8];
        let message = [&time[..], body, &mac].concat();
        let uri = format!("https://{calendar}");
        let mut reply = vec![0xf1, 4];
        reply.extend_from_slice(&time);
        reply.extend_from_slice(&[0xf0, 8]);
        reply.extend_from_slice(&mac);
        reply.push(0x00);
        reply.extend_from_slice(&PENDING);
        reply.extend_from_slice(&[uri.len() as u8 + 1, uri.len() as u8]);
        reply.extend_from_slice(uri.as_bytes());
        self.filed
            .lock()
            .unwrap()
            .insert(format!("{uri}/timestamp/{}", hex(&message)), message);
        Ok(Response {
            status: 200,
            body: reply,
        })
    }

    fn get(&self, url: &str) -> Result<Response, opentimestamps_lite::Error> {
        let not_found = Ok(Response {
            status: 404,
            body: vec![],
        });
        if url.contains("/timestamp/") {
            let first = self.filed.lock().unwrap().keys().min().cloned();
            if !self.anchored || first.as_deref() != Some(url) {
                return not_found;
            }
            let h = varuint(HEIGHT);
            return Ok(Response {
                status: 200,
                body: [&[0x08, 0x00][..], &BITCOIN, &[h.len() as u8], &h].concat(),
            });
        }
        let Some(header) = self.header() else {
            return not_found;
        };
        if url.ends_with(&format!("/block-height/{HEIGHT}")) {
            return Ok(Response {
                status: 200,
                body: block_hash(&header).into_bytes(),
            });
        }
        if url.ends_with(&format!("/block/{}/header", block_hash(&header))) {
            return Ok(Response {
                status: 200,
                body: hex(&header).into_bytes(),
            });
        }
        not_found
    }
}

/// A fresh, empty temporary folder, removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "draftstamp-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A small project folder: two LaTeX files, a figure, and junk that must be skipped.
pub fn project() -> TempDir {
    let d = TempDir::new();
    d.write("main.tex", b"\\documentclass{article}\n");
    d.write("sections/intro.tex", b"\\section{Intro}\n");
    d.write("figures/plot.png", &[0, 159, 255]);
    d.write(".DS_Store", b"junk");
    d.write(".git/HEAD", b"ref: refs/heads/main\n");
    d
}
