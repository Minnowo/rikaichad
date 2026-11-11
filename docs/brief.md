# Project brief: rikaichad

Status: approved (2026-11-10)
Phases done: 1-7 (repository initialized 2026-11-10)

The single source of truth for what this project is and why. Decisions are
recorded with their reasons so they can be revisited with full context.

## 1. Idea

- **Pitch:** A Japanese dictionary engine written in Rust that reimplements
  Yomitan's lookup behaviour (https://github.com/yomidevs/yomitan) and reads
  Yomitan-format dictionaries, exposed as a CLI and a local API so other
  tools (editors, video players, readers, a future web UI) can do
  Yomitan-quality lookups outside the browser.
- **User:** the public; released as open source. Primary consumers are
  people who type a command and developers/tools that call the API.
- **Problem:** Yomitan's engine (dictionary import, deinflection, term
  scanning, ranking) is locked inside a browser extension. Tools outside the
  browser cannot reuse it or its dictionary ecosystem.
- **v1 is done when:** (draft, refined in phase 6) a user can import a
  Yomitan dictionary zip (e.g. JMdict/Jitendex), then from the CLI and from
  the local API look up inflected Japanese text and get the dictionary form,
  readings, glosses and the matched length, with results comparable to
  Yomitan for the same dictionaries.
- **Hard constraints:**
  - Rust.
  - Interfaces for now: CLI and HTTP API (local by default, bindable anywhere). A web UI may come later, so
    the API must be enough to build one. No native GUI, no OS popup.
  - License: AGPL-3.0-or-later (user preference; GPL-3.0 acceptable).
    Porting Yomitan code (GPL-3.0-or-later) is allowed, see section 3.
  - Public project: install story, docs and stable data formats matter.
- **Ambiguous terms:**
  - "dictionary program" -> engine + CLI + local API (user decision).
  - "based on Yomitan" -> same behaviour and same dictionary format.
    Initially "reimplement, own code"; revised in phase 3 once the license
    became AGPL: Yomitan's deinflection rules are ported; scope of further
    porting decided in phase 3.
  - "local API" -> HTTP/JSON on localhost (phase 3 decision).
  - "Japanese dictionary" scope (kanji, frequency, pitch, audio, Anki) ->
    decided in phase 4.

## 2. Landscape

Researched 2026-11-10 by reading each project's README/docs (not code).
Dates and star counts are as shown on the pages that day.

### Existing software

| Name                    | Shape                                                        | Good at                                                         | Why not (for us)                                                       | License          |
|-------------------------|--------------------------------------------------------------|-----------------------------------------------------------------|------------------------------------------------------------------------|------------------|
| Yomitan                 | Browser extension (JS), dictionaries in IndexedDB            | The reference: format, deinflection, ranking, Anki, audio       | Browser only; we port its lookup core (license compatible, sec 3)      | GPL-3.0-or-later |
| yomitan-api             | Python native-messaging host, HTTP 127.0.0.1:19633, POST     | Already gives Yomitan lookups to outside tools (Yomine, MCP)    | Needs a running browser + Yomitan + Python; response format unstable   | GPL-3.0          |
| jidoujisho              | Flutter Android app; Isar/Hive storage                       | Imports Yomichan dicts with freq and pitch; reader/player/Anki  | Archived 2026-10-10; README says structured-content unsupported        | GPL-3.0          |
| hoshidicts              | C++ library for Hoshi Reader; per-dict folder, zstd glosses  | Yomitan import + query, styles, media; lookup like Yomitan      | C++; partial pitch spec; deinflection not validated against dict POS   | GPL-3.0 (MIT branch exists) |
| hoshidicts-rs           | Rust port of hoshidicts; mmap + custom binary + bloom        | Term/kanji/freq/pitch, deconjugation (Jiten rules), CLI         | 1 commit, 0 stars; no structured content mentioned                     | MIT              |
| yomichan_rs             | Rust library "based off Yomitan"; native_db + rusqlite       | Closest to a full Yomitan port, multi-language deinflector      | Self-described unstable; GPL; 4 stars                                  | GPL-3.0          |
| 10ten Japanese Reader   | Browser extension (TS); jpdict-idb, own JMdict pipeline      | Mature hover lookup; widely used deinflection rules             | Own data format, not Yomitan dicts; browser only                       | GPL-3.0          |
| tentoku (Rust)          | Library + CLI + FFI; JMdict XML -> SQLite (~105 MB)          | 241 deinflection rules (10ten-derived), POS validation          | JMdict only, no Yomitan dicts; GPL                                     | GPL-3.0-or-later |
| jmdict-fast + bunpo     | Rust library; JMdict -> mmap FST indexes; bunpo deinflector  | Very fast lookup, MIT deinflector with no deps                  | JMdict only, no Yomitan dicts                                          | MIT              |
| ichiran                 | Common Lisp CLI + web; JMdict in PostgreSQL (~4.7 GB)        | Sentence segmentation with glosses, romanization               | Heavy (Postgres), not Yomitan dicts                                    | MIT              |
| Sudachi.rs / Lindera    | Rust morphological analyzers (tokenizers)                    | Proper word segmentation, lemma, POS                            | Not dictionaries; large dictionary data; would be an optional layer    | Apache-2.0 / MIT |
| wty                     | Rust CLI/lib, Wiktionary (Kaikki) -> Yomitan zips            | Shows the format is produced by Rust tools; test dict source    | Writer, not reader                                                     | MIT              |

