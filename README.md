# DraftStamp

**Prove your work existed, with a date no one can forge.**

DraftStamp is a small command-line tool for timestamping drafts. Point it at a folder (a paper, a
thesis, a proof, a book, some code) and it records a fingerprint of those exact files in the
Bitcoin blockchain through [OpenTimestamps](https://opentimestamps.org). It keeps the exact files,
a receipt, and the proof together in a local vault, so years later you can show precisely what you
had and when.

Any single file can then be shared with its own standard `.ots` proof that **anyone can check with
the official OpenTimestamps client, without DraftStamp and without seeing your other files.**

- No account, no server, no wallet, no coins, no fees. Your files never leave your machine; only a
  32-byte hash does.
- Open source (Apache-2.0), with the formats written down as specs in [`spec/`](spec).

> **Status:** early, but working end to end: stamps made with DraftStamp have been confirmed in
> real Bitcoin blocks and checked with the official `ots` client. The CLI is ready to use; a Mac
> app is in progress.

## Why

If you work on something for months before publishing (a mathematical result, a manuscript, a
design), "who had it first?" can matter, and the usual evidence is weak: file dates are trivially
editable, emails to yourself can be faked, and git history can be rewritten.

A Bitcoin timestamp is different. Once a block is buried under later blocks, nobody (including
you) can change what it committed to. If your file's hash leads to that block, the file existed
before the block was mined.

DraftStamp makes that practical for ongoing work: stamp a draft whenever it matters ("before
submission", "first full proof"), keep every version organized by project, and share exactly the
parts you choose.

## A quick tour

```console
$ cd ~/papers/riemann-draft
$ draftstamp init
Wrote .draftstamp: stamps of this folder are tagged "riemann-draft".

$ draftstamp stamp -m "first full draft"
Stamped 14 files of riemann-draft as 20261121T140312Z-e72b05.
Pending: Bitcoin confirmation usually takes a few hours. Run `draftstamp upgrade` later.
```

A few hours later, once Bitcoin has confirmed it:

```console
$ draftstamp upgrade
20261121T140312Z-e72b05  confirmed in Bitcoin block 976886 (2026-11-21 15:38 UTC)

$ draftstamp log
20261121T140312Z-e72b05  riemann-draft  14 files  in Bitcoin block 976886  "first full draft"
20261118T091500Z-a41c9e  riemann-draft  13 files  in Bitcoin block 976102  "outline"

$ draftstamp verify
Record intact: all 14 files match the receipt, and the proof is for this stamp.
Bitcoin: confirmed in block 976886. These files existed by 2026-11-21 15:38 UTC.
```

Share one chapter with a reviewer, without revealing anything else:

```console
$ draftstamp share chapters/chapter3.tex
Wrote chapter3.tex and chapter3.tex.ots from stamp 20261121T140312Z-e72b05.
Anyone can check it with: ots verify -f chapter3.tex chapter3.tex.ots
```

The reviewer needs only the official OpenTimestamps client:

```console
$ ots verify -f chapter3.tex chapter3.tex.ots
Success! Bitcoin block 976886 attests existence as of 2026-11-21 UTC
```

Or share a whole folder, or a selection, as one zip with a README explaining how to check it:

```console
$ draftstamp share chapters/ main.tex --zip for-reviewer.zip
Wrote for-reviewer.zip: 6 files, each with its own .ots, and a README, from stamp 20261121T140312Z-e72b05.
```

## Install

**Prebuilt binaries** for macOS, Linux and Windows are on the
[releases page](https://github.com/anishnag/draftstamp/releases/latest). For example:

```sh
# macOS (Apple Silicon). For Intel Macs use x86_64-apple-darwin.
curl -fsSL https://github.com/anishnag/draftstamp/releases/latest/download/draftstamp-aarch64-apple-darwin.tar.gz | tar -xz
sudo mv draftstamp-aarch64-apple-darwin/draftstamp /usr/local/bin/

# Linux (x86_64). For ARM use aarch64-unknown-linux-gnu.
curl -fsSL https://github.com/anishnag/draftstamp/releases/latest/download/draftstamp-x86_64-unknown-linux-gnu.tar.gz | tar -xz
sudo mv draftstamp-x86_64-unknown-linux-gnu/draftstamp /usr/local/bin/
```

Each archive has a `.sha256` next to it, and `SHA256SUMS` covers them all. The macOS binaries are
not yet signed: downloading with `curl` as above avoids Gatekeeper's quarantine; if you download
in a browser instead, run `xattr -d com.apple.quarantine draftstamp` once.

**From source** (Rust 1.89 or newer):

```sh
cargo install --locked --git https://github.com/anishnag/draftstamp draftstamp-cli
```

**To check proofs** (yours or anyone's), install the official client:
`pip install opentimestamps-client` (or `pipx install opentimestamps-client`).

## Commands

| Command | What it does | Network |
|---|---|---|
| `draftstamp init [--project <name>]` | Write `.draftstamp` in this folder, naming its project | no |
| `draftstamp stamp [<folder> \| <file>] [-m <label>]` | Stamp a folder (this one by default) or a single file | calendars |
| `draftstamp upgrade [<stamp>]` | Complete pending proofs once Bitcoin confirms them | calendars, explorers |
| `draftstamp log [--project <name> \| --all]` | List stamps, newest first | no |
| `draftstamp find <file>` | Which stamps contain this file's exact bytes? | no |
| `draftstamp check [<stamp>]` | Is the stored record intact? (offline) | no |
| `draftstamp verify [<stamp>]` | Check, then confirm the date with Bitcoin | explorers |
| `draftstamp share <path>... [--zip <file>]` | Files or folders from a stamp, each with its own `.ots` | one upgrade attempt |
| `draftstamp export [<stamp>]` | The complete record as one zip | no |

A `<stamp>` is a full id, a unique prefix, or its 6-character ending (`e72b05`); left out, it means
the newest stamp of the current folder's project. `check` and `verify` exit with code 3 if anything
does not match. Full details: [`spec/0002_cli.md`](spec/0002_cli.md).

**Settings and ignores.** `.draftstamp` names the folder's project, so you don't pass `--project`
each time; it is never stamped. `.draftstampignore` uses gitignore-like patterns (`*.aux`,
`build/`, `figures/raw/*`) to leave out build outputs; it is stamped, so the record shows what was
excluded. `.git`, `.DS_Store` and similar are always skipped.

**The vault** is `~/.dsvault` (or `--vault`, or `$DRAFTSTAMP_VAULT`). Each stamp is one folder:

```
~/.dsvault/20261121T140312Z-e72b05/
├── snapshot.zip    the exact files, as they were
├── receipt.json    paths, sizes, hashes and salts: how the files become the timestamped value
└── proof.ots       the OpenTimestamps proof of that value
```

Nothing in the vault is ever overwritten, except that a pending proof is replaced by its
upgraded version. **Back it up**: the proof is only useful alongside the files and receipt.

## How it works

```
your files ──► salted Merkle tree ──► one 32-byte commitment ──► OpenTimestamps calendars ──► a Bitcoin block
```

1. **Commit.** Every file becomes a leaf: `SHA256(0x00 ‖ salt ‖ len(path) ‖ path ‖ SHA256(file))`,
   with a fresh random 16-byte salt per file. The leaves form an
   [RFC 6962](https://www.rfc-editor.org/rfc/rfc6962) Merkle tree, and the root is wrapped into a
   domain-separated commitment: `SHA256(0x02 ‖ "draftstamp:v1" ‖ root)`.
2. **Timestamp.** The commitment (plus a random nonce) goes to the public OpenTimestamps
   aggregators; at least two must accept it. They batch it with everyone else's and commit to
   Bitcoin, usually within a few hours. `upgrade` then fetches the path to the block.
3. **Keep.** The exact files, the receipt and the proof go into the vault together.
4. **Share.** For any file, DraftStamp writes the path from that file through the tree as standard
   OpenTimestamps operations (prepend, append, sha256), followed by the stamp's proof. The result
   is an ordinary `.ots` whose digest is the file's own SHA-256, so the official client can check
   it with `ots verify -f`.

Everything is specified, with test vectors from an independent Python implementation:
[`spec/0000_ots.md`](spec/0000_ots.md) (the OpenTimestamps subset), [`spec/0001_core.md`](spec/0001_core.md)
(commitments, receipts, bundles, sharing), [`spec/0002_cli.md`](spec/0002_cli.md) (the command),
and [`ARCHITECTURE.md`](ARCHITECTURE.md).

## Verifying without DraftStamp

| You have | Run | It proves |
|---|---|---|
| A shared file and its `.ots` | `ots verify -f file file.ots` | This exact file existed by the block's date |
| An export (`draftstamp export`) | `ots verify -f` on each file in `files/` | Every file, each on its own |
| A full bundle | The steps in [`spec/0001_core.md`](spec/0001_core.md), then `ots verify -d <commitment> proof.ots` | The complete set: exactly these files, nothing missing or added |

`ots verify` checks the Bitcoin block against a local Bitcoin node. Without one, run
`ots --no-bitcoin -v verify …`: it prints the block height and the merkle root to compare with any
block explorer.

## What is revealed, and to whom

- **The OpenTimestamps servers** see one random-looking 32-byte value per stamp. Not your files,
  not their names, not how many there are.
- **Someone you share a file with** learns that file's name and contents, and from the shape of its
  proof, roughly how many files the stamp had. The other files' names and contents stay hidden:
  their hashes in the proof are salted.
- **The receipt** lists every file's name and salt. It stays in your vault; share it (or an
  `export`) only with someone who should see everything.
- **DraftStamp itself** collects nothing. The only network traffic is to the OpenTimestamps
  aggregators and calendars, and to two block explorers (blockstream.info and mempool.space) when
  checking a block. No telemetry, no update checks.

## FAQ

**Do I need Bitcoin, a wallet or any money?**
No. OpenTimestamps calendars are free public services that batch many timestamps into a single
Bitcoin transaction they pay for. You never touch Bitcoin directly.

**What exactly does a stamp prove?**
That these exact bytes existed no later than the time of a specific Bitcoin block. It does not
prove who wrote them, or that they did not exist earlier. It is evidence of possession at a time,
which is what priority disputes usually turn on.

**How precise is the date?**
To within a couple of hours. A block's timestamp is set by its miner and Bitcoin's rules keep it
within about two hours of real time. Proofs are usually confirmed within a few hours of stamping;
until then a stamp is "pending" and backed only by the calendars' promises.

**How is this different from running `ots stamp` myself?**
The official client is excellent, and DraftStamp's proofs are fully compatible with it. What
DraftStamp adds is everything around the proof for ongoing work:

- it keeps the **exact files** with each proof, so you are never left with a `.ots` and a file that
  has since changed by a byte;
- it stamps **folders as a unit**, binding each file's path and checking later that the set is
  complete (nothing added or missing);
- **project history and search**: `log` and `find` ("which drafts contained this exact file?");
- **upgrades** pending proofs for you, and never overwrites anything;
- **selective sharing**: one file, a folder or any selection, each with a standalone `.ots`, salted
  so nothing about the other files leaks.

**Why not git commit dates, or emailing it to myself?**
Both are easy to fake after the fact: git dates are whatever your clock says and history can be
rewritten; an email's date is controlled by your provider and can't be independently checked. A
Bitcoin block can't be changed once it is buried.

**What if OpenTimestamps or DraftStamp disappears?**
A confirmed proof needs neither: it is a chain of SHA-256 steps ending in a Bitcoin block header,
which anyone can check with any implementation. Only *pending* proofs depend on the calendars, until
`upgrade` completes them. DraftStamp's formats are specified so they can be checked without its
code.

**What if I lose my vault?**
Then you lose the proof for anything you hadn't shared or exported. A proof without the exact files
is useless, and the files alone prove nothing. Keep `~/.dsvault` in your backups.

**Can I stamp unpublished or confidential work?**
Yes. Only a hash leaves your machine. When you later share a single file, only that file is
revealed.

**How big can a project be?**
File count isn't the limit: each file is hashed locally, and a file's proof grows by only one step
each time the number of files doubles. Size is: every stamp keeps its own compressed copy of the
files, so stamping large datasets often will grow the vault quickly. DraftStamp is built for
drafts (papers, manuscripts, code), not bulk data.

**Is SHA-256 going to last?**
The whole OpenTimestamps ecosystem rests on SHA-256 and Bitcoin. If SHA-256 were ever weakened,
proofs made before then would still be meaningful in the way older timestamps are, and existing
stamps could be re-stamped with a newer hash. That's a long way off.

**Does it work on Windows?**
CI builds and tests it on Windows and releases include a Windows binary, but it has been used on
macOS and Linux. Reports welcome.

## Project layout

```
crates/opentimestamps-lite   the OpenTimestamps subset: stamp, upgrade, check against Bitcoin
crates/draftstamp-core       commitments, receipts, bundles, the vault, history, sharing
crates/draftstamp-cli        the draftstamp command
spec/                        the formats, with test vectors
```

Libraries do no network I/O of their own: the command supplies HTTP, so every test runs offline
against a simulated network. Two opt-in tests go further: one has the official `ots` client check
DraftStamp's proofs, and one stamps through the real calendars and checks a real block
(`cargo test -p draftstamp-cli -- --ignored`).

## License

[Apache-2.0](LICENSE)
