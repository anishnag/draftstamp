# draftstamp-ots

Status: Draft · 2026-11-20

## Purpose

`draftstamp-ots` proves *when* a DraftStamp commitment existed. It takes the 32-byte commitment produced by `draftstamp-core`, timestamps it in Bitcoin through the public OpenTimestamps calendars, and produces a standard `.ots` proof that anyone can check with the official OpenTimestamps tools, without DraftStamp.

## Interface

**Input:** one 32-byte commitment, from `draftstamp-core`.

**Output:** one `.ots` file: a standard OpenTimestamps proof whose digest is that commitment. Status and the status link are derived from it on demand and never stored.

```rust
// The proof: OpenTimestamps concepts only, pure
impl Proof {
    pub fn from_bytes(bytes: &[u8]) -> Result<Proof, Error>;   // read a .ots file
    pub fn to_bytes(&self) -> Vec<u8>;                          // write a .ots file
    pub fn commitment(&self) -> [u8; 32];                       // what it proves
    pub fn status(&self) -> Status;                             // Pending | Anchored, from the file alone
    pub fn calendars(&self) -> Vec<CalendarEntry>;              // every (calendar, 44-byte message), answered or not
}

// The status link: DraftStamp's format
pub mod link {
    pub fn encode(calendars: &[CalendarEntry]) -> String;               // → https://draftstamp.org/status#v=1&cal=…
    pub fn decode(link: &str) -> Result<Vec<CalendarEntry>, Error>;
    pub async fn status(http: &impl Http, link: &str) -> Result<Status, Error>;   // live, read-only
}

// The flows that need the network
pub async fn stamp(http: &impl Http, commitment: [u8; 32]) -> Result<Proof, Error>;   // new pending proof
pub async fn upgrade(http: &impl Http, proof: &mut Proof) -> Result<Status, Error>;  // add Bitcoin paths, verify

// The only thing ots needs from a host
pub trait Http {
    async fn get(&self, url: &str) -> Result<Response, Error>;              // Response = { status, body }
    async fn post(&self, url: &str, body: &[u8]) -> Result<Response, Error>;
}
```

| Function | Who uses it | Network | Result |
| --- | --- | --- | --- |
| `stamp` | Whoever stamps | `POST` the blinded commitment to the aggregators | A pending proof |
| `upgrade` | The proof's owner | `GET` each pending message from its calendar; headers from both explorers | The proof gains its Bitcoin paths; returns `Anchored` once verified from the commitment |
| `link::status` | Anyone with a link | The same `GET`s and header checks | `Pending` or `Anchored`; nothing is saved |

Only 44-byte calendar messages and the blinded commitment `sha256(C ‖ nonce)` ever leave the host. The proof, commitment, nonce and files never do.

**Layering.** `Proof` holds only OpenTimestamps concepts and never changes with DraftStamp's choices. The `link` module holds DraftStamp's link format, and `CalendarEntry` is the value both produce: "ask this calendar about this message." `calendars()` returns every calendar the proof was submitted through, whether it has answered or not, so the link derived from it never changes. The flows contain every protocol rule (nonce, 2-of-4, merging, explorer agreement), so hosts cannot get them wrong.

**Hosts.** ots does no I/O. Each host implements `Http` and calls the same flows:

| Host | `Http` implemented with | Status |
| --- | --- | --- |
| `draftstamp-wasm` | The browser's `fetch`, passed in from JavaScript | Launch |
| `draftstamp-cli` | A native HTTP client | Later; no change to ots |
| Tests | Recorded calendar and explorer replies | Always |

ots does not depend on `draftstamp-core`, and core does not depend on it. The host passes the commitment in, saves the `.ots`, and checks `proof.commitment() == receipt.commitment()`.

## Dependencies

