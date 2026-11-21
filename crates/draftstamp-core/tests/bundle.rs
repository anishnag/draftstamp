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
fn export_holds_the_bundle_and_every_file_with_its_proof() {
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
    let expected: Vec<String> = [
        "README.txt",
        "files/figures/plot.png",
        "files/figures/plot.png.ots",
        "files/main.tex",
        "files/main.tex.ots",
        "files/sections/intro.tex",
        "files/sections/intro.tex.ots",
        "proof.ots",
        "receipt.json",
        "snapshot.zip",
    ]
    .iter()
    .map(|n| format!("{id}/{n}"))
    .collect();
    assert_eq!(names, expected);
}

#[test]
fn every_shared_file_proves_itself_to_bitcoin() {
    use sha2::{Digest, Sha256};

    let mut net = Network::default();
    let mut bundle = Bundle::create(&net, project().path(), &StampOptions::default()).unwrap();
    let paths: Vec<String> = bundle
        .receipt()
        .committed()
        .leaves()
        .iter()
        .map(|l| l.path.clone())
        .collect();

    // Shared while pending: the calendar promise carries over and completes later.
    let mut pending = bundle.share("main.tex").unwrap();
    net.anchored = true;
    assert_eq!(
        opentimestamps_lite::upgrade(&net, &mut pending.proof).unwrap(),
        Status::Anchored {
            height: HEIGHT,
            time: TIME
        }
    );

    // Shared after upgrading: each file's proof already reaches Bitcoin.
    bundle.upgrade(&net).unwrap();
    for path in &paths {
        let shared = bundle.share(path).unwrap();
        let file_hash: [u8; 32] = Sha256::digest(&shared.bytes).into();
        assert_eq!(
            shared.proof.digest(),
            file_hash,
            "{path}: proof is for this file's bytes"
        );
        assert_eq!(shared.proof.status(), Status::Claimed { height: HEIGHT });
        let mut check = shared.proof.clone();
        assert_eq!(
            opentimestamps_lite::upgrade(&net, &mut check).unwrap(),
            Status::Anchored {
                height: HEIGHT,
                time: TIME
            },
            "{path}: replays through the tree and the calendars to the block"
        );
        let bytes = shared.proof.to_bytes();
        assert_eq!(
            draftstamp_core::Proof::from_bytes(&bytes).unwrap(),
            shared.proof
        );
    }
    assert!(matches!(
        bundle.share("missing.tex"),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn ignore_file_leaves_out_matching_files() {
    let net = Network::default();
    let folder = project();
    folder.write("main.aux", b"build output");
    folder.write("build/out.pdf", b"build output");
    folder.write(".draftstampignore", b"# LaTeX outputs\n*.aux\nbuild/\n");
    let bundle = Bundle::create(&net, folder.path(), &StampOptions::default()).unwrap();
    let paths: Vec<&str> = bundle
        .receipt()
        .committed()
        .leaves()
        .iter()
        .map(|l| l.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            ".draftstampignore",
            "figures/plot.png",
            "main.tex",
            "sections/intro.tex"
        ],
        "ignored files left out; the ignore file itself is stamped"
    );
}

#[test]
fn project_comes_from_the_draftstamp_file() {
    let net = Network::default();
    let folder = project();
    folder.write(
        ".draftstamp",
        b"# DraftStamp settings\nproject = \"riemann-draft\"\n",
    );
    let bundle = Bundle::create(&net, folder.path(), &StampOptions::default()).unwrap();
    assert_eq!(
        bundle.receipt().advisory().project.as_deref(),
        Some("riemann-draft")
    );
    assert!(
        bundle
            .receipt()
            .committed()
            .leaves()
            .iter()
            .all(|l| l.path != ".draftstamp"),
        "the settings file is never stamped"
    );
    let opts = StampOptions {
        label: None,
        project: Some("override".into()),
    };
    let bundle = Bundle::create(&net, folder.path(), &opts).unwrap();
    assert_eq!(
        bundle.receipt().advisory().project.as_deref(),
        Some("override"),
        "an explicit option wins"
    );
}
