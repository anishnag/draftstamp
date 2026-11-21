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
       draftstamp-cli             draftstamp-desktop
  stamp · log · find ·            same commands, GUI
  export · verify
              └────────────┬────────────┘
                           ▼
                    draftstamp-core
           engine: files → commitment, receipts,
           verification · vault: bundles, find,
           history, export, storage backends
                           │
                           ▼
                  opentimestamps-lite
           OpenTimestamps: stamp, upgrade,
           check proofs against Bitcoin
```

| Crate | I/O | Role |
| --- | --- | --- |
| `opentimestamps-lite` | None: blocking network calls through an `Http` interface the caller supplies | A local, general OpenTimestamps library (not the upstream `opentimestamps` crate): stamping with the public calendars, upgrading, checking proofs against Bitcoin headers. Knows nothing about DraftStamp. |
| `draftstamp-core` | Files, only in the vault's storage backends | The DraftStamp engine. The `commitment` modules turn files into a commitment and verify them (no I/O); the `vault` modules save and read bundles, find which stamps contain a file, show project history and export evidence. |
| `draftstamp-cli` | Files and network | The commands; supplies native HTTP to opentimestamps-lite |
| `draftstamp-desktop` | Files and network | A desktop app (Tauri) on the same engine |

Dependencies point down only: the front ends use `draftstamp-core`, which uses `opentimestamps-lite`. `opentimestamps-lite` stays independent of DraftStamp so others can use it on its own. Anything built later sits on top and never re-implements the format.

There is no WebAssembly build: both front ends run the engine natively.

**Website.** A static landing page that explains DraftStamp and links to the CLI download. It performs no stamping or verification and has no accounts or backend.

## Storage

The vault writes through a small storage interface (save, list, read a bundle), so where bundles live can change without touching the rest.

- **Local folder:** default `~/DraftStamp`, or a synced Dropbox or iCloud folder.
- **Git repository:** the commit is the snapshot; receipt and proof are kept alongside.
- **Remote encrypted storage:** encrypted on the device, stored as ciphertext.

## Rules

- Nothing about your work leaves your machine except a blinded 32-byte value sent to the OpenTimestamps calendars.
- Proofs are completed quietly: any command first upgrades pending proofs, and network failures never block it.
- The bundle format is open and versioned. A bundle verifies with the official OpenTimestamps client and a stdlib-only script, with no DraftStamp software.
