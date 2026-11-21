//! End to end against the real OpenTimestamps calendars and block explorers.
//!
//! Opt-in, since it contacts public servers:
//!
//! ```text
//! cargo test -p draftstamp-cli --test live -- --ignored --nocapture
//! ```
//!
//! Needs the official client (`pipx install opentimestamps-client`, or `OTS=/path/to/ots`).
//!
//! 1. Stamps a throwaway folder through the real calendars, then checks, verifies (pending) and
//!    shares a file. The official client follows the shared `.ots` through our steps and asks the
//!    real calendars for the result: they must answer that it is pending in Bitcoin, which means
//!    they recognise the value our steps lead to.
//! 2. Bitcoin takes hours. Set `DRAFTSTAMP_LIVE_VAULT=<dir>` to keep the vault; a later run
//!    upgrades every stamp in it, and for each confirmed one checks with our `verify`, then has the
//!    official client walk every shared file's `.ots` to a merkle root that must equal the real
//!    block's, fetched from an explorer.

use std::path::{Path, PathBuf};
use std::process::Command;

use draftstamp_cli::http::Native;
use draftstamp_cli::{Context, run};
use draftstamp_core::{Http, LocalVault, Status, Vault};

struct Run {
    code: u8,
    out: String,
    err: String,
}

fn ds(cwd: &Path, vault: &Path, args: &[&str]) -> Run {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut ctx = Context {
        cwd: cwd.to_path_buf(),
        default_vault: vault.to_path_buf(),
        out: &mut out,
        err: &mut err,
    };
    let code = run(
        std::iter::once("draftstamp").chain(args.iter().copied()),
        &Native::new(),
        &mut ctx,
    );
    let r = Run {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    };
    eprintln!("$ draftstamp {}\n{}{}", args.join(" "), r.out, r.err);
    r
}

/// The official client with Bitcoin disabled (no node needed) and no cache.
fn ots(args: &[&str]) -> String {
    let bin = std::env::var("OTS").unwrap_or_else(|_| "ots".into());
    let out = Command::new(&bin)
        .args(["--no-cache", "--no-bitcoin", "-v"])
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {bin} ({e}); pipx install opentimestamps-client"));
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    eprintln!("$ ots {}\n{text}", args.join(" "));
    text
}

fn scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("draftstamp-live-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// The block's merkle root as the client prints it (reversed hex), from an explorer.
fn merkle_root(height: u64) -> String {
    let http = Native::new();
    let api = "https://blockstream.info/api";
    let hash = http.get(&format!("{api}/block-height/{height}")).unwrap();
    let hash = String::from_utf8(hash.body).unwrap();
    let header = http
        .get(&format!("{api}/block/{}/header", hash.trim()))
        .unwrap();
    let header = String::from_utf8(header.body).unwrap();
    let root = &header.trim()[72..136];
    (0..32).rev().map(|i| &root[2 * i..2 * i + 2]).collect()
}

#[test]
#[ignore = "contacts the public OpenTimestamps calendars and block explorers"]
fn live_end_to_end() {
    let kept = std::env::var_os("DRAFTSTAMP_LIVE_VAULT").map(PathBuf::from);
    let vault = kept.clone().unwrap_or_else(|| scratch("vault"));

    // 2. Stamps from earlier runs: upgrade, and check the confirmed ones all the way to Bitcoin.
    if vault.is_dir() {
        let stored = LocalVault::open(&vault).unwrap();
        for id in stored.list().unwrap() {
            let id = id.to_string();
            let anywhere = scratch("resume");
            assert_ne!(ds(&anywhere, &vault, &["upgrade", &id]).code, 3);
            // A stored proof's offline status: Pending, or Claimed once it names a block.
            let Status::Claimed { height } = stored.proof(&id.parse().unwrap()).unwrap().status()
            else {
                eprintln!("{id}: not confirmed yet; run again later.");
                continue;
            };
            let r = ds(&anywhere, &vault, &["verify", &id]);
            assert_eq!(r.code, 0, "{}", r.err);
            assert!(
                r.out
                    .contains(&format!("Bitcoin: confirmed in block {height}.")),
                "{}",
                r.out
            );

            let root = merkle_root(height);
            let expected = format!("check that Bitcoin block {height} has merkleroot {root}");
            let receipt = stored.receipt(&id.parse().unwrap()).unwrap();
            for leaf in receipt.committed().leaves() {
                let out = scratch("share");
                let r = ds(
                    &anywhere,
                    &vault,
                    &[
                        "share",
                        &leaf.path,
                        "--stamp",
                        &id,
                        "-o",
                        out.to_str().unwrap(),
                    ],
                );
                assert_eq!(r.code, 0, "{}", r.err);
                let name = leaf.path.rsplit('/').next().unwrap();
                let file = out.join(name);
                let proof = out.join(format!("{name}.ots"));
                let text = ots(&[
                    "verify",
                    "-f",
                    file.to_str().unwrap(),
                    proof.to_str().unwrap(),
                ]);
                assert!(
                    text.contains(&expected),
                    "{}: expected {expected:?}",
                    leaf.path
                );
                let _ = std::fs::remove_dir_all(out);
            }
            eprintln!("{id}: every file's .ots reaches the merkle root of Bitcoin block {height}.");
        }
    }

    // 1. A new stamp through the real calendars.
    let folder = scratch("project");
    let unique = format!("draftstamp live test {:?}\n", std::time::SystemTime::now());
    std::fs::write(folder.join("main.tex"), unique.as_bytes()).unwrap();
    std::fs::create_dir_all(folder.join("sections")).unwrap();
    std::fs::write(folder.join("sections/intro.tex"), b"\\section{Intro}\n").unwrap();

    let r = ds(&folder, &vault, &["stamp", "-m", "live test"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("Pending"));
    assert_eq!(ds(&folder, &vault, &["check"]).code, 0);
    let r = ds(&folder, &vault, &["verify"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("Bitcoin: pending"), "{}", r.out);

    let out = scratch("shared");
    let r = ds(
        &folder,
        &vault,
        &["share", "sections/intro.tex", "-o", out.to_str().unwrap()],
    );
    assert_eq!(r.code, 0, "{}", r.err);
    let text = ots(&[
        "verify",
        "-f",
        out.join("intro.tex").to_str().unwrap(),
        out.join("intro.tex.ots").to_str().unwrap(),
    ]);
    assert!(!text.contains("File does not match original"), "{text}");
    assert!(
        text.contains("Pending confirmation in Bitcoin blockchain"),
        "the real calendars must recognise the value our steps lead to:\n{text}"
    );

    for dir in [folder, out] {
        let _ = std::fs::remove_dir_all(dir);
    }
    match kept {
        Some(v) => eprintln!(
            "Kept the vault at {}; run again in a few hours to check Bitcoin.",
            v.display()
        ),
        None => {
            let _ = std::fs::remove_dir_all(&vault);
            eprintln!(
                "Set DRAFTSTAMP_LIVE_VAULT=<dir> to keep the stamp and check Bitcoin on a later run."
            );
        }
    }
}
