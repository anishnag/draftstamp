#!/usr/bin/env python3
"""Generate DraftStamp v1 test vectors from an independent, stdlib-only implementation of spec/v1.md.

Run from the repo root:  python3 -I spec/vectors/gen_0001.py > spec/vectors/0001.json

Salts are fixed per case so results are reproducible; they are listed in canonical path order.
"""

import hashlib
import json
import sys
import unicodedata

MAX_PATH_BYTES = 4096
JUNK_DIRS = {".git", ".draftstamp", "__MACOSX"}
JUNK_FILES = {".DS_Store", "Thumbs.db", "desktop.ini", ".draftstamp"}


class VectorError(Exception):
    def __init__(self, code):
        super().__init__(code)
        self.code = code


def sha256(*parts):
    h = hashlib.sha256()
    for p in parts:
        h.update(p)
    return h.digest()


def canonicalize(raw):
    """Returns the canonical path, or None for junk. Raises VectorError on invalid paths."""
    p = raw
    while p.startswith("./"):
        p = p[2:]
    p = unicodedata.normalize("NFC", p)
    if (
        p == ""
        or len(p.encode("utf-8")) > MAX_PATH_BYTES
        or "\0" in p
        or "\\" in p
        or p.startswith("/")
    ):
        raise VectorError("invalid_path")
    segments = p.split("/")
    if any(s in ("", ".", "..") for s in segments):
        raise VectorError("invalid_path")
    if any(s in JUNK_DIRS for s in segments[:-1]) or segments[-1] in JUNK_FILES:
        return None
    return p


def mth(hashes):
    """RFC 6962 Merkle Tree Hash."""
    n = len(hashes)
    if n == 1:
        return hashes[0]
    k = 1
    while k * 2 < n:
        k *= 2
    return sha256(b"\x01", mth(hashes[:k]), mth(hashes[k:]))


def salts_for(name, count):
    return [sha256(b"draftstamp-test-salt", name.encode(), i.to_bytes(4, "big"))[:16] for i in range(count)]


def build(name, files):
    """files: list of (raw_path, bytes). Returns the expected result dict."""
    kept, skipped = [], []
    for raw, data in files:
        c = canonicalize(raw)
        if c is None:
            skipped.append(raw)
        else:
            kept.append((c, data))
    kept.sort(key=lambda kv: kv[0].encode("utf-8"))
    for (a, _), (b, _) in zip(kept, kept[1:]):
        if a == b:
            raise VectorError("duplicate_path")
    if not kept:
        raise VectorError("empty_project")

    salts = salts_for(name, len(kept))
    leaves, leaf_hashes = [], []
    for (path, data), salt in zip(kept, salts):
        pb = path.encode("utf-8")
        file_hash = sha256(data)
        leaf_hashes.append(sha256(b"\x00", salt, len(pb).to_bytes(4, "big"), pb, file_hash))
        leaves.append({"path": path, "size": len(data), "sha256": file_hash.hex(), "salt": salt.hex()})
    root = mth(leaf_hashes)
    commitment = sha256(b"\x02", b"draftstamp:v1", root)
    return {
        "salts": [s.hex() for s in salts],
        "leaves": leaves,
        "leaf_hashes": [h.hex() for h in leaf_hashes],
        "root": root.hex(),
        "commitment": commitment.hex(),
        "skipped": sorted(skipped),
    }


TEX = b"\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n"

CASES = [
    ("single-file", [("main.tex", TEX)]),
    ("two-files", [("main.tex", TEX), ("refs.bib", b"@article{a, title={A}}\n")]),
    ("three-files", [("a.tex", b"a"), ("b.tex", b"b"), ("c.tex", b"c")]),
    ("five-files-uneven-tree", [(f"{c}.tex", c.encode()) for c in "abcde"]),
    ("seven-files-uneven-tree", [(f"{c}.tex", c.encode()) for c in "abcdefg"]),
    ("nested-dirs-and-binary", [
        ("main.tex", TEX),
        ("figures/plot.png", bytes(range(256))),
        ("sections/intro.tex", b"\\section{Intro}\n"),
    ]),
    ("empty-file", [("empty.txt", b""), ("main.tex", TEX)]),
    ("crlf-bytes-preserved", [("windows.txt", b"line one\r\nline two\r\n")]),
    ("nfd-path-normalized", [("the\u0301ore\u0300me.tex", b"theorem")]),
    ("leading-dot-slash", [("./main.tex", TEX), ("././figs/a.png", b"png")]),
    ("byte-order-sort", [("a.tex", b"1"), ("B.tex", b"2"), ("Z/z.tex", b"3"), ("_x.tex", b"4")]),
    ("junk-skipped", [
        ("main.tex", TEX),
        (".DS_Store", b"junk"),
        ("sub/Thumbs.db", b"junk"),
        (".git/HEAD", b"ref: refs/heads/main\n"),
        ("__MACOSX/._main.tex", b"junk"),
        (".draftstamp/old.receipt.json", b"{}"),
        (".draftstamp", b'project = "riemann-draft"\n'),
        (".gitignore", b"*.aux\n"),
    ]),
    ("error-parent-segment", [("../secret.tex", b"x")]),
    ("error-absolute-path", [("/etc/passwd", b"x")]),
    ("error-empty-segment", [("a//b.tex", b"x")]),
    ("error-trailing-slash", [("dir/", b"x")]),
    ("error-backslash", [("dir\\file.tex", b"x")]),
    ("error-duplicate-after-nfc", [("caf\u00e9.tex", b"1"), ("cafe\u0301.tex", b"2")]),
    ("error-only-junk", [(".DS_Store", b"junk")]),
]


def main():
    vectors = []
    for name, files in CASES:
        case = {
            "name": name,
            "files": [{"path": p, "bytes_hex": d.hex()} for p, d in files],
        }
        try:
            case["expected"] = build(name, files)
        except VectorError as e:
            case["error"] = e.code
        vectors.append(case)
    out = {
        "description": "DraftStamp v1 test vectors. Generated by spec/vectors/gen_0001.py; do not edit by hand.",
        "tree": "draftstamp-merkle-v1",
        "cases": vectors,
    }
    json.dump(out, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
