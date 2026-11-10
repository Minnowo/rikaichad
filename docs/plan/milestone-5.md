# Milestone 5 work list

Fuzzing and release prep (docs/brief.md section 6): importer and renderer
fuzzers, renderer coverage across all structured-content tags, README,
install docs, NOTICE.

## Decisions

- Fuzzing: first deferred, then run on the user's request ("ok lets run
  the fuzz test", 2026-11-10), with cargo-fuzz (libFuzzer) on a nightly
  toolchain. The fuzz crate (fuzz/) has its own workspace so the main
  build stays on stable; scripts/check.sh only checks its formatting.
- The brief says 17 tags; the structured-content schema has 20 (br, ruby,
  rt, rp, table, thead, tbody, tfoot, tr, td, th, span, div, ol, ul, li,
  details, summary, img, a). Coverage is checked against the schema.

## Steps

1. [x] Renderer coverage: a test with a case per schema tag that fails
       if the schema's tag list and the cases differ; ordered lists now
       numbered ("1. ") instead of bulleted.
2. [x] Nesting bound: structured content nested deeper than serde_json's
       recursion limit (128) is refused at import with the row and field
       named, so the recursive renderer and lookups never see unbounded
       depth (test).
3. [x] README rewritten for the current state; docs/install.md
       (requirements per platform, build and install, dictionaries, the
       store, upgrading, running the server and a systemd unit, binary
       distribution, uninstalling).
4. [x] NOTICE: third-party crates section; scripts/third-party-licenses.py
       writes the license texts of the 65 crates compiled into the binary
       (fails if one ships no license file).
5. [x] Fuzzers: five targets in fuzz/fuzz_targets, seeded from the test
       fixtures by fuzz/seed.py, each run for one hour (`-O`,
       `-max_len=65536`, in parallel), no crash, panic, timeout or OOM:

       | target  | input                                 | runs       | corpus |
       |---------|---------------------------------------|------------|--------|
       | archive | bytes as a dictionary zip             |  2,918,344 |  3,347 |
       | rows    | first byte picks a bank kind, rest as its JSON | 10,182,948 |  9,442 |
       | render  | bytes as structured content to text    | 23,710,707 |  7,107 |
       | import  | bytes as a zip, imported into a store  |  2,061,876 |  2,923 |
       | lookup  | op byte + text: lookup/kanji/parse/furigana on a fixture store | 100,809 | 1,623 |

       lookup is slow (27 runs/s: every lookup runs the full translator
       against SQLite), so in an hour libFuzzer grew its inputs only to
       165 bytes; a longer run would reach longer texts. Peak RSS stayed
       under 800 MB.
6. [ ] Check script green on Linux/macOS/Windows CI: needs the GitHub
       repository; green locally on Linux.
