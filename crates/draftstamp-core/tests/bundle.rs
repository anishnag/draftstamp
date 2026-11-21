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

#[test]
fn a_single_file_is_a_one_file_stamp() {
    let net = Network::default();
    let folder = project();
    folder.write(".draftstampignore", b"*.tex\n");
    let bundle = Bundle::create(
        &net,
        &folder.path().join("main.tex"),
        &StampOptions::default(),
    )
    .unwrap();
    let leaves = bundle.receipt().committed().leaves();
    assert_eq!(leaves.len(), 1);
    assert_eq!(
        leaves[0].path, "main.tex",
        "the file's name; the ignore file does not apply"
    );
    assert_eq!(
        bundle.receipt().advisory().project.as_deref(),
        Some("main.tex")
    );
    assert_eq!(bundle.check().unwrap(), 1);
    assert_eq!(
        bundle.share("main.tex").unwrap().bytes,
        b"\\documentclass{article}\n"
    );

    folder.write(".draftstamp", b"project = \"paper\"\n");
    let bundle = Bundle::create(
        &net,
        &folder.path().join("main.tex"),
        &StampOptions::default(),
    )
    .unwrap();
    assert_eq!(
        bundle.receipt().advisory().project.as_deref(),
        Some("paper"),
        "the file's folder settings"
    );
}

#[test]
fn select_and_share_zip() {
    use sha2::Digest;
    use std::io::Read;
    let mut net = Network::default();
    let folder = project();
    folder.write("sections/method.tex", b"\\section{Method}\n");
    let mut bundle = Bundle::create(&net, folder.path(), &StampOptions::default()).unwrap();
    assert_eq!(
        bundle.select(&["sections/"]).unwrap(),
        ["sections/intro.tex", "sections/method.tex"]
    );
    assert_eq!(
        bundle
            .select(&["sections", "main.tex", "./main.tex"])
            .unwrap()
            .len(),
        3,
        "no repeats"
    );
    assert_eq!(bundle.select(&["."]).unwrap().len(), 4);
    assert!(
        bundle.select(&["sect"]).is_err(),
        "a prefix of a name is not a folder"
    );
    assert!(matches!(
        bundle.select(&["missing/"]),
        Err(Error::NotFound(_))
    ));

    net.anchored = true;
    bundle.upgrade(&net).unwrap();
    let out = TempDir::new();
    let zip_path = out.path().join("shared.zip");
    assert_eq!(bundle.share_zip(&["sections"], &zip_path).unwrap(), 2);
    let mut archive = zip::ZipArchive::new(Cursor::new(std::fs::read(&zip_path).unwrap())).unwrap();
    let mut names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    names.sort();
    let id = bundle.id();
    let expected: Vec<String> = [
        "README.txt",
        "sections/intro.tex",
        "sections/intro.tex.ots",
        "sections/method.tex",
        "sections/method.tex.ots",
    ]
    .iter()
    .map(|n| format!("{id}/{n}"))
    .collect();
    assert_eq!(
        names, expected,
        "only the selection: no receipt, snapshot or stamp proof"
    );

    let mut ots = Vec::new();
    archive
        .by_name(&format!("{id}/sections/method.tex.ots"))
        .unwrap()
        .read_to_end(&mut ots)
        .unwrap();
    let mut proof = draftstamp_core::Proof::from_bytes(&ots).unwrap();
    assert_eq!(
        proof.digest(),
        <[u8; 32]>::from(sha2::Sha256::digest(b"\\section{Method}\n"))
    );
    assert!(matches!(
        opentimestamps_lite::upgrade(&net, &mut proof).unwrap(),
        Status::Anchored { .. }
    ));
    let mut readme = String::new();
    archive
        .by_name(&format!("{id}/README.txt"))
        .unwrap()
        .read_to_string(&mut readme)
        .unwrap();
    assert!(
        readme.contains("  sections/method.tex\n") && !readme.contains("main.tex"),
        "{readme}"
    );
}
