# Architecture

## Use cases

| Use | Surface | Built on |
| --- | --- | --- |
| Upload files or folders (including an Overleaf source zip). Hashing runs in the browser; nothing is uploaded. | Website | `draftstamp-wasm` |
| Stamp git repos (commit hooks, GitHub Actions), Overleaf projects through their git integration, or any local folder | CLI | `draftstamp-cli` |

## Crates

```
┌──────────────────────────┐            ┌──────────────────────────┐
│ web/          TypeScript │            │ draftstamp-cli      Rust │
│ picker, drop zone,       │            │ folder walk, git hook,   │
│ unzip, calendar fetch    │            │ writes .draftstamp/      │
└─────────────┬────────────┘            └─────────────┬────────────┘
              │                                       │
┌─────────────▼────────────┐                          │
│ draftstamp-wasm     Rust │                          │
└─────────────┬────────────┘                          │
              └───────────────────┬───────────────────┘
                                  │ both surfaces use both libraries
              ┌───────────────────┴───────────────────┐
┌─────────────▼────────────┐            ┌─────────────▼────────────┐
│ draftstamp-core     Rust │            │ draftstamp-ots      Rust │
│ format rules             │            │ proof format, status     │
│ no I/O                   │            │ no I/O; network flows    │
└──────────────────────────┘            │ use the host's Http      │
                                        │ (fetch or native client) │
                                        └──────────────────────────┘
```

| Crate | Purity | Role |
| --- | --- | --- |
| `draftstamp-core` | Pure: no files, no network | Format rules: canonical paths, salted leaves, Merkle tree, commitment, receipts, verification |
| `draftstamp-ots` | Pure: no I/O; its network flows call an `Http` interface the host supplies | OpenTimestamps proofs, stamping, upgrading, status links and live link status |
| `draftstamp-wasm` | No file access; implements ots's `Http` with the browser's `fetch` | Exposes core and ots to the website |
| `draftstamp-cli` | Does I/O: files and network; implements ots's `Http` natively | `stamp`, `verify`, `upgrade`, `status`, `hook install` (after launch) |

v1 runs no server: the website is a static page, and both surfaces talk directly to the public OpenTimestamps calendars and Bitcoin block explorers.

`draftstamp-core` and `draftstamp-ots` never depend on each other; the CLI and WASM crates combine them. Anything built later (extensions, plugins) sits on the CLI or WASM crate and never re-implements the format.

Hosts pass the core paths relative to the project root with `/` separators, and each file's exact bytes read as binary. The core decides everything else.