Dictionaries users will import (data, not software): Jitendex (JMdict-based,
CC BY-SA 4.0, structured-content heavy), JMdict/KANJIDIC conversions,
frequency lists (e.g. JPDB, BCCWJ), pitch accent (e.g. Kanjium-derived).

### Standards and references

| Standard / spec                | What it covers                                       | Local copy                                   | Version         |
|--------------------------------|------------------------------------------------------|----------------------------------------------|-----------------|
| Yomitan dictionary schemas v3  | index, term, term meta (freq/pitch/ipa), kanji, tags | docs/reference/yomitan-schemas/              | yomitan@ee81f8d |
| Making Yomitan dictionaries    | Zip layout, styles.css, images, field semantics      | docs/reference/making-yomitan-dictionaries.md| yomitan@ee81f8d |
| yomitan-api endpoints          | /termEntries, /kanjiEntries, /tokenize, /ankiFields  | docs/reference/yomitan-api/                  | yomitan-api@88b9d85 |
| JMdict / EDRDG licence         | Upstream of most Japanese dicts; CC BY-SA 4.0        | not yet (needed only for attribution text)   | -               |
| Zip (PKWARE APPNOTE), JSON     | Container and encoding of dictionaries               | via crates                                   | -               |

Key format facts (from the schemas):

- A dictionary is a zip with `index.json` (title, revision, format 3) plus
  numbered `term_bank_N.json`, `term_meta_bank_N.json`, `kanji_bank_N.json`,
  `kanji_meta_bank_N.json`, `tag_bank_N.json`, optional `styles.css` and
  images referenced by path.
- Term entry = 8-item array: term, reading, definition tags, rule ids (POS
  conditions used to validate deinflection, e.g. v1, v5, vs, adj-i), score,
  glossary, sequence, term tags.
- Glossary items: string, text, structured-content (an HTML-like tree:
  ruby, table, div, span, ul/ol, details, img, a with internal `?` links,
  plus a restricted style object), image, or a deinflection reference.
- Term meta: freq (number / string / {value, displayValue}, optionally per
  reading), pitch (positions, nasal, devoice), ipa.

### Architecture patterns

- **Browser-backed bridge** (yomitan-api): a local server forwards to the
  real extension. Pros: exact Yomitan results. Cons: browser must run.
- **Import to own store, query in-process** (hoshidicts, yomichan_rs,
  Yomitan itself): parse zips once into a database/index, then fast
  lookups. Pros: headless, fast. Cons: own engine must match behaviour.
  Sub-choices: embedded SQL (SQLite) vs custom mmap files (hoshidicts-rs).
- **Scan-based lookup** (Yomitan, 10ten, hoshidicts): take the text at a
  position, try progressively shorter prefixes, deinflect each with rules,
  keep candidates whose rule conditions match the dictionary entry.
- **Tokenizer-based lookup** (Sudachi/Lindera/MeCab, ichiran): segment the
  sentence first, then look up lemmas. Better for whole-sentence parsing;
  needs a large tokenizer dictionary. Yomitan offers MeCab as optional.

### Build vs use

- If you only need Yomitan results outside the browser and can keep a
  browser open, yomitan-api already does that today.
- The gap we fill: a headless, single-binary engine (no browser, no
  Python) that reads the same dictionaries, with a stable documented API.
- hoshidicts-rs (MIT) is the closest existing Rust project but is a
  1-commit, unmaintained-looking port; yomichan_rs is GPL and unstable.
  Neither handles structured-content per their READMEs. Building our own
  is justified; both are useful as behaviour comparisons only.

### Kit research applied

