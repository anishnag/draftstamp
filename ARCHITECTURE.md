# Architecture

## Goal

When you stamp a draft, DraftStamp keeps three things together: that exact version of your work, its receipt, and its Bitcoin proof. They stay in a vault you control, so years later you can show what you had and when, and anyone can verify it without DraftStamp.

Stamping is deliberate and occasional ("save this draft"), not automatic on every change. The value is not the timestamp itself, which OpenTimestamps provides; it is that work and proof are never separated and are always easy to find.

## The bundle

Every stamp produces one self-contained folder in the vault. All three files are needed to prove anything.

```
<vault>/
└── <project>/
    └── <stamp id>/
        ├── snapshot.zip     the exact stamped files
        ├── receipt.json     file list and salts: turns the snapshot into the commitment
        └── proof.ots        OpenTimestamps proof that the commitment existed before a Bitcoin block
```

## Crates

```
                    draftstamp-cli                   desktop app (later)
          stamp · log · find · export · verify       same engine, GUI
                           │
         ┌─────────────────┼──────────────────┐
         ▼                 ▼                  ▼
  draftstamp-core    draftstamp-ots     draftstamp-vault
  files → commitment commitment → .ots  bundles: save, list,
  receipts, verify   stamp, upgrade,    find, export
                     Bitcoin check      storage backends
```

| Crate | I/O | Role | Status |
| --- | --- | --- | --- |
| `draftstamp-core` | None | Canonical paths, salted Merkle tree, commitment, receipts, file verification | Built |
| `draftstamp-ots` | None: network through an `Http` interface the host supplies | Stamping with OpenTimestamps calendars, upgrading, checking proofs against Bitcoin headers | Built |
| `draftstamp-vault` | Files, through storage backends | Writing and reading bundles, finding which stamps contain a file, project history, exporting evidence | Next |
| `draftstamp-cli` | Files and network | The commands; supplies native HTTP to ots | Next |

There is no WebAssembly build: the CLI runs the engine natively, and the desktop app will call the same Rust code directly.

**Website.** A static landing page that explains DraftStamp and links to the CLI download. It performs no stamping or verification and has no accounts or backend.

`draftstamp-core` and `draftstamp-ots` never depend on each other. `draftstamp-vault` builds on both (it reads receipts and proofs), and the CLI, later the desktop app, sits on top. Anything built later never re-implements the format.

## Storage

The vault writes through a small storage interface (save, list, read a bundle), so where bundles live can change without touching the rest.

| Backend | When |
| --- | --- |
| Local folder (default `~/DraftStamp`, or a synced Dropbox or iCloud folder) | v1 |
| Git repository: the commit is the snapshot; receipt and proof kept alongside | Later |
| Remote encrypted storage: encrypted on the device, stored as ciphertext | Later |

## Rules

- Nothing about your work leaves your machine except a blinded 32-byte value sent to the OpenTimestamps calendars.
- Proofs are completed quietly: any command first upgrades pending proofs, and network failures never block it.
- The bundle format is open and versioned. A bundle verifies with the official OpenTimestamps client and a stdlib-only script, with no DraftStamp software.
