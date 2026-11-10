# Milestone 5 work list

Fuzzing and release prep (docs/brief.md section 6): importer and renderer
fuzzers, renderer coverage across all structured-content tags, README,
install docs, NOTICE.

## Decisions

- Fuzzing: skipped for now (user decision 2026-11-10, offered cargo-fuzz
  with a nightly toolchain, a stable-only random harness, or skipping).
  The brief's done-criterion "each fuzzer runs 1 hour" is therefore open.
  What stands in for it today: hostile-input tests from milestone 1
  (caps, zip bombs, CRC, encrypted entries, oversized rows), the depth
  limit test below, and every glossary of the user's 99 dictionaries
  (9.06 million rows) rendered at import without a failure.
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
5. [ ] Fuzzers (deferred, see Decisions).
6. [ ] Check script green on Linux/macOS/Windows CI: needs the GitHub
       repository; green locally on Linux.