- Lessons from the storytold craft apps' agent files
  (https://github.com/storytold/filmcraft and siblings, read via the
  bootstrap kit's notes, 2026-10-14) for product clones: study the
  target by behaviour and public docs; implement formats from specs; use
  the real product as a test oracle (the "no GPL code" part does not apply:
  we are AGPL and port Yomitan deliberately, sec 3); parity
  tracking via a measured gaps list; real-file test corpora fetched at
  pinned versions and sha256-verified, never committed.

## 3. Architecture decisions

| Dimension    | Decision                                                      | Options considered                                   | Reason                                                                 | By      |
|--------------|---------------------------------------------------------------|------------------------------------------------------|------------------------------------------------------------------------|---------|
| Placement    | One local process on the user's machine                       | -                                                    | Single user, offline dictionary tool                                   | default |
| Interfaces   | CLI + HTTP/JSON API on localhost; web UI later on same server | HTTP, stdio JSON-lines, both                         | Callable from any language and from a browser; web UI can reuse it     | user    |
| API compat   | Own stable, documented API; no yomitan-api compatibility      | own only, compat in v1, compat later                 | Upstream format is large and declared unstable                         | user    |
| Data in      | Yomitan dictionary zips (format 3) imported by the user       | Yomitan zips, raw JMdict XML                         | The whole dictionary ecosystem uses it; jidoujisho-style support       | user    |
| License      | AGPL-3.0-or-later; ported Yomitan files stay                  | AGPL, GPL, MIT/Apache                                | User preference. GPLv3 s.13 and AGPLv3 s.13 permit combining GPLv3     | user    |
|              | GPL-3.0-or-later with Yomitan copyright headers               |                                                      | and AGPLv3 code (read in docs/reference/{gpl,agpl}-3.0.txt)            |         |
| Deinflection | Port Yomitan's language transform rules                       | own rules, Jiten/bunpo rules, bunpo crate, Yomitan   | Matches Yomitan results exactly and keys on the same condition ids     | user    |
|              |                                                               |                                                      | that dictionaries store in the term entry rules field                  |         |
| Platforms    | Linux, macOS, Windows; CI on all three from milestone 1       | Linux only, all three                                | Public CLI tool; nothing platform-specific (std paths, SQLite, tokio), | default |
|              |                                                               |                                                      | so the cost is CI time; catching path/encoding bugs early is cheaper   |         |
| Scale        | One user; ~100 dictionaries, ~3 GB compressed, 10M+ term rows,| -                                                    | Measured on the user's real collection (docs/dictionary-survey.md);    | default |
|              | single dictionaries up to 3.5 GB uncompressed                 |                                                      | heavy monolingual users are the target, not a Jitendex-only setup      |         |
| Port scope   | Port Yomitan's lookup core: Japanese transforms, text         | rules only, lookup core, as much as possible         | Results match Yomitan; storage/import/API/CLI stay idiomatic Rust      | user    |
|              | preprocessors, prefix scanning, candidate validation,         |                                                      |                                                                        |         |
|              | ranking/sorting, sequence merging. Own design elsewhere       |                                                      |                                                                        |         |
| Languages    | Japanese only, not pluggable                                  | ja pluggable, ja only, several                       | Simplest; add a language layer when a second language is real          | user    |
| Data out     | API: raw structured-content JSON + plain-text rendering;      | raw+text, raw+text+HTML, text only                   | Clients render; HTML comes with the web UI                             | user    |
|              | CLI: plain text, plus `--json` with the API's JSON            |                                                      |                                                                        |         |
| Security     | No auth. Default bind 127.0.0.1; `--bind <addr:port>` lets    | +token auth, configurable bind                       | User decision 2026-11-10: auth not wanted; users import once and then  | user    |
|              | the user expose it anywhere (LAN, all interfaces).            |                                                      | serve read-only almost always, which removes the mutation risk.        |         |
|              | `--read-only`: store opened read-only and every mutating      |                                                      | Cross-origin default kept so a web page in the user's browser cannot   | default |
|              | endpoint (import, delete, reorder, settings) returns 403.     |                                                      | drive the API; allowlist origins in config                             |         |
|              | Cross-origin browser requests rejected unless allowlisted     |                                                      |                                                                        |         |
|              | (Origin check); Host header allowlist against DNS rebinding;  |                                                      | Review H4: Origin alone misses DNS rebinding and simple POSTs          | default |
|              | POST bodies must be application/json (import: zip upload);    |                                                      |                                                                        |         |
|              | caps on body size, text length, results, response size;       |                                                      |                                                                        |         |
|              | request timeouts; one import at a time; import by upload      |                                                      |                                                                        |         |
|              | only (never a server-side path)                               |                                                      |                                                                        |         |
| Concurrency  | One writer at a time via a lock file in the data dir; `serve`  | immutable=1, SQLITE_OPEN_READONLY, API-level guard   | Review H3: a WAL database opened read-only needs a writable -shm;      | default |
|              | and CLI imports may run in parallel (WAL readers). The store  |                                                      | `--read-only` is about what clients may do, not the filesystem.        |         |
|              | is opened normally; `--read-only` is an API-level guard plus  |                                                      | Truly read-only media (immutable mode) is "later"                      |         |
|              | `PRAGMA query_only` on serve connections                      |                                                      |                                                                        |         |
| Bind default | Writable unless `--read-only`, on any bind address; startup   | read-only by default off loopback                    | User decision 2026-11-10 after review H4                               | user    |
|              | warning when off-loopback and writable                        |                                                      |                                                                        |         |
| State        | Imported dictionaries in a local store in the platform data   | read zips at query time, own store                   | Zips are slow to scan; import once, query fast. Engine in phase 5      | default |
|              | dir (XDG on Linux); store format versioned, re-import on bump |                                                      |                                                                        |         |
| Failure      | Import is all-or-nothing (no half-imported dict visible);     | -                                                    | A crash mid-import must not corrupt lookups                            | default |
|              | lookups keep working during an import; bad dicts are rejected |                                                      |                                                                        |         |
|              | with the schema path of the first error                       |                                                      |                                                                        |         |
| Config       | Zero-config defaults; CLI flags; JSON config file written by  | flags, env, TOML file, JSON file                     | Dictionary order/enabled and allowed origins need persisting; JSON     | default |
|              | the CLI (hand-editable)                                       |                                                      | reuses serde_json, no TOML dependency                                  |         |
|              | for dictionary order/enabled flags, port, allowed origins     |                                                      |                                                                        |         |
| Distribution | `cargo install --git` plus prebuilt binaries on GitHub        | crates.io, distro packages                           | One binary, no runtime deps; crates.io blocked by the name (sec 5);    | default |
|              | releases                                                      |                                                      | releasing needs user approval each time                                |         |

Note: the phase 1 "clean-room" constraint is withdrawn. Ported code must
keep Yomitan's copyright notice and license header and be listed in a
NOTICE file with the upstream commit it came from.

### Non-goals (v1)

- No native GUI, OS popup, OCR, clipboard watching or text hooking.
- No yomitan-api compatibility layer.
- No languages other than Japanese.
- No network access at runtime (no dictionary downloads, no online audio).
- No HTML rendering of glossaries (comes with the web UI).

### Diagram

```
 dictionary.zip --> [import] --> local store (data dir)
                                      |
 text --> [preprocess] --> [scan prefixes] --> [deinflect] --> [lookup]
                                                                  |
                                       [validate conditions, rank, merge]
                                                                  |
                                 +-------------------+------------+
                                 |                   |
                           CLI (text/--json)   HTTP JSON (--bind, default 127.0.0.1)
```

## 4. Unknowns surfaced

Accepted by the user 2026-11-10 (no changes requested).

| #  | Item                                       | Why it matters                                                          | Decision            |
|----|--------------------------------------------|-------------------------------------------------------------------------|---------------------|
| 1  | Dictionary order, enable/disable           | Yomitan ranks/groups results by dictionary priority                     | v1                  |
| 2  | Frequency dictionaries in results/sorting  | Users choose readings/senses by frequency                               | v1                  |
| 3  | Pitch accent and IPA output                | Part of the format; data passthrough                                    | v1                  |
| 4  | Kanji lookup (kanji + kanji meta banks)    | Part of the format, cheap                                               | v1                  |
| 5  | Sentence parsing (repeated scan)           | Lets tools annotate a whole line                                        | v1                  |
| 6  | Hostile dictionary files                   | Zip bombs, path traversal, huge JSON: caps + fuzzing                    | v1                  |
| 7  | Attribution in API/CLI                     | CC BY-SA dictionaries carry attribution in index.json                   | v1                  |
| 8  | Re-import / duplicate titles / revisions   | Must have defined behaviour                                             | v1                  |
| 9  | Concurrent import and lookup               | serve while import runs                                                 | v1                  |
| 10 | AGPL source offer (`/source` link)         | AGPLv3 s.13 for modified network versions                               | v1                  |
| 11 | Media (images, audio, pdf, other files)    | User 2026-11-10: "only really care about text". Media entries are       | never: skipped at   |
|    |                                            | skipped at import without decompressing; img nodes stay in the raw      | import              |
|    |                                            | structured-content JSON and render as alt/title text or "[image]";      |                     |
|    |                                            | image glossaries render their alt/description text or are dropped       |                     |
|    |                                            | when they have none. styles.css (small text) is kept for a web UI       |                     |
| 12 | Romaji input conversion                    | Comes with the port (alphabeticToHiragana preprocessor); CLI users     | v1 (review M6)      |
|    |                                            | without an IME need it most                                             |                     |
| 13 | Wildcard/prefix and reverse search         | Common in dictionary apps                                               | later               |
| 14 | Dictionary auto-update (isUpdatable)       | Needs network                                                           | later               |
| 15 | Anki export                                | UI and network bound                                                    | later               |
| 15b| Audio                                      | Not text (user decision)                                                | never               |
| 16 | Dictionary formats 1 and 2                 | Rare today                                                              | never               |
| 17 | MeCab/Sudachi tokenizer mode               | Optional in Yomitan                                                     | later               |
| 19 | Kana-insensitive search                    | Comes with the port (hiragana/katakana preprocessor)                    | v1                  |
| 20 | frequencyMode rank-based vs occurrence     | Changes sort direction for frequency ordering                           | v1                  |
| 21 | index.json validation (format, minimum     | Reject format != 3; warn on minimumYomitanVersion; reject a             | v1                  |
|    | version, sourceLanguage)                   | sourceLanguage other than ja; ignore unknown keys (`weight`)            |                     |
| 22 | Plain-text rendering of img and a nodes    | img -> alt/title text or "[image]"; a -> its content                    | v1                  |
| 23 | Import order/enabled from Yomitan settings | Convenient migration for existing users                                 | later               |
| 24 | Zip entries with a wrong CRC-32            | 8 of the user's 12 pitch dicts; Yomitan never checks CRCs. Accept if    | v1                  |
|    |                                            | the entry inflates to its declared size; warn with the entry name       |                     |
| 25 | Junk entries (`__MACOSX/`, nested names)   | Only root-level exact bank/index names are dictionary files             | v1                  |

### Open questions

- Parity with Yomitan - resolve by: golden outputs captured once via
  yomitan-api with a real browser for a fixed sentence corpus.
- Store speed/size with Jitendex - resolve by: measurement in phase 5.

## 5. Tech stack

Status: approved 2026-11-10.

- **Name:** `rikaichad` (user decision, 2026-11-11; first named `jisho`,
  renamed to avoid confusion with jisho.org). Binary `rchad` (short to
  type), crates `rikaichad-*`, data dir `rikaichad`, environment
  variables `RIKAICHAD_*`. The CLI warns when it finds a store at the old
  `jisho` path and does not move it.
- **Language / runtime:** Rust (stable, edition 2024) - user constraint.
  The type system catches the likely bug classes (malformed input shapes,
  missing cases in result modes). Runner-up: none considered.
- **Build / test tooling:** cargo workspace, `cargo fmt`, `cargo clippy`,
  `cargo test`, one `scripts/check.sh`.

Facts that drive the choices (see `docs/dictionary-survey.md` and the
Yomitan source reading of 2026-11-10):

- Real collection: 10.2M term rows, one dictionary 3.5 GB uncompressed,
  bank files up to 125 MB; only deflate/stored compression.
- Yomitan's Japanese pipeline needs no regex engine for ja (suffix rules
  are plain string suffixes), one NFKD use limited to the CJK
  Compatibility Ideographs block, and an `Intl.Collator('en-US')` used
  only as a last tie-break in sorting.
