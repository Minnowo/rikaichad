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
  store it and warn (we have no Yomitan version).

## Steps

1.  [ ] `docs/api.md`: `/v1` endpoints, error envelope, caps, lengths.
2.  [ ] jisho-format: zip entry access via raw bytes + flate2 inflate,
        CRC check -> warning, caps (entry count, declared sizes, actual
        inflated size <= declared), root-level name matching.
3.  [ ] jisho-format: index.json parsing/validation.
4.  [ ] jisho-format: streaming bank parsing (term, term meta, kanji,
        kanji meta, tag) with arity/type checks and error locations
        (file, row, field).
5.  [ ] Fixtures: small hand-written dictionaries built into zips by the
        tests; hostile fixtures (bomb, traversal names, bad arity, deep
        nesting, bad CRC, encrypted, unsupported method).
6.  [ ] jisho-store: schema v1, single file, WAL, lock file, streaming
        import in one transaction, duplicate title rejection, list,
        remove, exact term lookup (expression or reading).
7.  [ ] Plain-text glossary rendering (strings, structured content,
        image alt) for CLI output.
8.  [ ] CLI: `import`, `list`, `remove`, `lookup`, with `--json`.
9.  [ ] jisho-api + `serve`: `POST /v1/terms/lookup`, `--bind`,
        `--read-only`, Host/Origin checks, caps.
10. [ ] `bench`: import time, peak RSS, store size, probe latency.
11. [ ] Run on the public corpus and the user's collection; record
        numbers in brief section 5; tune.
12. [ ] CI workflow for Linux/macOS/Windows.
13. [ ] Independent review of the milestone diff; fix findings.
