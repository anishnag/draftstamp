//! Canonical paths (spec section 7.1) and the built-in junk list (section 6).

use unicode_normalization::UnicodeNormalization;

use crate::Error;

/// Longest canonical path accepted, in UTF-8 bytes.
pub(crate) const MAX_PATH_BYTES: usize = 4096;

/// Directory names skipped wherever they appear.
const JUNK_DIRS: &[&str] = &[".git", ".draftstamp", "__MACOSX"];

/// File names skipped wherever they appear.
const JUNK_FILES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Canonical {
    /// Stamp this file under this canonical path.
    Keep(String),
    /// A junk file: leave it out silently.
    Skip,
}

/// Turns a host-supplied relative path into its canonical form.
pub(crate) fn canonicalize(raw: &str) -> Result<Canonical, Error> {
    let invalid = |reason| Error::InvalidPath {
        path: raw.to_owned(),
        reason,
    };

    let mut trimmed = raw;
    while let Some(rest) = trimmed.strip_prefix("./") {
        trimmed = rest;
    }
    let path: String = trimmed.nfc().collect();

    if path.is_empty() {
        return Err(invalid("empty path"));
    }
    if path.len() > MAX_PATH_BYTES {
        return Err(invalid("longer than 4096 bytes"));
    }
    if path.contains('\0') {
        return Err(invalid("contains a NUL character"));
    }
    if path.contains('\\') {
        return Err(invalid("contains a backslash; use '/' between folders"));
    }
    if path.starts_with('/') {
        return Err(invalid(
            "absolute path; paths must be relative to the project",
        ));
    }

    let segments: Vec<&str> = path.split('/').collect();
    for segment in &segments {
        match *segment {
            "" => return Err(invalid("empty segment (a doubled or trailing '/')")),
            "." | ".." => return Err(invalid("contains a '.' or '..' segment")),
            _ => {}
        }
    }

    let (file, dirs) = segments
        .split_last()
        .expect("split always yields a segment");
    if dirs.iter().any(|d| JUNK_DIRS.contains(d)) || JUNK_FILES.contains(file) {
        return Ok(Canonical::Skip);
    }
    Ok(Canonical::Keep(path))
}

/// True when `path` is already canonical and not junk, as every receipt path must be.
pub(crate) fn is_canonical(path: &str) -> bool {
    matches!(canonicalize(path), Ok(Canonical::Keep(p)) if p == path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keep(raw: &str) -> String {
        match canonicalize(raw).unwrap() {
            Canonical::Keep(p) => p,
            Canonical::Skip => panic!("{raw:?} was skipped"),
        }
    }

    #[test]
    fn normalizes() {
        assert_eq!(keep("main.tex"), "main.tex");
        assert_eq!(keep("./main.tex"), "main.tex");
        assert_eq!(keep("././figs/a.png"), "figs/a.png");
        assert_eq!(keep("the\u{301}ore\u{300}me.tex"), "th\u{e9}or\u{e8}me.tex");
    }

    #[test]
    fn rejects() {
        for raw in [
            "",
            "./",
            "/etc/passwd",
            "a//b",
            "dir/",
            "../x",
            "a/./b",
            "a\\b",
            "a\0b",
        ] {
            assert!(canonicalize(raw).is_err(), "{raw:?} should be rejected");
        }
        assert!(canonicalize(&"a".repeat(MAX_PATH_BYTES + 1)).is_err());
    }

    #[test]
    fn skips_junk() {
        for raw in [
            ".DS_Store",
            "sub/Thumbs.db",
            ".git/HEAD",
            "a/.git/config",
            "__MACOSX/._x",
            ".draftstamp/r.json",
        ] {
            assert_eq!(canonicalize(raw).unwrap(), Canonical::Skip, "{raw:?}");
        }
        // Only exact names are junk.
        assert_eq!(keep(".gitignore"), ".gitignore");
        assert_eq!(keep("notes/.git-notes"), "notes/.git-notes");
    }

    #[test]
    fn canonical_check() {
        assert!(is_canonical("figs/a.png"));
        assert!(!is_canonical("./figs/a.png"));
        assert!(!is_canonical("the\u{301}ore\u{300}me.tex"));
        assert!(!is_canonical(".DS_Store"));
    }
}