| Dependency | Used for | Notes |
| --- | --- | --- |
| **Our OpenTimestamps format module** (inside this crate) | Reading and writing `.ots` files and calendar replies; evaluating SHA-256, append and prepend; merging; checking block headers | About 350 lines, written against the reference implementation. We do not depend on the `opentimestamps` crate (0.2.0): it lacks merging and header checks, cannot parse Keccak ops, and has been unmaintained since 2023. |
| `sha2`, `getrandom` | SHA-256; the 16-byte nonce | Same crates as core |
| OpenTimestamps aggregators and calendars | Stamping and upgrading | Public, run by the OpenTimestamps community |
| Block explorers `blockstream.info/api`, `mempool.space/api` | Block headers | Both must agree |
| Test only: official `ots` client, `opentimestamps` crate | Fixtures and cross-checks | Never shipped |

No HTTP library, no DraftStamp server.

## Layout

```
crates/draftstamp-ots/
├── Cargo.toml
├── src/
│   ├── lib.rs              public surface: Proof, Status, CalendarEntry, Error, Http, stamp, upgrade, link
│   ├── proof.rs            Proof: from_bytes, to_bytes, commitment, status, calendars
│   ├── error.rs            Error with stable code() strings
│   ├── http.rs             the Http trait and Response
│   ├── flows.rs            stamp and upgrade; shared calendar and explorer steps
│   ├── link.rs             encode, decode, status; the frozen calendar name table
│   ├── servers.rs          aggregators, calendar allowlist, explorers
│   ├── bitcoin.rs          check a message against an 80-byte block header
│   └── format/             the OpenTimestamps format
│       ├── encoding.rs     varuint, varbytes
│       ├── op.rs           op tags, evaluating SHA-256 / append / prepend, ordering
│       ├── attestation.rs  Bitcoin, Pending, preserved unknowns
│       └── tree.rs         timestamp tree: read, write, merge, walk
└── tests/
    ├── fixtures/           .ots files from the official client, recorded calendar and explorer replies
    ├── proofs.rs           round trips, status, cross-check with the opentimestamps crate
    └── flows.rs            stamp, upgrade and link::status against a recorded-reply Http
```

`format/`, `bitcoin.rs` and `servers.rs` are private; their own tests are unit tests in the same files.

## How it works

A `.ots` file is a recipe: start from the commitment, apply its operations, and arrive at a value recorded in Bitcoin. Stamping writes the first part; upgrading fetches the rest.

### `stamp`

1. **Hide the commitment.** Pick 16 random bytes `N` and compute `M = sha256(C ‖ N)`: in the recipe, `append N`, then `sha256`. Calendars only ever see `M`.
2. **Submit.** `POST M` to all four aggregators in parallel. Each aggregator adds its own 16-byte nonce to every submission (`append`, `sha256`), combines the batch in a Merkle tree where each level is `sha256(left ‖ right)`, and passes the root to a backend calendar. The calendar prepends a 4-byte time and appends an 8-byte MAC, giving a 44-byte message it promises to put in Bitcoin.
3. **Collect replies.** Each reply is the recipe from `M` to that message, written with the same three ops: the aggregator's `append` nonce and `sha256`, then for each Merkle level an `append` or `prepend` of the sibling hash and a `sha256`, then the calendar's `prepend` and `append`, ending in a pending attestation naming the calendar. At least two within 5 seconds, or no proof.
4. **Assemble** every reply under the `M` node:

   ```
   C
   └─ append N
      └─ sha256                                   → M
         ├─ aggregator A's branch → 44-byte message → Pending(alice…)
         └─ aggregator B's branch → 44-byte message → Pending(bob…)
   ```

5. **Return the proof.** `to_bytes()` writes magic, `01`, `08`, `C`, then the tree. This is a valid pending `.ots`.

### `upgrade`

