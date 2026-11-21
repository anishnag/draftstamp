//! LocalVault storage rules, tampering, and history/find.

mod common;

use std::fs;

use common::{HEIGHT, Network, TempDir, project};
use draftstamp_core::{Bundle, Error, LocalVault, StampOptions, Status, Vault, find, history};

fn stamped(net: &Network, project_tag: &str) -> (TempDir, Bundle) {
    let folder = project();
    let opts = StampOptions {
        label: None,
        project: Some(project_tag.into()),
    };
    let bundle = Bundle::create(net, folder.path(), &opts).unwrap();
    (folder, bundle)
}

#[test]
fn put_get_list() {
    let net = Network::default();
    let dir = TempDir::new();
    let vault = LocalVault::open(dir.path()).unwrap();
    let (_f, bundle) = stamped(&net, "paper");
    let id = vault.put(&bundle).unwrap();
    assert_eq!(vault.list().unwrap(), vec![id.clone()]);
    let back = vault.get(&id).unwrap();
    assert_eq!(back.receipt(), bundle.receipt());
    assert_eq!(back.proof(), bundle.proof());
    assert_eq!(back.verify(&net).unwrap().files, 3);
    assert!(
        matches!(vault.put(&bundle), Err(Error::Exists(_))),
        "never overwritten"
    );

    // A leftover from an interrupted write is ignored.
    fs::create_dir(dir.path().join(format!(".tmp-{id}"))).unwrap();
    assert_eq!(vault.list().unwrap(), vec![id]);
}

#[test]
fn put_proof_only_accepts_the_same_commitment() {
    let mut net = Network::default();
    let dir = TempDir::new();
    let vault = LocalVault::open(dir.path()).unwrap();
    let (_f, bundle) = stamped(&net, "paper");
    let id = vault.put(&bundle).unwrap();

    net.anchored = true;
    let mut upgraded = vault.get(&id).unwrap();
    upgraded.upgrade(&net).unwrap();
    vault.put_proof(&id, upgraded.proof()).unwrap();
    assert_eq!(
        vault.proof(&id).unwrap().status(),
        Status::Claimed { height: HEIGHT }
    );

    let (_g, other) = stamped(&Network::default(), "other");
    assert!(matches!(
        vault.put_proof(&id, other.proof()),
        Err(Error::Mismatch(_))
    ));
}

#[test]
fn tampered_bundles_fail() {
    let net = Network::default();
    let dir = TempDir::new();
    let vault = LocalVault::open(dir.path()).unwrap();
    let (_f, bundle) = stamped(&net, "paper");
    let id = vault.put(&bundle).unwrap();
    let stamp_dir = dir.path().join(id.as_str());

    // Edited receipt: the root no longer matches the leaves.
    let receipt = fs::read_to_string(stamp_dir.join("receipt.json")).unwrap();
    let first_salt = &bundle.receipt().committed().leaves()[0].salt.to_string();
    fs::write(
        stamp_dir.join("receipt.json"),
        receipt.replacen(first_salt.as_str(), &"0".repeat(32), 1),
    )
    .unwrap();
    assert!(matches!(vault.get(&id), Err(Error::InvalidReceipt(_))));
    fs::write(stamp_dir.join("receipt.json"), &receipt).unwrap();

    // Snapshot from a stamp with an edited main.tex: files no longer match the receipt.
    let snapshot = fs::read(stamp_dir.join("snapshot.zip")).unwrap();
    let other_dir = TempDir::new();
    let other_vault = LocalVault::open(other_dir.path()).unwrap();
    let edited = TempDir::new();
    edited.write("main.tex", b"edited");
    edited.write("sections/intro.tex", b"\\section{Intro}\n");
    edited.write("figures/plot.png", &[0, 159, 255]);
    let edited_bundle = Bundle::create(&net, edited.path(), &StampOptions::default()).unwrap();
    let edited_id = other_vault.put(&edited_bundle).unwrap();
    fs::copy(
        other_dir
            .path()
            .join(edited_id.as_str())
            .join("snapshot.zip"),
        stamp_dir.join("snapshot.zip"),
    )
    .unwrap();
    assert!(matches!(
        vault.get(&id).unwrap().verify(&net),
        Err(Error::Mismatch(_))
    ));
    fs::write(stamp_dir.join("snapshot.zip"), &snapshot).unwrap();

    // A proof for a different commitment.
    fs::write(
        stamp_dir.join("proof.ots"),
        edited_bundle.proof().to_bytes(),
    )
    .unwrap();
    assert!(matches!(
        vault.get(&id).unwrap().verify(&net),
        Err(Error::Mismatch(_))
    ));
}

#[test]
fn history_and_find() {
    let net = Network::default();
    let dir = TempDir::new();
    let vault = LocalVault::open(dir.path()).unwrap();
    let (paper, a) = stamped(&net, "paper");
    let (_g, b) = stamped(&net, "grant");
    // Fresh salts make every commitment, and so every id, distinct.
    vault.put(&a).unwrap();
    vault.put(&b).unwrap();
    assert_eq!(history(&vault, None).unwrap().len(), 2);
    let papers = history(&vault, Some("paper")).unwrap();
    assert_eq!(papers.len(), 1);
    assert_eq!(papers[0].id, *a.id());
    assert_eq!(papers[0].files, 3);

    let found = find(&vault, &paper.path().join("main.tex")).unwrap();
    assert_eq!(found.len(), 2, "both stamps contain the same main.tex");
    assert!(found.iter().all(|f| f.path == "main.tex"));
    let unknown = TempDir::new();
    unknown.write("x.tex", b"never stamped");
    assert!(
        find(&vault, &unknown.path().join("x.tex"))
            .unwrap()
            .is_empty()
    );
}