- Yomitan stores 7 IndexedDB stores: terms (indexes expression, reading,
  sequence, reversed expression/reading), termMeta, kanji, kanjiMeta,
  tagMeta, dictionaries, media.
- Yomitan code indexes strings by UTF-16 code units; the port must keep
  that where it affects results (substring scanning, stem lengths).

### Dependencies (proposed)

| Dependency           | Purpose                                 | Why not stdlib / vendored                               | License            | Approved |
|----------------------|-----------------------------------------|---------------------------------------------------------|--------------------|----------|
| serde + serde_json   | Parse dictionary JSON, API JSON         | JSON parsing of untrusted input is not worth hand-rolling| MIT OR Apache-2.0  | yes      |
| zip (no default      | Read dictionary zips (central directory,| Zip container; 9.x is past all RustSec advisories (all  | MIT                | yes      |
| features,            | raw entry access)                       | were in extract(), which we do not use). Feature        |                    |          |
| `deflate-flate2-     |                                         | `deflate` would also pull zopfli (a compressor we never |                    |          |
| zlib-rs`)            |                                         | use)                                                    |                    |          |
| flate2 (zlib-rs)     | Inflate raw entries ourselves so a bad  | Already in the tree via zip (no added weight); needed   | MIT OR Apache-2.0  | yes      |
|                      | CRC-32 warns instead of failing         | because zip always fails on CRC mismatch (survey: 8 of  |                    |          |
|                      |                                         | 12 pitch dicts have bad CRCs; Yomitan ignores CRCs)     |                    |          |
| rusqlite (`bundled`) | Store                                   | Indexed lookups, transactions, WAL concurrent readers   | MIT (SQLite: PD)   | yes      |
| zstd (no default     | Compress stored glossaries with a       | 4x smaller at ~850 MB/s decompression with a trained    | MIT; libzstd       | yes (M3) |
| features,            | dictionary trained per imported         | dictionary; flate2 (already here) gives 3x at 128 MB/s, | BSD-3-Clause       |          |
| `zdict_builder`)     | dictionary                              | too slow for grouped lookups (docs/plan/milestone-3.md) |                    |          |
| axum + tokio         | HTTP API                                | Maintained HTTP parser for a server that may face a     | MIT                | yes      |
| (minimal features,   |                                         | network via --bind                                      |                    |          |
| current_thread)      |                                         |                                                         |                    |          |
| lexopt               | CLI argument parsing                    | Tiny, no deps; saves a hand-rolled parser               | MIT                | yes      |