6. **Ask each calendar** in `proof.calendars()` that has not yet answered: `GET {calendar}/timestamp/{hex(message)}`. `404` means not anchored yet (normal for 1–6 hours).
7. **Merge each reply** below its pending node; merging only adds, so the pending attestation stays (the official `ots upgrade` behaves the same). The reply continues the recipe through the calendar's Merkle tree, into the Bitcoin transaction carrying its root, `sha256` twice, then through the block's transaction tree, ending in a Bitcoin attestation with the block height.
8. **Verify.** Replay the recipe from `C` to the Bitcoin attestation. The result must equal the Merkle root in that block's header (bytes 36–67), fetched from both explorers. The block time (bytes 68–71) is the proven time. Returns `Anchored` only if this holds.

### `link::status`

Steps 6 and 8, starting from each message in the link instead of from `C`, and saving nothing. It proves that the stamp is anchored, not which commitment it covers.

## Status link

`link::encode(&proof.calendars())` writes each calendar and its 44-byte message as parameters in the URL fragment. Upgrades leave pending attestations in place (ours, and the official `ots upgrade`), so a proof gives the same link before and after it completes. The link cannot be turned back into the `.ots`.

```
https://draftstamp.org/status#v=1&cal=alice:<message>&cal=bob:<message>
```

- **Fragment, not query.** Everything after `#` stays in the browser; the hosting server never sees which stamp is checked.
- **`v=1`** is the version. Readers ignore parameters they do not know.
- **One `cal=<calendar>:<message>` per calendar**, sorted by calendar. `<message>` is base64url without padding (59 characters).
- **`<calendar>`** is a short name from the table below or a full host. `https://` is implied; the host must be on the allowlist.

About 170 characters with two calendars, 320 with four.

**The message** is the calendar's lookup key: 4 bytes of time (Unix seconds, big-endian), the aggregator's 32-byte batch root, and an 8-byte MAC from the calendar's secret key.

**The link is public.** It reveals the stamp time and lets anyone check status. It reveals nothing about the files or commitment (reaching the commitment needs the nonce and aggregator path, which exist only in the `.ots`), and it cannot be used to alter a proof. It is a stable identifier: several shared links show how many stamps were made and when.

| Item | Sensitivity |
| --- | --- |
| Files | private |
| Receipt (paths, hashes, salts) | private: allows testing guesses about the files |
| `.ots` proof | low: contains the commitment, which reveals nothing without the receipt |
| Status link | public |

**Calendar names** are a constant in `link.rs` and part of the link format. The table is **frozen when v1 ships and never extended**, so every v1 reader knows every name a v1 link can contain; later calendars are written as full hosts. A retired calendar keeps its name.

| Name | Host |
| --- | --- |
| `alice` | `alice.btc.calendar.opentimestamps.org` **(confirm)** |
| `bob` | `bob.btc.calendar.opentimestamps.org` **(confirm)** |
| `finney` | `finney.calendar.eternitywall.com` **(confirm)** |
| `catallaxy` | `btc.calendar.catallaxy.com` **(confirm)** |

A link format that needs a different table is `v=2`; v1 links keep working.

## Rules

- Status and the link are never stored; both come from the `.ots`.
- Upgrades only add; pending attestations are never removed. Hosts upgrade silently whenever they can and save by writing a temporary file, then renaming.
- A proof whose pending attestations were removed (the official `ots prune`, run deliberately, discards them all by default) has no calendar entries and so no link. It still verifies; hosts show verification instead of a link.
- Only allowlisted calendars are contacted, never a URI just because a proof or link names it.
- `.ots` files (≤ 64 KiB) and replies (≤ 10,000 bytes) are untrusted: parsed with the limits below, never panicking.
- A branch needing an op we do not evaluate is unverifiable, never valid. `Anchored` requires both explorers to agree.

## Reference: OpenTimestamps

