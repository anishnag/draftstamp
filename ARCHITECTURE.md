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
│ no I/O                   │            │ no I/O by default;       │
└──────────────────────────┘            │ "client" feature adds    │
                                        │ calendar HTTP (CLI only) │
                                        └──────────────────────────┘
```

| Crate | Purity | Role |
| --- | --- | --- |
| `draftstamp-core` | Pure: no files, no network | Format rules: canonical paths, salted leaves, Merkle tree, commitment, receipts, verification |
| `draftstamp-ots` | Pure by default; the `client` feature adds network calls | OpenTimestamps proof format and status; with `client`, calendar submission and upgrade |
| `draftstamp-wasm` | Pure | Exposes core and pure ots to the website |
| `draftstamp-cli` | Does I/O: files and network | `stamp`, `verify`, `upgrade`, `status`, `hook install`; enables ots `client` |

`draftstamp-core` and `draftstamp-ots` never depend on each other; the CLI and WASM crates combine them. Anything built later (extensions, plugins) sits on the CLI or WASM crate and never re-implements the format.

Hosts pass the core paths relative to the project root with `/` separators, and each file's exact bytes read as binary. The core decides everything else.