Not taken (vendored or written instead): config file (JSON via serde_json,
not TOML), data-dir lookup (XDG/AppData/Library rules, ~30 lines),
logging (small module writing to stderr), CJK compatibility NFKD (a
generated table from UnicodeData.txt instead of unicode-normalization),
collation (code-point order; documented deviation in tie-breaks only).

### Storage

SQLite via rusqlite (user decision; "performance is a must"). The store is
private to rikaichad (not a public format): each dictionary row records its
source path and sha256 so `rchad reimport --all` can rebuild after a store
version bump; if a source zip is gone, the CLI says which ones.

Layout: **one SQLite file for all dictionaries** (user decision
2026-11-10: a single file is easy to copy to any platform; users keep
their zips as the source of truth). Only text is stored (no images or other
media, user decision 2026-11-10); styles.css is kept inside the same file,
so the store is self-contained and much smaller. The file is checkpointed
(WAL merged, `-wal`/`-shm` removed) on clean shutdown and after each
import, so a copied file is complete; `rchad export-store <path>` makes a
consistent copy while running (SQLite backup API). Reimport after a store
version bump matches source zips by sha256, searching paths the user gives
(`rchad reimport --all --from <dir>`), since recorded paths differ across
machines.

The milestone 1 benchmark now tunes this layout instead of choosing one.
Bank files
are parsed with a streaming serde visitor over the top-level array (never
a whole-file `Value`), so peak memory is bounded by one row plus batches.

