# DraftStamp FAQ

- [The basics](#the-basics)
- [Verifying without DraftStamp](#verifying-without-draftstamp)
- [What is revealed, and to whom](#what-is-revealed-and-to-whom)
- [The vault, settings and ignores](#the-vault-settings-and-ignores)
- [Commands](#commands)
- [How it works](#how-it-works)

## The basics

**Do I need Bitcoin, a wallet or any money?**
No. OpenTimestamps calendars are free public services that batch many timestamps into a single
Bitcoin transaction they pay for. You never touch Bitcoin directly.

**What exactly does a stamp prove?**
That these exact bytes existed no later than the time of a specific Bitcoin block. It does not
prove who wrote them, or that they did not exist earlier. It is evidence of possession at a time,
which is what priority disputes usually turn on.

**How precise is the date?**
To within a couple of hours. A block's timestamp is set by its miner, and Bitcoin's rules keep it
within about two hours of real time. Proofs are usually confirmed within a few hours of stamping;
until then a stamp is "pending" and backed only by the calendars' promises. Run `draftstamp upgrade`
to complete pending proofs (`share` also tries before sharing).

**How is this different from running `ots stamp` myself?**
The official client is excellent, and DraftStamp's proofs work with it. What DraftStamp adds is
everything around the proof for ongoing work:

- it keeps the **exact files** with each proof, so you are never left with a `.ots` and a file that
  has since changed by a byte;
- it stamps **folders as a unit**, binding each file's path, and `check`/`verify` confirm later that
  the set is complete (nothing added or missing);
- **project history and search**: `log`, and `find` ("which drafts contained this exact file?");
- **upgrades** pending proofs on request and never overwrites anything;
- **selective sharing**: one file, a folder or any selection, each with a standalone `.ots`, salted
  so nothing about the other files leaks.

**Why not git commit dates, or emailing it to myself?**
Both are easy to fake after the fact: git dates are whatever your clock says and history can be
rewritten; an email's date is controlled by your provider and can't be independently checked. A
Bitcoin block can't be changed once it is buried under later blocks.

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
files, so stamping large datasets often will grow the vault quickly. DraftStamp is built for drafts
(papers, manuscripts, code), not bulk data.

**Is SHA-256 going to last?**
The whole OpenTimestamps ecosystem rests on SHA-256 and Bitcoin. If SHA-256 were ever weakened,
proofs made before then would still carry weight the way older timestamps do, and existing stamps
could be timestamped again under a newer hash. That is a long way off.

**Does it work on Windows?**
CI builds and tests it on Windows and releases include a Windows binary, but it has been used on
macOS and Linux. Reports welcome.

## Verifying without DraftStamp

| You have | Run | It proves |
|---|---|---|
| A shared file and its `.ots` | `ots verify -f file file.ots` | This exact file existed by the block's date |
| An export (`draftstamp export`) | `ots verify -f` on each file in `files/` | Every file, each on its own |
| A full bundle | The steps in [`spec/0001_core.md`](../spec/0001_core.md), then `ots verify -d <commitment> proof.ots` | The complete set: exactly these files, nothing missing or added |

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

## The vault, settings and ignores

The vault is `~/.dsvault` (or `--vault`, or `$DRAFTSTAMP_VAULT`). Each stamp is one folder:

```
~/.dsvault/20261121T140312Z-e72b05/
├── snapshot.zip    the exact files, as they were
├── receipt.json    paths, sizes, hashes and salts: how the files become the timestamped value
└── proof.ots       the OpenTimestamps proof of that value
```

Nothing in the vault is ever overwritten, except that a pending proof is replaced by its upgraded
version.

- **`.draftstamp`** (written by `draftstamp init`) names a folder's project, so you don't pass
  `--project` each time. It is never stamped.
- **`.draftstampignore`** uses gitignore-like patterns (`*.aux`, `build/`, `figures/raw/*`) to leave
  out build outputs. It is stamped, so the record shows what was excluded.
- `.git`, `.DS_Store` and similar are always skipped.

## Commands

| Command | What it does | Network |
|---|---|---|
| `draftstamp init [--project <name>]` | Write `.draftstamp` in this folder, naming its project | no |
| `draftstamp stamp [<folder> \| <file>] [-m <label>]` | Stamp a folder (this one by default) or a single file | calendars |
| `draftstamp upgrade [<stamp>]` | Complete pending proofs once Bitcoin confirms them | calendars, explorers |
| `draftstamp log [--project <name> \| --all]` | List stamps, newest first | no |
| `draftstamp find <file>` | Which stamps contain this file's exact bytes? | no |
| `draftstamp check [<stamp>]` | Is the stored record intact? | no |
| `draftstamp verify [<stamp>]` | Check, then confirm the date with Bitcoin | explorers |
| `draftstamp share <path>... [--zip <file>]` | Files or folders from a stamp, each with its own `.ots` | one upgrade attempt |
| `draftstamp export [<stamp>]` | The complete record as one zip | no |

A `<stamp>` is a full id, a unique prefix, or its 6-character ending (`e72b05`); left out, it means
the newest stamp of the current folder's project. `check` and `verify` exit with code 3 if anything
does not match. Full details: [`spec/0002_cli.md`](../spec/0002_cli.md).

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
   OpenTimestamps operations (prepend, append, sha256), followed by the stamp's proof. The result is
   an ordinary `.ots` whose digest is the file's own SHA-256, so the official client checks it with
   `ots verify -f`.

Specs: [`spec/0000_ots.md`](../spec/0000_ots.md) (the OpenTimestamps subset),
[`spec/0001_core.md`](../spec/0001_core.md) (commitments, receipts, bundles, sharing),
[`spec/0002_cli.md`](../spec/0002_cli.md) (the command), and [`ARCHITECTURE.md`](../ARCHITECTURE.md).

The libraries do no network I/O of their own: the command supplies HTTP, so every test runs offline
against a simulated network. Two opt-in tests go further: one has the official `ots` client check
DraftStamp's proofs, and one stamps through the real calendars and checks a real block
(`cargo test -p draftstamp-cli -- --ignored`).
