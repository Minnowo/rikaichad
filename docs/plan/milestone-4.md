# Milestone 4 work list

Server hardening and management API (docs/brief.md section 6): import by
upload, remove, reorder and settings over HTTP; `/v1/source`; Yomitan's
scanning sentence parser as `POST /v1/text/parse` and `jisho parse`.
Already in place from milestone 1: `--read-only`, Host/Origin checks,
body caps, request timeout, connection limits, off-loopback warning.

Done when: HTTP tests pass including read-only 403s, rebinding Host
rejection, oversize bodies, concurrent import + lookup.

## What is ported, from where

| Rust                        | Yomitan source (ext/js/...)                                          |
|-----------------------------|-----------------------------------------------------------------------|
| jisho-ja furigana           | language/ja/japanese.js: distributeFurigana, distributeFuriganaInflected, segmentizeFurigana, getFuriganaKanaSegments, getStemLength |
| jisho-lookup parse          | background/backend.js: _textParseScanning (the "scanning-parser"; MeCab is out of scope) |

Behaviour kept exactly: UTF-16 positions and lengths throughout (the
parser steps through the text by UTF-16 units and cuts `scanLength` units);
simple-mode lookups; a match counts only if it is longer than one character
or the character is Japanese; consecutive unmatched characters form one
token; headwords kept only with a primary exact source whose original text
is the token.

## API (docs/api.md has the details)

| Method | Path                         | Mutates |
|--------|------------------------------|---------|
| POST   | `/v1/dictionaries`           | yes: zip upload, `202` with a job |
| GET    | `/v1/jobs/{id}`              | no      |
| DELETE | `/v1/jobs/{id}`              | yes: cancel a running import, forget a finished one |
| DELETE | `/v1/dictionaries/{id}`      | yes     |
| PATCH  | `/v1/dictionaries/{id}`      | yes: enabled, alias, per-dictionary settings |
| PUT    | `/v1/dictionaries/order`     | yes     |
| GET    | `/v1/settings`               | no      |
| PATCH  | `/v1/settings`               | yes: main and sort-frequency dictionary |
| GET    | `/v1/source`                 | no      |
| POST   | `/v1/text/parse`             | no      |

- One import at a time (`409 busy`). Uploads stream to a file next to the
  store (no temp-dir size surprises), capped by `--max-upload` (default
  4 GiB); the upload is exempt from the 10 s request limit but fails if no
  data arrives for the idle timeout (30 s). The file is deleted after the
  import.
- Imports are cancellable: the store checks a cancel flag every 10,000
  rows; cancelling rolls the import back.
- `/v1/source`: repository URL and the commit the binary was built from
  (embedded at build time; "unknown" outside a git checkout, with a
  `dirty` flag), for AGPL-3.0 section 13.

## Steps

1.  [x] docs/api.md: the endpoints above.
2.  [x] Store: cancellable import.
3.  [x] jisho-ja furigana port + differential test against Yomitan's
        functions (node oracle).
4.  [x] jisho-lookup parse port; `jisho parse`.
5.  [x] API: jobs + upload, remove, patch, order, settings, source, parse;
        read-only 403s.
6.  [x] HTTP tests: read-only 403 on every mutating endpoint, rebinding
        Host, oversize bodies (JSON and upload), concurrent import +
        lookup, cancel.
7.  [x] Independent review; fix findings.

## Results

- Furigana: 3,000 generated (term, reading, source) triples give exactly
  Yomitan's output for both functions (tests/fixtures/yomitan/furigana-golden.json).
- Parser: the oracle (scripts/port/yomitan-oracle.mjs, mode `parse`) runs
  `_textParseScanning`, copied from backend.js because the Backend class
  needs a browser, around Yomitan's own findTerms and furigana functions.
  Public corpus: the 40 bench sentences and the 923 scan texts parse
  identically; the crafted dictionary's queries are in the committed
  parity test.
- HTTP: tests/management.rs covers read-only 403 on every mutating
  endpoint, upload jobs (success, conflict, invalid zip, forget), upload
  caps (415, 413, stalled upload holding the write slot until the idle
  timeout, busy 409 meanwhile, partial upload removed), lookups answering
  during an import of 60,000 rows, cancelling an import, remove, patch,
  order, settings, parse and source. Host rebinding, oversize JSON bodies
  and timeouts were covered since milestone 1 (tests/http.rs).

## Independent review (2026-11-10)

The reviewer ran a release server against a public-corpus store, crafted
uploads and 18 edge texts through the parser oracle (all identical), and
checked the upload path (names, cleanup on every exit path, write slot,
read-only on every route, timeout exemption).

| Id | Finding                                                         | Resolution                                                        |
|----|-----------------------------------------------------------------|-------------------------------------------------------------------|
| 1  | Ctrl-C during an import waited for it and committed it          | Shutdown cancels running jobs, runtime stops with a 10 s limit; measured: exit in 0.7 s mid-import, nothing committed, upload removed |
| 2  | CORS preflight did not allow PATCH                              | Allowed; test                                                      |
| 3  | Stalled upload got 400 (connection idle limit fired first)      | Upload's own limit just under the connection's: 408; test asserts it |
| 4  | Non-numeric path ids answered in plain text                     | Parsed in the handlers: 400 with the envelope; test               |
| 5  | /v1/source could report a stale or clean commit for modified code | build.rs reruns on any workspace source change, counts untracked files, requires the checkout to be this workspace, handles worktrees |
| 6  | PATCH/PUT/settings writes waited 2 s for SQLite during an import; PATCH in two transactions | Take the write slot (409 at once); one transaction; documented |
| 7  | One client could hold the write slot for days by trickling bytes | Minimum average rate 64 KiB/s after the first minute             |
| 8  | Cancel could answer 202 and the job still succeed               | Checked while hashing the upload and just before commit; docs say "unless already committing" |
| 9  | A panicking import task would leave its job running forever     | Guard marks it failed                                              |
| 10 | Blocking file writes (and an fsync) on the 2 async workers       | Writes in block_in_place; no fsync for the temporary file          |
| 11 | Tests not checking what they claimed                           | Chunked 413, 408 status, cancel after the first bank, busy timing, PATCH preflight |
| 12 | Nits: error table order                                         | Fixed. Non-ASCII in the parse usage string is an allowed literal; WAL checkpoint after a busy import noted as known |