Considered (kept for the record):

- **A. One database (WAL), all dictionaries (chosen):** one `terms` table with a
  dictionary id column, indexes on expression, reading, sequence. One
  indexed probe per candidate string regardless of dictionary count.
  Import = one transaction (atomic); WAL readers see a consistent snapshot.
- **B. One file per dictionary:** atomic import by temp file + rename
  with indexes built after the load, trivial delete; but every candidate
  string is probed in every file (92 separate connections; ATTACH is
  capped at 10 by default, 125 at compile time, so no single query).
- Measured for both: import time with index-after-load (B) vs into live
  indexes (A), WAL size during the 3.5 GB import, disk peak, delete time
  of a large dictionary and file size after it (A needs VACUUM), probe
  latency. Review H2 risks (multi-GB transaction into live shared
  indexes, WAL growth, delete leaving free pages) are now tuning items
  for A: batch sizes, `wal_autocheckpoint`, index-after-load per import
  via a staging table, `incremental_vacuum` after delete. Results are
  recorded here before milestone 2.
- Estimate (not measured): one scan yields hundreds to a few thousand
  candidate strings, so B costs up to ~92x the probes of A. Expect A.

Performance targets (approved by the user 2026-11-10, measured by a benchmark command against the
user's full 92-dictionary collection, warm cache, on the dev machine):

| Operation                                   | Target              |
|---------------------------------------------|---------------------|
| Term scan of a 16-char line, all dicts      | p50 < 5 ms, p99 < 20 ms |
| Kanji lookup                                | p99 < 2 ms          |
| Import of the 3.5 GB dictionary             | measured, reported; peak RSS < 1 GB |
| Store size                                  | measured, reported; decides glossary compression |

Targets are checked on two corpora: a public reproducible one (Jitendex +
KANJIDIC + one frequency and one pitch fixture, pinned URLs and sha256) for
done-criteria, and the user's private collection for the real-world report.

Benchmarks report measured numbers; no target is claimed met without one.

### Milestone 1 measurements (2026-11-10)

Measured on the dev machine (12 cores, 15 GB RAM, Linux), release build,
the user's collection: 104 zips, 99 imported (5 rejected as duplicate
titles, as Yomitan does), 9.06M terms, 14.8M rows in total.

| Measurement                                  | Result                         | Target            |
|----------------------------------------------|--------------------------------|-------------------|
| Import of all 104 zips                       | 592 s                          | reported          |
| Import peak memory                           | 90 MB (95 MB for the 3.5 GB dict) | < 1 GB         |
| Largest dictionary (3.5 GB, 1.2M terms)      | 167 s into a fresh store, 222 s into the full store | reported |
| Remove the largest dictionary                | 28 s; file keeps its size, freed pages reused by the next import | reported |
| Store size                                   | 11.6 GB (raw bank JSON 17.8 GB) | reported          |
| 16-char scan, prefixes only, match           | p50 0.64 ms, p99 3.8 ms        | p50 5, p99 20 ms  |
| Same, plus glossaries of the first 100 hits  | p50 1.9 ms, p99 5.7 ms         | (same)            |

`rchad bench` with `bench/sentences.txt` (967 scans, 11,367 candidates).
The first layout (glossary inside `terms`) gave p50 12 ms / p99 78 ms for
the same scans: every match loaded and parsed its glossary. Glossaries now
live in `term_glossaries` and are loaded only for returned entries.
Prefix-only scans are a lower bound; milestone 2 adds deinflection
candidates and re-measures against the targets.

Public corpus (`scripts/fetch-corpus.sh`: JMdict English and KANJIDIC
English, jmdict-yomitan release 2026-11-09, sha256-pinned): imports in
6.2 s (527,699 terms, 10,384 kanji), 80 MB peak, 223 MB store; prefix
scans p50 0.11 ms / p99 0.30 ms to match, p50 0.24 ms / p99 0.88 ms with
glossaries. Jitendex is not in the corpus: its GitHub releases stopped in
2024 without downloadable assets.

After the review fixes (row-size cap while parsing, LIMIT on matching,
deadline checks): JMdict import 7.2 s (was 6.1 s, the per-byte row
counter); full-store scans p50 0.71 ms / p99 4.4-5.0 ms to match and p50
2.05 ms / p99 5.8-6.7 ms with glossaries (3 runs).

Milestone 2, full Yomitan-style scans (prefixes, variants, deinflection,
ranking) with Yomitan's defaults (scanning length 16, maxResults 32): user
collection p50 3.4-3.5 ms / p99 17.4-17.6 ms for a complete lookup with
glossaries; public corpus p50 1.0 ms / p99 2.7 ms. Targets met. Details in
docs/plan/milestone-2.md.

Milestone 3 (grouping, metadata, kanji) measurements and the result-mode
costs are in docs/plan/milestone-3.md: split mode meets the targets on the
user's collection; group and merge full lookups return 10x more content
and do not.

