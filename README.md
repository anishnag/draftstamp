# DraftStamp

**Prove your work existed, with a date no one can forge.**

DraftStamp timestamps your drafts in Bitcoin via [OpenTimestamps](https://opentimestamps.org).
Stamp a folder, and the exact files and their proof are kept together in a local vault. Share any
file later with a standard `.ots` that anyone can check, without DraftStamp and without seeing your
other files.

No account, no server, no fees. Only a hash leaves your machine.

## Quick start

```console
$ draftstamp stamp -m "first full draft"
Stamped 14 files of riemann-draft as 20261121T140312Z-e72b05.

$ draftstamp upgrade        # a few hours later
20261121T140312Z-e72b05  confirmed in Bitcoin block 976886 (2026-11-21 15:38 UTC)

$ draftstamp share chapters/chapter3.tex
Wrote chapter3.tex and chapter3.tex.ots from stamp 20261121T140312Z-e72b05.
```

The recipient checks it with the official client:

```console
$ ots verify -f chapter3.tex chapter3.tex.ots
Success! Bitcoin block 976886 attests existence as of 2026-11-21 UTC
```

Also: `log`, `verify`, `find`, `export`, and `share <folder> --zip`. See `draftstamp --help`.

## Install

**macOS (Apple Silicon)**

```sh
mkdir -p ~/.local/bin
curl -fsSL https://github.com/anishnag/draftstamp/releases/latest/download/draftstamp-aarch64-apple-darwin.tar.gz | tar -xz -C ~/.local/bin --strip-components=1 draftstamp-aarch64-apple-darwin/draftstamp
```

**macOS (Intel)**

```sh
mkdir -p ~/.local/bin
curl -fsSL https://github.com/anishnag/draftstamp/releases/latest/download/draftstamp-x86_64-apple-darwin.tar.gz | tar -xz -C ~/.local/bin --strip-components=1 draftstamp-x86_64-apple-darwin/draftstamp
```

**Linux (x86_64)**

```sh
mkdir -p ~/.local/bin
curl -fsSL https://github.com/anishnag/draftstamp/releases/latest/download/draftstamp-x86_64-unknown-linux-gnu.tar.gz | tar -xz -C ~/.local/bin --strip-components=1 draftstamp-x86_64-unknown-linux-gnu/draftstamp
```

**Linux (ARM64)**

```sh
mkdir -p ~/.local/bin
curl -fsSL https://github.com/anishnag/draftstamp/releases/latest/download/draftstamp-aarch64-unknown-linux-gnu.tar.gz | tar -xz -C ~/.local/bin --strip-components=1 draftstamp-aarch64-unknown-linux-gnu/draftstamp
```

**From source** (Rust 1.89+)

```sh
cargo install --locked --git https://github.com/anishnag/draftstamp draftstamp-cli
```

Make sure `~/.local/bin` is on your PATH. Windows builds and checksums are on the
[releases page](https://github.com/anishnag/draftstamp/releases/latest). To check proofs:
`pipx install opentimestamps-client`.

## How it works

Your files form a salted Merkle tree whose root is timestamped in Bitcoin through the
OpenTimestamps calendars. Each file's path through the tree is written as standard OpenTimestamps
steps, so every file gets its own `.ots`. The salts keep your other files hidden when you share
one. Formats are specified in [`spec/`](spec).

## FAQ

**Do I need Bitcoin?** No. The OpenTimestamps calendars are free and pay for the transactions.

**What does a stamp prove?** That these exact bytes existed by a Bitcoin block's time, give or take
a couple of hours. Possession at a time, not authorship.

**Why not just `ots stamp`?** DraftStamp's proofs work with the official client. It adds what
ongoing work needs: the exact files kept with each proof, folders stamped as a unit, project
history, and private sharing of any file or folder.

**What if I lose my vault?** You lose the proofs you haven't shared. Back up `~/.dsvault`.

More in the [FAQ](docs/FAQ.md).

## License

[Apache-2.0](LICENSE)
