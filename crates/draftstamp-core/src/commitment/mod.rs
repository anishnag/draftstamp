//! Turning a folder's files into a commitment, and receipts (spec/0001_core.md, "Commitment").

pub(crate) mod hash;
pub(crate) mod paths;
pub(crate) mod receipt;
pub(crate) mod stamp;
pub(crate) mod tree;

pub use hash::{Digest, Salt};
pub use receipt::{Advisory, Committed, Leaf, Receipt, TREE_ALGORITHM};

#[cfg(test)]
mod tests {
    //! Checks against spec/vectors/0001.json, produced by an independent Python implementation
    //! (spec/vectors/gen_0001.py).
    use super::stamp::{File, commit_with_salts};
    use super::*;
    use serde_json::Value;

    const VECTORS: &str = include_str!("../../../../spec/vectors/0001.json");

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn code(e: &crate::Error) -> &'static str {
        match e {
            crate::Error::InvalidPath { .. } => "invalid_path",
            crate::Error::DuplicatePath(_) => "duplicate_path",
            crate::Error::EmptyFolder => "empty_project",
            _ => "other",
        }
    }

    #[test]
    fn matches_independent_implementation() {
        let doc: Value = serde_json::from_str(VECTORS).unwrap();
        let cases = doc["cases"].as_array().unwrap();
        assert!(cases.len() >= 15);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let files: Vec<File> = case["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| File {
                    path: f["path"].as_str().unwrap().into(),
                    bytes: hex(f["bytes_hex"].as_str().unwrap()),
                })
                .collect();
            let salts: Vec<Salt> = case["expected"]["salts"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|s| s.as_str().unwrap().parse().unwrap())
                        .collect()
                })
                .unwrap_or_default();
            let mut next = salts.into_iter();
            match (
                commit_with_salts(&files, || next.next().unwrap()),
                case.get("error"),
            ) {
                (Ok(s), None) => {
                    let exp = &case["expected"];
                    assert_eq!(s.committed.root().to_string(), exp["root"], "{name}: root");
                    assert_eq!(
                        s.committed.commitment().to_string(),
                        exp["commitment"],
                        "{name}: commitment"
                    );
                    assert_eq!(
                        serde_json::to_value(s.committed.leaves()).unwrap(),
                        exp["leaves"],
                        "{name}: leaves"
                    );
                    assert_eq!(
                        serde_json::to_value(&s.skipped).unwrap(),
                        exp["skipped"],
                        "{name}: skipped"
                    );
                }
                (Err(e), Some(c)) => assert_eq!(code(&e), c, "{name}"),
                (r, e) => panic!("{name}: got {:?}, expected error {e:?}", r.map(|_| ())),
            }
        }
    }
}