Glossaries are stored zstd-compressed with their plain text from store
version 4 (docs/plan/milestone-3.md, Storage and response format). Open:
import speed into a large existing store.

### HTTP

axum + tokio (user decision). Default multi-threaded runtime with a small
worker count; SQLite calls run on the blocking pool so they never stall
the reactor. A single-threaded runtime was considered; no measured reason
to prefer it.

### Licensing details

- Crate SPDX: `rikaichad-format`, `rikaichad-store`, `rikaichad-api`, `rikaichad`:
  `AGPL-3.0-or-later`. `rikaichad-ja`, `rikaichad-lookup` (contain ported files):
  `AGPL-3.0-or-later AND GPL-3.0-or-later`.
- The generated CJK-compatibility table carries the Unicode License v3
  notice (UnicodeData.txt source, version recorded).
- Test data: committed goldens hold only headwords, readings, inflection
  chains and order, never glosses; CC BY-SA sources are attributed in
  `tests/fixtures/NOTICE`. Public test dictionaries are used in their
  published Yomitan-format builds (Jitendex release zip, a KANJIDIC Yomitan
  build), not raw XML, so no converter is involved.
- Pitch fixture: hand-written (our own) for committed tests; the user's 12
  pitch dictionaries (docs/dictionary-survey.md) for local runs.
- `/source` returns the repository URL plus the exact commit the binary
  was built from (embedded at build time).

### Module layout

Cargo workspace; layering is enforced by crate dependencies (a lower crate
cannot name a higher one).

```
crates/
  rikaichad-format/    Yomitan zip + JSON parsing and validation, caps for
                   hostile input. No storage. (own code, AGPL)
  rikaichad-ja/        Ported Yomitan Japanese: transforms, text preprocessors,
                   kana/CJK utilities, generated CJK-compat table.
                   (GPL-3.0-or-later headers, Yomitan copyright)
  rikaichad-store/     SQLite schema, import (streaming), queries. Depends on
                   rikaichad-format.
  rikaichad-lookup/    Ported Yomitan translator: variants, prefix scan,
                   deinflect, validate, rank, group/merge, kanji lookup,
                   sentence parse. Depends on rikaichad-ja, rikaichad-store.
                   (GPL-3.0-or-later headers for ported files)
  rikaichad-api/       HTTP server: one handler per file, read-only guard,
                   origin check. Depends on rikaichad-lookup.
  rikaichad/           Binary: one CLI command per file (import, list, remove,
                   order, lookup, kanji, parse, serve, bench).
docs/              brief, reference, survey, API spec (docs/api.md)
tests/fixtures/    tiny hand-written dictionaries (own, committed)
scripts/check.sh   all checks
```

### Test strategy

- External interface first: CLI tests run the binary against tiny
  hand-written fixture dictionaries (committed, our own license); HTTP
  tests start `serve` on an ephemeral port and call it, including
  `--read-only` (mutations -> 403) and cross-origin rejection.
- Ported deinflection: port Yomitan's Japanese transform test cases
  (GPL test data, ported with headers) as table-driven tests.
- Parity: goldens captured from real Yomitan via yomitan-api for a fixed
  sentence corpus and the public test corpus. Stored with them: Yomitan
  version (must equal the port commit), the exported Yomitan settings
  (dictionaries, order, result mode, scan options) and dictionary sha256s,
  plus `scripts/capture-goldens` to regenerate when the port commit moves.
  Pass criterion: exact match of the set of (headword, reading,
  inflection chain) per query, and exact order except inside groups
  that tie on every criterion before the final collation tie-break
  (Yomitan uses Intl.Collator en-US there; we use code-point order and
  document it - review H6). ICU4X collation is "later" if users notice.
- Local collection: an opt-in test/bench (`RIKAICHAD_DICT_DIR=dictionaries`)
  imports every zip in the user's folder; never runs in CI.
- Fuzzing: the zip/JSON importer and structured-content renderer
  (cargo-fuzz; needs a dev-tool approval when added). HTTP parsing is
  axum/hyper's and is not fuzzed by us.
- Hostile input fixtures: zip bomb, path traversal names, wrong arity,
  huge strings, deep structured-content nesting.

### Enforcement (scripts/check.sh)

- `cargo fmt --check`, `cargo clippy --all-targets -D warnings` with
  `unwrap_used`, `expect_used`, `panic`, `indexing_slicing` denied outside
  tests.
- `cargo test --workspace`, failing if zero tests ran.
- ASCII check on code and docs, excluding fixture data, ported Japanese
  tables and `docs/reference/`.
- License header check: every file in rikaichad-ja and every ported file in
  rikaichad-lookup carries the GPL-3.0-or-later Yomitan header; NOTICE lists
  them with the upstream commit.
- Dependency license check via `cargo metadata` (allow MIT, Apache-2.0,
  BSD, Unicode, Zlib, ISC, GPL/AGPL; anything else fails).
- `dictionaries/` is git-ignored; a check fails if any zip is tracked.

## 6. Milestones

Each milestone is a vertical slice through CLI and HTTP, runnable and
tested end to end. Done-criteria use the public test corpus (sec 5) so
anyone can reproduce them; the user's collection is benchmarked and
reported additionally.

