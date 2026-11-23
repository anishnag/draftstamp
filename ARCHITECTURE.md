# Architecture

## Goal

When you stamp a draft, DraftStamp keeps three things together: that exact version of your work, its receipt, and its Bitcoin proof. They stay in a vault you control, so years later you can show what you had and when, and anyone can verify it without DraftStamp.

Stamping is deliberate and occasional ("save this draft"), not automatic on every change. The value is not the timestamp itself, which OpenTimestamps provides; it is that work and proof are never separated, are easy to find, and can be shared one file at a time.

## The bundle

Every stamp produces one self-contained folder in the vault. All three files are needed to prove anything.

```
~/.dsvault/                      the vault (or --vault, or $DRAFTSTAMP_VAULT)
└── <stamp id>/                  e.g. 20261121T140312Z-e72b05: UTC time + start of the commitment
    ├── snapshot.zip             the exact stamped files
    ├── receipt.json             paths, sizes, hashes and salts: turns the snapshot into the commitment
    └── proof.ots                OpenTimestamps proof that the commitment existed before a Bitcoin block
```

The vault is flat. A stamp's project is a tag in its receipt, not a folder, so renaming a project never moves or rewrites anything. Bundles are never deleted or overwritten; the only change ever made is replacing a pending proof with its upgraded version.

Per-file proofs are not stored. Any file's `.ots` is derived on demand from the receipt and `proof.ots`, so it always reflects the latest upgrade.

## Crates

```
                  draftstamp-cli
     init · stamp · upgrade · log · find ·
     check · verify · share · export
                        │
                        ▼
                 draftstamp-core
     commitment: files → salted Merkle tree → commitment
     bundle: create, check, verify, share, export
     vault: store and read bundles · history and find
                        │
                        ▼
               opentimestamps-lite
     stamp, upgrade, check proofs against Bitcoin,
     build proofs from steps
```

| Crate | I/O | Role |
| --- | --- | --- |
| `opentimestamps-lite` | No I/O of its own: blocking network calls go through an `Http` interface the caller supplies | A small, general OpenTimestamps library (not the upstream `opentimestamps` crate): reading and writing `.ots` files, stamping with the public aggregators, upgrading from the calendars, checking proofs against Bitcoin block headers, and building a proof from prepend/append/sha256 steps followed by another proof. Knows nothing about DraftStamp. |
| `draftstamp-core` | Files: reads the folder being stamped, writes the vault, exports and shares. Network only through the caller's `Http` | The engine. `commitment` turns files into a commitment and receipt (no I/O); `bundle` creates, checks, verifies, shares and exports one stamp; `vault` stores bundles (`LocalVault`, a folder on disk); `query` answers history and find. Also reads `.draftstamp` and `.draftstampignore`. |
| `draftstamp-cli` | Files and network | The `draftstamp` command; supplies native HTTP (`ureq`, 10-second timeouts, no identifying headers). |

Dependencies point down only. `opentimestamps-lite` stays independent of DraftStamp so others can use it on its own. Anything built later (the Mac app, in progress, built with Tauri on `draftstamp-core`) sits on top and never re-implements the format.

**Website.** A static landing page that explains DraftStamp and links to the CLI download. It performs no stamping or verification and has no accounts or backend.

## Sharing

A file's `.ots` is the path from that file through the stamp's tree, written as standard OpenTimestamps operations, followed by the stamp's proof. Its digest is the file's own SHA-256, so the official client checks it with `ots verify -f`. A folder or selection is shared as one `.ots` per file (a `.ots` proves one file), optionally zipped with a README. Shares never include the receipt, snapshot or stamp proof, so unselected files stay hidden; `export` writes the complete record.

## Folder settings

- `.draftstamp` names the folder's project. It is never stamped.
- `.draftstampignore` lists gitignore-like patterns to leave out. It is stamped, so the record shows what was excluded.
- `.git`, `.DS_Store` and similar are always skipped.

## Storage

The vault is reached through a small `Vault` trait (put, put_proof, get, receipt, proof, list), so where bundles live can change without touching the rest. Today there is one implementation, a local folder; it can also sit inside a synced folder such as Dropbox or iCloud. Possible later backends: a git repository, or remote storage encrypted on the device.

## Rules

- Nothing about your work leaves your machine except a blinded 32-byte value sent to the OpenTimestamps aggregators.
- Network use is explicit: `stamp`, `upgrade`, `verify` and `share` (which tries one upgrade first) go online; `init`, `log`, `find`, `check` and `export` never do. No telemetry, no update checks.
- The formats are open and versioned, and specified in `spec/` (`0000_ots.md`, `0001_core.md`, `0002_cli.md`), with test vectors generated by an independent Python implementation.
- Every file can be shared with its own standard `.ots` proof and verified with the official OpenTimestamps client, with no DraftStamp software and without revealing the other files.

## Testing and releases

- The libraries take `Http` as a parameter, so every test runs offline against a simulated network; the CLI's commands are tested in-process the same way.
- Two opt-in tests go further: the official `ots` client checks DraftStamp's per-file proofs, and a live test stamps through the real calendars and, on a later run, checks a confirmed stamp against the real Bitcoin block.
- CI runs formatting, clippy and tests on Linux, macOS and Windows, builds with the minimum Rust version (1.89), and checks that the test vectors regenerate exactly.
- Pushing a `v*` tag builds the CLI for macOS, Linux and Windows and publishes a GitHub release.
