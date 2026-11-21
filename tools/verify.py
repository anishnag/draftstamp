#!/usr/bin/env python3
"""Verify a DraftStamp bundle without DraftStamp. Python standard library only.

Usage:
    python3 verify.py <bundle>

<bundle> is a stamp folder (containing snapshot.zip, receipt.json and proof.ots) or a zip
exported by DraftStamp. This checks that every file in the snapshot matches the receipt, that the
receipt's root and commitment are correct, and that proof.ots is a proof of that commitment.

To check the proof against Bitcoin, then run the official OpenTimestamps client:
    ots verify -d <commitment> proof.ots

Exit status: 0 if everything checks out, 1 otherwise. Format: spec/0001_core.md.
"""

import hashlib
import io
import json
import sys
import unicodedata
import zipfile
from pathlib import Path

OTS_MAGIC = b"\x00OpenTimestamps\x00\x00Proof\x00\xbf\x89\xe2\xe8\x84\xe8\x92\x94"
JUNK_DIRS = {".git", ".draftstamp", "__MACOSX"}
JUNK_FILES = {".DS_Store", "Thumbs.db", "desktop.ini"}


class Invalid(Exception):
    pass


def sha256(*parts):
    h = hashlib.sha256()
    for p in parts:
        h.update(p)
    return h.digest()


def canonical(path):
    """True if path is exactly as DraftStamp writes it (spec: Canonical paths)."""
    if not path or path != unicodedata.normalize("NFC", path) or len(path.encode()) > 4096:
        return False
    if "\\" in path or "\0" in path or path.startswith("/"):
        return False
    parts = path.split("/")
    if any(p in ("", ".", "..") for p in parts):
        return False
    return not (any(p in JUNK_DIRS for p in parts[:-1]) or parts[-1] in JUNK_FILES)


def merkle_root(hashes):
    """RFC 6962 Merkle Tree Hash."""
    if len(hashes) == 1:
        return hashes[0]
    k = 1
    while k * 2 < len(hashes):
        k *= 2
    return sha256(b"\x01", merkle_root(hashes[:k]), merkle_root(hashes[k:]))


def load(bundle):
    """Returns (receipt bytes, snapshot bytes, proof bytes) from a folder or an exported zip."""
    path = Path(bundle)
    names = ("receipt.json", "snapshot.zip", "proof.ots")
    if path.is_dir():
        return tuple((path / n).read_bytes() for n in names)
    with zipfile.ZipFile(path) as z:
        found = {n.rsplit("/", 1)[-1]: n for n in z.namelist()}
        if not all(n in found for n in names):
            raise Invalid("the zip does not contain snapshot.zip, receipt.json and proof.ots")
        return tuple(z.read(found[n]) for n in names)


def verify(bundle):
    receipt_bytes, snapshot_bytes, proof = load(bundle)
    receipt = json.loads(receipt_bytes)

    if receipt.get("format") != "draftstamp.receipt" or receipt.get("version") != 1:
        raise Invalid("not a version 1 DraftStamp receipt")
    if set(receipt) - {"format", "version", "committed", "advisory"}:
        raise Invalid("unknown field in the receipt")
    c = receipt["committed"]
    if set(c) != {"tree", "leaves", "root", "commitment"} or c["tree"] != "draftstamp-merkle-v1":
        raise Invalid("unknown committed fields or tree algorithm")
    leaves = c["leaves"]
    paths = [leaf["path"] for leaf in leaves]
    if not leaves or paths != sorted(paths, key=lambda p: p.encode()) or len(set(paths)) != len(paths):
        raise Invalid("leaves are missing, unsorted or duplicated")

    with zipfile.ZipFile(io.BytesIO(snapshot_bytes)) as snap:
        names = snap.namelist()
        if sorted(names) != sorted(paths):
            raise Invalid("the snapshot's files are not exactly the receipt's files")
        hashes = []
        for leaf in leaves:
            if set(leaf) != {"path", "size", "sha256", "salt"} or not canonical(leaf["path"]):
                raise Invalid(f"invalid leaf {leaf.get('path')!r}")
            data = snap.read(leaf["path"])
            file_hash = sha256(data)
            if len(data) != leaf["size"] or file_hash.hex() != leaf["sha256"]:
                raise Invalid(f"{leaf['path']} does not match the receipt")
            p = leaf["path"].encode()
            hashes.append(sha256(b"\x00", bytes.fromhex(leaf["salt"]), len(p).to_bytes(4, "big"), p, file_hash))

    root = merkle_root(hashes)
    commitment = sha256(b"\x02", b"draftstamp:v1", root)
    if root.hex() != c["root"] or commitment.hex() != c["commitment"]:
        raise Invalid("the root or commitment does not match the files")

    if proof[:31] != OTS_MAGIC or proof[31:33] != b"\x01\x08" or proof[33:65] != commitment:
        raise Invalid("proof.ots is not an OpenTimestamps proof of this commitment")
    return len(leaves), commitment.hex()


def main(argv):
    if len(argv) != 2:
        print(__doc__.strip())
        return 1
    try:
        files, commitment = verify(argv[1])
    except (Invalid, OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile) as e:
        print(f"FAILED: {e}")
        return 1
    print(f"OK: {files} file(s) match the receipt, and proof.ots proves commitment {commitment}.")
    print(f"To check it against Bitcoin:  ots verify -d {commitment} proof.ots")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