The format has no standards document; the official implementation defines it: [python-opentimestamps](https://github.com/opentimestamps/python-opentimestamps), [opentimestamps-server](https://github.com/opentimestamps/opentimestamps-server), [opentimestamps-client](https://github.com/opentimestamps/opentimestamps-client). Checked against their source on 2026-11-20; **(confirm)** marks items a real proof must settle.

**File**

```
magic     00 "OpenTimestamps" 00 00 "Proof" 00 bf 89 e2 e8 84 e8 92 94   (31 bytes)
version   1 byte: 0x01
hash op   1 byte: 0x08 (SHA-256)
digest    32 bytes
tree      timestamp tree from the digest; no trailing bytes
```

`varuint`: unsigned LEB128, ≤ 9 bytes (read non-minimal, write minimal). `varbytes`: `varuint` length, then bytes.

**Tree.** A node has a set of attestations and a map from op to child. Write attestations then ops, each sorted; an attestation is `0x00` + attestation; an op is the op then its child; every item but the node's last is preceded by `0xff`. Read by looping on `0xff`; reject duplicate ops; at most 256 levels; messages ≤ 4096 bytes.

| Op | Tag | Argument | We |
| --- | --- | --- | --- |
| SHA-256 | `08` | none | evaluate |
| append, prepend | `f0`, `f1` | varbytes, 1–4096 bytes | evaluate |
| SHA-1, RIPEMD-160, Keccak-256, reverse, hexlify | `02`, `03`, `67`, `f2`, `f3` | none | preserve only |

| Attestation | Tag | Payload (varbytes ≤ 8192, consumed exactly) |
| --- | --- | --- |
| Bitcoin | `0588960d73d71901` | varuint height ≤ 2³²−1 |
| Pending | `83dfe30d2ef90c8e` | URI ≤ 1000 bytes of `[A-Za-z0-9-._/:]` |
| Other (e.g. Litecoin) | any | preserve, never trust |

Ops sort by tag, then argument. Attestations sort by tag, then height or URI.

**Network**

| Call | Request | Response |
| --- | --- | --- |
| Submit | `POST {aggregator}/digest`, raw 32-byte body, `Accept: application/vnd.opentimestamps.v1` | `200`: tree fragment from the submitted bytes |
| Upgrade | `GET {calendar}/timestamp/{lowercase hex}` | `200`: fragment to a Bitcoin attestation; `404`: not yet |
| Header | `GET {explorer}/block-height/{h}`, then `GET {explorer}/block/{hash}/header` | block hash; 80-byte header as hex |

| Servers | Hosts (HTTPS) |
| --- | --- |
| Aggregators | `a.pool.opentimestamps.org`, `b.pool.opentimestamps.org`, `a.pool.eternitywall.com`, `ots.btc.catallaxy.com` |
| Calendars (allowlist) | `*.calendar.opentimestamps.org`, `*.calendar.eternitywall.com`, `*.calendar.catallaxy.com` |

A calendar prepends a 4-byte big-endian time and appends an 8-byte MAC before its pending attestation, so pending messages are 44 bytes. A Bitcoin attestation holds when its message equals header bytes 36–67 (raw byte order, **confirm**); the time is header bytes 68–71, little-endian.

## Tests

1. Real proofs from `ots stamp` and `ots upgrade` round-trip byte for byte; commitments match `ots info`.
2. Our proofs pass `ots info`, `ots upgrade` and `ots verify`.
3. Our parser and the `opentimestamps` crate read every fixture identically.
4. The header check matches stored real headers (settles byte order).
5. Recorded aggregator replies confirm the calendar name table and the 44-byte messages.
6. `stamp`, `upgrade` and `link::status` run against a recorded-reply `Http`, including one aggregator down, `404`s, explorers disagreeing and malformed replies.
7. Fuzzed parsing of proofs, replies and links never panics.

## Open

**CORS.** The reference calendar server sends no CORS headers, and the explorers do not document theirs. Probe them before building the website. If browsers are blocked, the website's `Http` routes through a stateless pass-through that only adds CORS headers; it stores nothing, links never depend on it, and ots does not change.
