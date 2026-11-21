//! Runs tools/verify.py, an independent stdlib-only Python verifier, against bundles written by
//! this crate. Skipped when python3 is not installed.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::{Network, TempDir, project};
use draftstamp_core::{Bundle, LocalVault, StampOptions, Vault};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/verify.py");

fn python_verifies(bundle: &Path) -> Option<bool> {
    let out = Command::new("python3")
        .arg("-I")
        .arg(SCRIPT)
        .arg(bundle)
        .output()
        .ok()?;
    Some(out.status.success())
}

#[test]
fn python_verifier_agrees() {
    let net = Network::default();
    let dir = TempDir::new();
    let vault = LocalVault::open(dir.path()).unwrap();
    let bundle = Bundle::create(&net, project().path(), &StampOptions::default()).unwrap();
    let id = vault.put(&bundle).unwrap();
    let stamp_dir = dir.path().join(id.as_str());

    let Some(ok) = python_verifies(&stamp_dir) else {
        eprintln!("python3 not found; skipping");
        return;
    };
    assert!(ok, "a stored bundle verifies");

    let export = dir.path().join("evidence.zip");
    bundle.export(&export).unwrap();
    assert_eq!(
        python_verifies(&export),
        Some(true),
        "an exported zip verifies"
    );

    let receipt = fs::read_to_string(stamp_dir.join("receipt.json")).unwrap();
    let salt = bundle.receipt().committed().leaves()[0].salt.to_string();
    fs::write(
        stamp_dir.join("receipt.json"),
        receipt.replacen(&salt, &"0".repeat(32), 1),
    )
    .unwrap();
    assert_eq!(
        python_verifies(&stamp_dir),
        Some(false),
        "a tampered receipt fails"
    );
}
