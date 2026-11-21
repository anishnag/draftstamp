//! Bundle: create, upgrade, verify, export, and tampering.

mod common;

use std::io::Cursor;

use common::{HEIGHT, Network, TIME, TempDir, project};
use draftstamp_core::{Bundle, Error, StampOptions, Status};

#[test]
fn create_upgrade_verify() {
    let mut net = Network::default();
    let folder = project();
    let opts = StampOptions {
        label: Some("first draft".into()),
        project: None,
    };
    let mut bundle = Bundle::create(&net, folder.path(), &opts).unwrap();

    let receipt = bundle.receipt();
    let paths: Vec<&str> = receipt
        .committed()
        .leaves()
        .iter()
        .map(|l| l.path.as_str())
        .collect();
    assert_eq!(
        paths,
        ["figures/plot.png", "main.tex", "sections/intro.tex"],
        "junk skipped, sorted"
    );
    assert_eq!(receipt.advisory().label.as_deref(), Some("first draft"));
    let folder_name = folder.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(
        receipt.advisory().project.as_deref(),
        Some(folder_name),
        "project defaults to the folder name"
    );
    assert_eq!(receipt.advisory().id.as_deref(), Some(bundle.id().as_str()));
    assert_eq!(bundle.proof().digest(), receipt.commitment().0);

    // Pending: the files verify, the proof is not yet in Bitcoin.
    assert_eq!(bundle.verify(&net).unwrap().status, Status::Pending);

    // Anchored: verify reports it without changing the bundle; upgrade stores it.
    net.anchored = true;
    let anchored = Status::Anchored {
        height: HEIGHT,
        time: TIME,
    };
    let v = bundle.verify(&net).unwrap();
    assert_eq!((v.files, v.status), (3, anchored));
    assert_eq!(
        bundle.proof().status(),
        Status::Pending,
        "verify does not modify the bundle"
    );
    assert_eq!(bundle.upgrade(&net).unwrap(), anchored);
    assert_eq!(bundle.proof().status(), Status::Claimed { height: HEIGHT });
}

#[test]
fn empty_folder_is_an_error() {
    let net = Network::default();
    let folder = TempDir::new();
    folder.write(".DS_Store", b"junk");
    assert!(matches!(
        Bundle::create(&net, folder.path(), &StampOptions::default()),
        Err(Error::EmptyFolder)
    ));
}

#[test]
fn export_holds_the_three_files() {
    let net = Network::default();
    let bundle = Bundle::create(&net, project().path(), &StampOptions::default()).unwrap();
    let out = TempDir::new();
    let zip_path = out.path().join("evidence.zip");
    bundle.export(&zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(std::fs::read(&zip_path).unwrap())).unwrap();
    let mut names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    names.sort();
    let id = bundle.id();
    assert_eq!(
        names,
        [
            format!("{id}/proof.ots"),
            format!("{id}/receipt.json"),
            format!("{id}/snapshot.zip"),
        ]
    );
}
