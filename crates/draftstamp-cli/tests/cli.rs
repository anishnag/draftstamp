//! Every command, end to end, on temporary folders and vaults with the simulated network.

#[path = "../../draftstamp-core/tests/common/mod.rs"]
mod common;

use std::path::Path;

use common::{HEIGHT, Network, TempDir, project};
use draftstamp_cli::{Context, run};
use draftstamp_core::{LocalVault, Proof, Status, Vault};
use sha2::{Digest, Sha256};

struct Run {
    code: u8,
    out: String,
    err: String,
}

fn ds(net: &Network, cwd: &Path, vault: &Path, args: &[&str]) -> Run {
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
    Run {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

fn ids(vault: &Path) -> Vec<String> {
    let v = LocalVault::open(vault).unwrap();
    v.list().unwrap().iter().map(|i| i.to_string()).collect()
}

#[test]
fn init_stamp_log() {
    let net = Network::default();
    let (folder, vault) = (project(), TempDir::new());
    let (cwd, v) = (folder.path(), vault.path().join("vault"));

    let r = ds(&net, cwd, &v, &["log"]);
    assert!(r.out.contains("No stamps for project"), "{}", r.out);
    assert!(!v.exists(), "read-only commands never create the vault");

    let r = ds(&net, cwd, &v, &["init", "--project", "riemann-draft"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(
        ds(&net, cwd, &v, &["init"]).code,
        1,
        "never overwrites .draftstamp"
    );

    let r = ds(&net, cwd, &v, &["stamp", "-m", "before submission"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out.starts_with("Stamped 3 files of riemann-draft as "),
        "{}",
        r.out
    );
    assert!(r.out.contains("Pending"));

    let r = ds(&net, cwd, &v, &["log"]);
    assert_eq!(r.out.lines().count(), 1, "{}", r.out);
    assert!(
        r.out
            .contains("riemann-draft  3 files  pending  \"before submission\""),
        "{}",
        r.out
    );

    let elsewhere = TempDir::new();
    assert!(
        ds(&net, elsewhere.path(), &v, &["log"])
            .out
            .contains("Use --all")
    );
    assert_eq!(
        ds(&net, elsewhere.path(), &v, &["log", "--all"])
            .out
            .lines()
            .count(),
        1
    );
    assert_eq!(
        ds(
            &net,
            elsewhere.path(),
            &v,
            &["log", "--project", "riemann-draft"]
        )
        .out
        .lines()
        .count(),
        1
    );
}

#[test]
fn project_override_notes_and_single_files() {
    let net = Network::default();
    let (folder, vault) = (project(), TempDir::new());
    let (cwd, v) = (folder.path(), vault.path());
    ds(&net, cwd, v, &["init", "--project", "paper"]);

    let r = ds(&net, cwd, v, &["stamp", "--project", "grant"]);
    assert!(
        r.out.starts_with("Note: tagging this stamp \"grant\""),
        "{}",
        r.out
    );
    assert!(
        std::fs::read_to_string(cwd.join(".draftstamp"))
            .unwrap()
            .contains("\"paper\""),
        "never edited"
    );

    let r = ds(&net, cwd, v, &["stamp", "main.tex"]);
    assert!(
        r.out.starts_with("Stamped 1 file of paper as "),
        "{}",
        r.out
    );
    assert_eq!(ds(&net, cwd, v, &["stamp", "missing.tex"]).code, 1);
}

#[test]
fn upgrade_check_verify() {
    let mut net = Network::default();
    let (folder, vault) = (project(), TempDir::new());
    let (cwd, v) = (folder.path(), vault.path());
    ds(&net, cwd, v, &["stamp"]);

    let r = ds(&net, cwd, v, &["verify"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("Record intact: all 3 files match"));
    assert!(r.out.contains("Bitcoin: pending"));
    assert!(ds(&net, cwd, v, &["upgrade"]).out.contains("still pending"));

    net.anchored = true;
    let r = ds(&net, cwd, v, &["upgrade"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out
            .contains(&format!("confirmed in Bitcoin block {HEIGHT}")),
        "{}",
        r.out
    );
    assert_eq!(
        ds(&net, cwd, v, &["upgrade"]).out.trim(),
        "Nothing pending."
    );

    assert!(
        ds(&net, cwd, v, &["log"])
            .out
            .contains(&format!("in Bitcoin block {HEIGHT}"))
    );
    let r = ds(&net, cwd, v, &["check"]);
    assert_eq!(
        r.out.trim(),
        "Record intact: all 3 files match the receipt, and the proof is for this stamp."
    );
    let r = ds(&net, cwd, v, &["verify"]);
    assert!(
        r.out.contains(&format!(
            "Bitcoin: confirmed in block {HEIGHT}. These files existed by 2025-11-13"
        )),
        "{}",
        r.out
    );
}

#[test]
fn a_tampered_stamp_exits_3() {
    let net = Network::default();
    let (folder, vault) = (project(), TempDir::new());
    let (cwd, v) = (folder.path(), vault.path());
    ds(&net, cwd, v, &["stamp"]);
    let id = ids(v).remove(0);
    let receipt = v.join(&id).join("receipt.json");
    let text = std::fs::read_to_string(&receipt).unwrap();
    let edited = text.replacen("\"size\": 3", "\"size\": 4", 1);
    assert_ne!(text, edited);
    std::fs::write(&receipt, edited).unwrap();
    let r = ds(&net, cwd, v, &["check"]);
    assert_eq!(r.code, 3, "{} {}", r.out, r.err);
    assert_eq!(ds(&net, cwd, v, &["verify"]).code, 3);
}

#[test]
fn stamp_references() {
    let net = Network::default();
    let (folder, vault) = (project(), TempDir::new());
    let (cwd, v) = (folder.path(), vault.path());
    ds(&net, cwd, v, &["stamp"]);
    ds(&net, cwd, v, &["stamp"]);
    let all = ids(v);
    let suffix = all[0].rsplit('-').next().unwrap();
    assert_eq!(ds(&net, cwd, v, &["check", &all[1]]).code, 0, "full id");
    assert_eq!(
        ds(&net, cwd, v, &["check", suffix]).code,
        0,
        "6-character ending"
    );
    let r = ds(&net, cwd, v, &["check", "2"]);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("matches several stamps"), "{}", r.err);
    assert!(
        ds(&net, cwd, v, &["check", "zzz"])
            .err
            .contains("No stamp matches")
    );
}

#[test]
fn share_before_and_after_upgrading() {
    let mut net = Network::default();
    let (folder, vault, out) = (project(), TempDir::new(), TempDir::new());
    let (cwd, v) = (folder.path(), vault.path());
    ds(&net, cwd, v, &["stamp"]);

    // Pending: the file and its .ots, written elsewhere.
    let r = ds(
        &net,
        cwd,
        v,
        &[
            "share",
            "sections/intro.tex",
            "-o",
            out.path().to_str().unwrap(),
        ],
    );
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out.contains("Wrote intro.tex and intro.tex.ots"),
        "{}",
        r.out
    );
    assert!(r.out.contains("still pending"));
    assert!(r.out.contains("ots verify -f intro.tex intro.tex.ots"));

    // Confirmed: share upgrades the stored proof first. main.tex is already here, unchanged,
    // so only its .ots is written.
    net.anchored = true;
    let r = ds(&net, cwd, v, &["share", "main.tex"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out.starts_with("Wrote main.tex.ots from stamp"),
        "{}",
        r.out
    );
    assert!(!r.out.contains("pending"));
    assert!(
        ds(&net, cwd, v, &["log"]).out.contains("in Bitcoin block"),
        "upgrade was saved"
    );

    let file = std::fs::read(cwd.join("main.tex")).unwrap();
    let mut proof = Proof::from_bytes(&std::fs::read(cwd.join("main.tex.ots")).unwrap()).unwrap();
    assert_eq!(proof.digest(), <[u8; 32]>::from(Sha256::digest(&file)));
    assert!(matches!(
        opentimestamps_lite::upgrade(&net, &mut proof).unwrap(),
        Status::Anchored { .. }
    ));

    assert_eq!(
        ds(&net, cwd, v, &["share", "main.tex"]).code,
        1,
        "never overwrites a .ots"
    );
    assert_eq!(ds(&net, cwd, v, &["share", "nope.tex"]).code, 1);
}

#[test]
fn export_and_find() {
    let net = Network::default();
    let (folder, vault) = (project(), TempDir::new());
    let (cwd, v) = (folder.path(), vault.path());
    ds(&net, cwd, v, &["stamp"]);

    let r = ds(&net, cwd, v, &["export", "-o", "evidence.zip"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("3 files, each with its own .ots"));
    assert!(cwd.join("evidence.zip").is_file());
    assert_eq!(
        ds(&net, cwd, v, &["export", "-o", "evidence.zip"]).code,
        1,
        "never overwrites"
    );

    let r = ds(&net, cwd, v, &["find", "sections/intro.tex"]);
    assert!(r.out.contains("  sections/intro.tex  "), "{}", r.out);
    folder.write("new.tex", b"never stamped");
    assert_eq!(
        ds(&net, cwd, v, &["find", "new.tex"]).out.trim(),
        "No stamp contains this exact file."
    );
}

#[test]
fn usage() {
    let net = Network::default();
    let dir = TempDir::new();
    assert_eq!(ds(&net, dir.path(), dir.path(), &["bogus"]).code, 2);
    let r = ds(&net, dir.path(), dir.path(), &["--help"]);
    assert_eq!(r.code, 0);
    assert!(r.out.contains("share"));
    let r = ds(&net, dir.path(), &dir.path().join("missing"), &["check"]);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("No vault"), "{}", r.err);
}