1. **Import + exact lookup, CLI and HTTP + storage benchmark:**
   `rchad import <zip>` streams a format-3 dictionary into SQLite (atomic,
   capped, hostile input rejected with the JSON path of the error, media
   entries skipped, styles.css stored); `rchad lookup <word>` prints exact matches;
   `rchad serve` answers `POST /v1/terms/lookup` with the same results.
   `docs/api.md` written first: `/v1` prefix, error envelope, caps, result
   limits, lengths given both in code points and UTF-16 units, async
   import jobs (id, progress, cancel). `rchad bench` measures import,
   size and probe latency of the single-file store. - done when: fixture
   + hostile-input tests pass on 3 OSes; the public corpus imports; the
   user collection imports with 0 errors; a copied store file opens and
   answers lookups on another OS in CI; numbers recorded in sec 5.
2. **Japanese engine port:** rikaichad-ja (transforms, preprocessors incl.
   romaji and kana conversion) and scan/deinflect/validate in
   rikaichad-lookup, through CLI and HTTP. Looking up "taberarenakatta"
   (in kana/kanji) returns "taberu" with its inflection chain. - done
   when: ported Yomitan transform tests pass; scan targets in sec 5 met
   on the public corpus (a miss blocks the milestone).
3. **Ranking, grouping, metadata, kanji:** sort order, group/merge
   modes, frequency (rank/occurrence) and pitch/IPA metadata, tags,
   dictionary order and enable flags, kanji lookup; CLI and HTTP. - done
   when: the parity test passes the criterion in sec 5.
4. **Server hardening and management API:** import by upload, remove,
   reorder over HTTP; `--read-only`, Host/Origin checks, caps, timeouts,
   off-loopback warning, `/v1/source`, sentence parse endpoint and CLI.
   - done when: HTTP tests pass incl. read-only 403s, rebinding Host
   rejection, oversize bodies, concurrent import + lookup.
5. **Fuzzing and release prep:** importer and renderer fuzzers, renderer
   coverage across all 17 tags, README, install docs, NOTICE. - done
   when: each fuzzer runs 1 hour without a finding; check script green on
   Linux/macOS/Windows CI.
6. **Web UI** (added 2026-11-10, user request): word lookup, kanji and
   sentence parse pages served by `rchad serve` from files embedded in
   the binary; TypeScript and Tailwind with ponkotsu-css; glossaries
   rendered from structured content with dictionaries' styles.css
   scoped as in Yomitan. Plan and decisions: docs/plan/milestone-6.md. -
   done when: server tests for the UI routes, headers, same-origin rule
   and styles endpoint pass; ui/dist matches its sources in check.sh;
   every page looked at in light and dark on the public corpus.
7. **Conjugation tables** (added 2026-11-11, user request): a "Show forms"
   checkbox on each headword shows every conjugated form of a verb or
   i-adjective (`POST /v1/terms/forms`), like accentjiten's table. Plan
   and decisions: docs/plan/milestone-7.md. - done when: every generated
   form deinflects back with the ported transformer (listed exceptions
   only), API tests pass, the table is looked at in the UI.

## 7. Adversarial review (2026-11-10)

A reviewer subagent read only this brief and the original idea. Findings
and resolution:

| Id  | Finding                                                  | Resolution                                                |
|-----|----------------------------------------------------------|-----------------------------------------------------------|
| H1  | Scale contradicted measured data                         | Scale row restated from the survey                        |
| H2  | One multi-GB transaction into live shared indexes        | User chose single file; M1 benchmark tunes WAL, disk peak,|
|     | unverified; ATTACH limit for per-file                    | delete, index-after-load; streaming parser stated         |
| H3  | WAL + read-only open needs writable -shm                 | Concurrency row: lock file, API-level read-only guard     |
| H4  | --bind + no auth: import path, rebinding, caps           | Host allowlist, JSON-only POST, upload-only import, caps, |
|     |                                                          | timeouts. User kept writable-by-default (sec 3)           |
| H5  | Parity oracle underspecified                             | Goldens carry version/settings/sha256 + capture script,   |
|     |                                                          | explicit pass criterion                                   |
| H6  | Collation deviation breaks order parity                  | Final tie-break excluded from order comparison; ICU4X later|
| M1  | Contradictions (GPL, TOML, CI, diagram)                  | Fixed                                                     |
| M2  | Licensing details                                        | Licensing details subsection                              |
| M3  | No API spec                                              | docs/api.md is the first task of milestone 1              |
| M4  | Milestones horizontal, private-data done criteria        | Milestones rewritten as CLI+HTTP slices, public corpus    |
| M5  | Store format stability                                   | Store private; source sha256 + reimport --all             |
| M6  | Missing user expectations                                | Unknowns 11, 12, 19-23                                    |
| Low | Handler fuzzing overbuilt; name; reasons; /source commit | Dropped; open question; reasons added; commit embedded    |

Kept despite the review: sentence parsing in v1 (the user accepted it in
phase 4; it is scheduled after parity, in milestone 4).
