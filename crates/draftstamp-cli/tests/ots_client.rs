//! Our per-file proofs, checked by the official OpenTimestamps client (offline).
//!
//! Opt-in: `cargo test -p draftstamp-cli --test ots_client -- --ignored`. Needs `ots` on PATH
//! (`pipx install opentimestamps-client`) or `OTS=/path/to/ots`. The client runs with no
//! calendars, no cache and no Bitcoin node, so nothing leaves the machine.

#[path = "../../draftstamp-core/tests/common/mod.rs"]
mod common;

use std::path::Path;
use std::process::Command;

use common::{HEIGHT, Network, TempDir, project};
use draftstamp_cli::{Context, run};

const OFFLINE: [&str; 4] = ["--no-cache", "--no-default-whitelist", "--no-bitcoin", "-v"];

fn ds(net: &Network, cwd: &Path, vault: &Path, args: &[&str]) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut ctx = Context {
        cwd: cwd.to_path_buf(),
        default_vault: vault.to_path_buf(),
        out: &mut out,
        err: &mut err,
    };
    let code = run(
        std::iter::once("draftstamp").chain(args.iter().copied()),
        net,
        &mut ctx,
    );
    assert_eq!(
        code,
        0,
        "draftstamp {args:?}: {}",
        String::from_utf8_lossy(&err)
    );
}

/// Runs the official client; returns (success, everything it printed).
fn ots(args: &[&str]) -> (bool, String) {
    let bin = std::env::var("OTS").unwrap_or_else(|_| "ots".into());
    let out = Command::new(&bin)
        .args(OFFLINE)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {bin} ({e}); pipx install opentimestamps-client"));
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text)
}

fn hex_reversed(b: &[u8]) -> String {
    b.iter().rev().map(|x| format!("{x:02x}")).collect()
}

const FILES: [(&str, &str); 3] = [
    ("main.tex", "main.tex"),
    ("sections/intro.tex", "intro.tex"),
    ("figures/plot.png", "plot.png"),
];

#[test]
#[ignore = "needs the official OpenTimestamps client: pipx install opentimestamps-client"]
fn official_client_reads_and_follows_per_file_proofs() {
    let mut net = Network::default();
    let (folder, vault) = (project(), TempDir::new());
    let (cwd, v) = (folder.path(), vault.path());
    ds(&net, cwd, v, &["stamp"]);

    // Pending: the client parses each proof and accepts the file's hash.
    let pending = TempDir::new();
    for (path, name) in FILES {
        ds(
            &net,
            cwd,
            v,
            &["share", path, "-o", pending.path().to_str().unwrap()],
        );
        let ots_file = pending.path().join(format!("{name}.ots"));
        let (ok, text) = ots(&["info", ots_file.to_str().unwrap()]);
        assert!(ok, "ots info {name}: {text}");
        assert!(text.contains("PendingAttestation"), "{text}");
        let (_, text) = ots(&[
            "verify",
            "-f",
            pending.path().join(name).to_str().unwrap(),
            ots_file.to_str().unwrap(),
        ]);
        assert!(
            !text.contains("does not match") && !text.contains("Invalid timestamp"),
            "{name}: {text}"
        );
    }

    // Confirmed: walking our steps from each file, the client reaches the block's merkle root.
    net.anchored = true;
    ds(&net, cwd, v, &["upgrade"]);
    let expected = format!(
        "check that Bitcoin block {HEIGHT} has merkleroot {}",
        hex_reversed(&net.merkle_root().unwrap())
    );
    let confirmed = TempDir::new();
    for (path, name) in FILES {
        ds(
            &net,
            cwd,
            v,
            &["share", path, "-o", confirmed.path().to_str().unwrap()],
        );
        let (file, ots_file) = (
            confirmed.path().join(name),
            confirmed.path().join(format!("{name}.ots")),
        );
        let (_, text) = ots(&[
            "verify",
            "-f",
            file.to_str().unwrap(),
            ots_file.to_str().unwrap(),
        ]);
        assert!(
            text.contains(&expected),
            "{name}: expected {expected:?} in\n{text}"
        );
    }

    // A changed file is rejected by the client.
    let file = confirmed.path().join("main.tex");
    std::fs::write(&file, b"edited").unwrap();
    let (ok, text) = ots(&[
        "verify",
        "-f",
        file.to_str().unwrap(),
        confirmed.path().join("main.tex.ots").to_str().unwrap(),
    ]);
    assert!(
        !ok && text.contains("File does not match original!"),
        "{text}"
    );
}
