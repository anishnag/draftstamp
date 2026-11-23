# DraftStamp

**Prove your work existed, with a date no one can forge.**

DraftStamp timestamps your drafts in Bitcoin. Stamp a folder (a paper, a thesis, a manuscript, some
code) and it records a fingerprint of those exact files through
[OpenTimestamps](https://opentimestamps.org), keeping the files and their proof together in a local
vault.

Later, share any single file with a standard `.ots` proof. Anyone can check it with the official
OpenTimestamps client, without DraftStamp and without seeing your other files.

No account, no server, no fees. Only a 32-byte hash leaves your machine.

## Quick start

```console
$ cd ~/papers/riemann-draft
$ draftstamp stamp -m "first full draft"
Stamped 14 files of riemann-draft as 20261121T140312Z-e72b05.

$ draftstamp upgrade            # a few hours later, once Bitcoin confirms
20261121T140312Z-e72b05  confirmed in Bitcoin block 976886 (2026-11-21 15:38 UTC)

$ draftstamp share chapters/chapter3.tex
Wrote chapter3.tex and chapter3.tex.ots from stamp 20261121T140312Z-e72b05.
```

Whoever receives the file checks it with the official client:

```console
$ ots verify -f chapter3.tex chapter3.tex.ots
Success! Bitcoin block 976886 attests existence as of 2026-11-21 UTC
```

Other commands: `log` (your stamps), `verify` (re-check a stamp against Bitcoin), `find` (which
stamps contain this file?), `share chapters/ --zip out.zip` (a folder or selection), `export` (the
complete record). Run `draftstamp --help` for the rest.

## Install

```sh
# macOS (Apple Silicon; for Intel use x86_64-apple-darwin)
mkdir -p ~/.local/bin
curl -fsSL https://github.com/anishnag/draftstamp/releases/latest/download/draftstamp-aarch64-apple-darwin.tar.gz | tar -xz
mv draftstamp-aarch64-apple-darwin/draftstamp ~/.local/bin/

# Linux (x86_64; for ARM use aarch64-unknown-linux-gnu)
mkdir -p ~/.local/bin
curl -fsSL https://github.com/anishnag/draftstamp/releases/latest/download/draftstamp-x86_64-unknown-linux-gnu.tar.gz | tar -xz
mv draftstamp-x86_64-unknown-linux-gnu/draftstamp ~/.local/bin/

# From source (Rust 1.89+)
cargo install --locked --git https://github.com/anishnag/draftstamp draftstamp-cli
```

No `sudo` needed. If `draftstamp` isn't found afterwards, add `~/.local/bin` to your PATH (on macOS:
`echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc`, then open a new terminal). Windows and
checksums are on the [releases page](https://github.com/anishnag/draftstamp/releases/latest).
To check proofs, install the official client: `pipx install opentimestamps-client`.

## How it works

Your files become a salted Merkle tree, and its root is timestamped through the public
OpenTimestamps calendars, which commit it to a Bitcoin block. Each file's path through the tree is
written as standard OpenTimestamps steps, so every file gets its own ordinary `.ots`. Salts keep the
other files' names and contents hidden when you share one.

The formats are specified in [`spec/`](spec), with test vectors from an independent implementation.

## FAQ

**Do I need Bitcoin or a wallet?** No. The OpenTimestamps calendars are free public services that
pay for the transactions.

**What does a stamp prove?** That these exact bytes existed by the time of a Bitcoin block (accurate
to a couple of hours). It shows possession at a time, not authorship.

**Why not just `ots stamp`?** DraftStamp speaks the same protocol, and its proofs work with the
official client. It adds what ongoing work needs: the exact files kept with each proof, folders stamped
as a unit, project history, and sharing any file or selection privately.

**What if I lose my vault?** You lose the proofs you hadn't shared. Back up `~/.dsvault`.

More in the [FAQ](docs/FAQ.md): what is revealed to whom, verifying without DraftStamp, precision,
longevity, and the vault.

## License

[Apache-2.0](LICENSE)
