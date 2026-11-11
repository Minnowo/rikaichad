# Milestone 1 work list

Import + exact lookup through CLI and HTTP + storage benchmark. See
`docs/brief.md` section 6. Items are done in order; each ends green on
`scripts/check.sh` and is committed.

## Yomitan importer behaviour to match

From `reference-src/yomitan/ext/js/dictionary/dictionary-importer.js`
at the pinned commit:

- Files are matched by exact root-level name: `index.json`, `styles.css`,
  `term_bank_N.json`, `term_meta_bank_N.json`, `kanji_bank_N.json`,
  `kanji_meta_bank_N.json`, `tag_bank_N.json` (regex `^...(\d+)\.json$`).
  Bank files are processed in zip entry order, terms first, then term
  meta, kanji, kanji meta, tags. Row ids follow that order (merge mode
  sorts by id later).
- index.json missing at root: error; if found nested, say so.
- `format` or `version` gives the version; title and revision required.
- A dictionary whose title is already imported is rejected.
- Term v3: `reading` empty -> reading = expression.
- Glossary `{type: "text", text}` is stored as the plain string `text`.
- Old index `tagMeta` entries are added as tags.
- `styles.css` present but empty is an import error in Yomitan
  ("No styles found"). We treat it as no styles (documented deviation).
- `minimumYomitanVersion`: Yomitan rejects when newer than itself. We
  store it and warn (we have no Yomitan version). Rikaitan's fork of the
  format names it `minimumRikaitanVersion`; we read and warn on both.

## Steps

1.  [x] `docs/api.md`: `/v1` endpoints, error envelope, caps, lengths.
2.  [x] jisho-format: zip entry access via raw bytes + flate2 inflate,
        CRC check -> warning, caps (entry count, declared sizes, actual
        inflated size <= declared), root-level name matching.
3.  [x] jisho-format: index.json parsing/validation.
4.  [x] jisho-format: streaming bank parsing (term, term meta, kanji,
        kanji meta, tag) with arity/type checks and error locations
        (file, row, field).
5.  [x] Fixtures: small hand-written dictionaries built into zips by the
        tests; hostile fixtures (bomb, traversal names, bad arity, deep
        nesting, bad CRC, encrypted, unsupported method).
6.  [x] jisho-store: schema v1, single file, WAL, streaming
        import in one transaction, duplicate title rejection, list,
        remove, exact term lookup (expression or reading).
7.  [x] Plain-text glossary rendering (strings, structured content,
        image alt) for CLI output.
8.  [x] CLI: `import`, `list`, `remove`, `lookup`, with `--json`.
9.  [x] jisho-api + `serve`: `POST /v1/terms/lookup`, `--bind`,
        `--read-only`, Host/Origin checks, caps.
10. [x] `bench`: import time, peak RSS, store size, probe latency.
11. [x] Run on the public corpus and the user's collection; record
        numbers in brief section 5; tune.
12. [~] CI workflow for Linux/macOS/Windows.
13. [x] Independent review of the milestone diff; fix findings.

## Deviations recorded while implementing

- No lock file: SQLite's own write lock (IMMEDIATE transactions, 2 s busy
  timeout) serializes imports and removals across processes; a second
  writer gets `StoreError::Busy` ("try again when it finishes").
- `index.json` `tagMeta` tags are added once when the dictionary has at
  least one tag bank (Yomitan adds them once per tag bank file; the
  duplicates have no effect on results).
- `glossary_text` uses jisho's own plain-text rules (docs/api.md), since
  Yomitan's text comes from browser `innerText`.
- Glossaries are stored in `term_glossaries`, not in `terms`: measured
  12x faster scans (brief section 5, milestone 1 measurements).
- Key cap raised from 4 KiB to 64 KiB after a real dictionary (Babylon)
  had 9.9 KB terms.

## Status notes

- 11: user collection and public corpus (scripts/fetch-corpus.sh) done
  and recorded in brief section 5.
- 12: workflow written; it has not run yet because the repository has no
  remote. The cross-OS portability tests pass locally on Linux only.

## Independent review (2026-11-10)

A reviewer subagent read the milestone diff with the plan and API spec.
Findings and what was done (tests added for each fix unless noted):

| Id  | Finding                                                    | Resolution                                                                  |
|-----|------------------------------------------------------------|-----------------------------------------------------------------------------|
| H1  | Exact lookup loaded all matches; HTTP timeout did not stop work; no concurrency cap | LIMIT on matching; SQLite progress-handler deadline (StoreError::Interrupted -> 503); semaphore of 8 store tasks |
| H2  | Two processes creating a fresh store raced                 | Lock-free check, then IMMEDIATE transaction re-check; WAL switch retried on busy |
| M1  | One huge row could take GBs at import and lookup           | Limits.max_row_bytes (16 MiB) enforced while parsing; glossaries loaded within the response byte budget using stored lengths |
| M2  | Entry cap checked after parsing the whole central directory | EOCD / zip64 EOCD entry count read first                                   |
| M3  | Dictionary ids reused after removal                        | AUTOINCREMENT                                                               |
| M4  | No header/body time limit, unlimited connections           | Whole-request timeout middleware; listener with 64-connection cap and 30 s idle timeout |
| L1  | Checkpoint result ignored; failure after commit reported as error | checkpoint() returns completeness; warnings after commit                |
| L2  | Every body read error was 413                              | Content-Length precheck; only length-limit errors are 413                    |
| L3  | lookup exit 1 for errors and for not found                 | Errors exit 2                                                               |
| L4  | Pitch integers rejected 2.0                                | Shared JSON-Schema integer check                                            |
| L5  | Removal scanned meta/kanji tables                          | (dictionary) indexes                                                         |
| L6  | bench --max-length overflow                                | saturating_add                                                              |
| L7  | Non-UTF-8 Origin treated as absent                         | Rejected with 403                                                            |
| L8  | Weak tests (import concurrency, races)                     | Tests for read-during-import, concurrent creation, deadlines, budgets       |
| -   | Dictionary text could inject terminal escapes              | CLI replaces control characters in text output                              |

Not changed: scripts/check.sh still checks that more than zero tests ran
overall, not per crate; the portable-store test expects an empty WAL,
which holds in CI where nothing else reads the store.
