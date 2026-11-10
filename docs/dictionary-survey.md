# Local dictionary survey

Measured 2026-11-10 by a throwaway script over the user's `dictionaries/`
folder (92 zips, 2.7 GB compressed). These dictionaries are the user's
private copies, many of commercial origin: they are never committed, never
used as public test fixtures, and `dictionaries/` is git-ignored.

## Totals

| Item                    | Count                                                   |
|-------------------------|---------------------------------------------------------|
| Zips / parse errors     | 92 / 0 (all `format: 3`)                                |
| Term rows               | 10,228,465 (all 8 fields)                               |
| Term meta rows          | 4,608,294 (all `freq`; no `pitch`, no `ipa`)            |
| Kanji / kanji meta rows | 37,490 / 48,774                                         |
| Tag rows                | 990                                                     |
| Media files             | ~55,000 (gif, webp, jpg, png, svg), 1 pdf, 1 txt        |
| Zip compression methods | deflate (8) and stored (0) only                         |

## Largest dictionaries

| Dictionary (abridged)           | Uncompressed | Zip entries | Largest bank file |
|---------------------------------|--------------|-------------|-------------------|
| Nihon Kokugo Daijiten 2nd ed.   | 3,543 MB     | 23,857      | 125 MB            |
| Daijirin 4th ed.                | 2,691 MB     | 3,027       | 19 MB             |
| Daijirin 4th ed. (no images)    | 2,602 MB     | 282         | 19 MB             |
| Meikyo 3rd ed.                  | 1,145 MB     | 166         | 101 MB            |
| Sanseido Kokugo 8th ed.         | 1,081 MB     | 653         | 25 MB             |

Import must stream: one bank file can be 125 MB of JSON, one dictionary
3.5 GB uncompressed. Size caps must allow these.

## Key lengths

Longest index keys (bytes, UTF-8): term 9,893 (Babylon Japanese-English,
331 terms over 4 KiB, phrase entries), reading 746 (PixivLight), term meta
378 (BCCWJ), tag name 18, kanji 7. jisho caps keys at 64 KiB.

## Feature usage

- Glossary items: string 5.4M, structured-content 5.3M (41 of 92 dicts),
  image 30k (1 dict). No deinflection-reference glossaries.
- Structured-content tags used: span, div, a, li, img, ruby, rt, rp, ul,
  ol, br, table, tr, td, th, details, summary. Every tag in the schema.
- Links: 11.7M internal (`?query`) and 1.3M external `a` hrefs.
- `data` attributes: 170 distinct keys (class, name, id, ...); used by
  dictionaries' `styles.css` (4 dicts ship one).
- Style keys used: 21 of the schema's set.
- Frequency shapes: int, string, {value, displayValue}, each optionally
  per reading. 8 dicts declare `frequencyMode: rank-based`.
- Term `rules` field: Yomitan condition ids (v1, v5, vs, vk, vz, adj-i)
  plus many ids Yomitan has no condition for (n, vt, vi, adv, v2*, v4,
  adj-na, ...). Unknown ids must be ignored, not rejected.
- Quirks to accept: `definitionTags` null (400k rows); unknown index key
  `weight` (1 dict); media in nested folders (37 zips have directories).

## Pitch accent dictionaries (added 2026-11-10)

12 zips in `Pitch Accent/`, 1.2M pitch rows in total.

- 10 are `term_meta_bank` pitch dictionaries. Every pitch object is
  `{reading, pitches}`; every pitch has an integer `position` (no `HL`
  strings), `nasal` and `devoice` as arrays (mostly empty; 9 of 10 dicts
  include them), no `tags`. Multiple pitches per reading are common.
- 2 carry pitch as term-bank text instead: NHK new accent dictionary
  (plain string glossaries) and Counters & Suffixes (structured-content
  tables, `isUpdatable`).
- No IPA dictionary is present.

### Bad CRC-32 in 8 of 12 pitch zips

Every `term_meta_bank_*.json` in Kanjium 2, NHK 1998, NHK 2016, Sanseido
8th, Daijirin 4th, Daijisen, Shinmeikai 7th and 8th has a wrong CRC-32 in
the zip headers. The data itself is intact: each entry inflates to its
declared size and parses as valid JSON. All 8 were written by the same
tool (version-made-by 3/21, UTF-8 flag set).

- Yomitan imports them anyway: its importer calls zip.js
  `entry.getData(writer)` with no options
  (`ext/js/dictionary/dictionary-importer.js` `_getData`), and zip.js
  2.7.54 defaults `checkSignature` to false (`index.d.ts`), so CRCs are
  never verified.
- The Rust `zip` crate 9.5.0 always verifies CRC on normal reads and
  fails with "Invalid checksum" at end of entry
  (`src/read/readers.rs`, `src/crc32.rs`); only AES entries skip it.
- Requirement: the importer must accept a CRC mismatch when the entry
  inflates to its declared size, and log a warning naming the entry.

### Other quirks

- `__MACOSX/._index.json` (macOS resource fork) in the NHK new accent
  zip: only root-level exact names may be treated as dictionary files.

## Gaps for testing

No IPA dictionary is present; IPA tests use a small hand-written fixture.
Pitch tests use a hand-written fixture for committed tests and the user's
pitch dictionaries for local runs.

## Decision note

Media (~55k files, mostly images in a few monolingual dictionaries) is not
imported: the user wants text only (2026-11-10). Skipping those entries
also cuts import time and store size for the largest dictionaries.
