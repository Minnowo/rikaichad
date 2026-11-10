# Milestone 3 work list

Ranking, grouping, metadata, kanji: the rest of Yomitan's translator
(`findTerms` modes `group`, `merge`, `term`, `simple`; tags; term and kanji
metadata; sort-frequency dictionary; `findKanji`), dictionary order,
enable flags and per-dictionary settings, through CLI and HTTP. See
`docs/brief.md` section 6. Upstream: `reference-src/yomitan` at
ee81f8dcf13e985bb4d7961009fb49a764b4d924.

Done when: Yomitan's translator tests pass for every Japanese case without
textReplacements (all modes, kanji, full entries including tags,
frequencies and pronunciations); the differential run of Yomitan's own
translator against jisho on the public corpus (group and merge modes)
matches by the brief's criterion; scan targets still met in the default
mode on the user's collection.

## What is ported, from where

| Rust                        | Yomitan source (ext/js/...)                                         |
|-----------------------------|----------------------------------------------------------------------|
| jisho-lookup tags           | language/translator.js: TranslatorTagAggregator, _expandTagGroups, _groupTags, _mergeSimilarTags, _flagRedundantDefinitionTags, _createTag |
| jisho-lookup grouping       | _groupDictionaryEntriesByHeadword/ByTerm, _groupDictionaryEntries, _createGroupedDictionaryEntry, _sortHeadwords, _getHeadwordMatchCounts, _addTermHeadwords, _addUniqueSources, _addTermDefinitions(Fast), _addUniqueTermHeadwordIndex |
| jisho-lookup merge          | _getRelatedDictionaryEntries, _addRelatedDictionaryEntries, _addSecondaryRelatedDictionaryEntries, _removeExcludedDefinitions, _removeUnusedHeadwords |
| jisho-lookup metadata       | _addTermMeta, _getFrequencyInfo, _convertStringToNumber, _updateSortFrequencies, _sortTermDictionaryEntryDefinitions, _sortTermDictionaryEntrySimpleData |
| jisho-lookup kanji          | findKanji, _expandKanjiStats, _sortKanjiStats, _addKanjiMeta, _sortKanjiDictionaryEntries, _sortKanjiDictionaryEntryData |
| jisho-store queries         | dictionary/dictionary-database.js: findTermMetaBulk, findTagMetaBulk, findTermsBySequenceBulk, findTermsExactBulk, findKanjiBulk, findKanjiMetaBulk (row order of _findMultiBulk / _findFirstBulk) |
| lookup options              | background/backend.js _getTranslatorFindTermsOptions (merge mode adds a disabled main dictionary and excludes its definitions) |

## Behaviour that must be kept exactly

- Database order: results in request order, then by id; tag lookup takes
  the first row by id with that name (part before `:`) and dictionary;
  sequence lookups are not limited to enabled dictionaries.
- Entries added by merge mode (sequence and secondary lookups) keep their
  deinflection-reference items in `entries`, unlike scanned entries.
- Merge mode dedupes definitions by dictionary and glossary content.
- Grouped entries take the inflection and text-processor chains of the
  primary entry with the fewest chains (first wins ties).
- Tag expansion happens once at the end on the merged tag-name groups;
  tags with the same name and category merge; sort by order, then name.
- Yomitan's default result mode is `group`; jisho's default follows.

## Store changes (version 4; older stores are refused and re-imported)

- `dictionaries`: `alias`, `allow_secondary_searches`,
  `parts_of_speech_filter`, `use_deinflections`.
- `meta`: `main_dictionary` (set at import to the first sequenced
  dictionary, as Yomitan's settings page does), `sort_frequency_dictionary`,
  `sort_frequency_order`.
- Kanji stats keep their key order (JavaScript `Object.entries` order).
- Glossaries stored compressed with their plain text (see Storage and
  response format below). Version 3, briefly migrated in place from 2
  during this milestone, is refused like 2: the user chose re-importing
  over a migration that rewrites every glossary.

## Steps

1.  [x] Store: schema (v3, then v4), settings, queries above.
2.  [x] jisho-lookup: tags, grouping, merge, metadata, sort frequencies,
        kanji.
3.  [x] Translator parity test: every in-scope case, full entries.
4.  [x] API: `result_mode`, tags as objects, frequencies, pronunciations;
        `POST /v1/kanji/lookup`; docs/api.md.
5.  [x] CLI: `lookup --result-mode`, `kanji`, `dictionary`, `order`,
        `settings`.
6.  [x] Differential oracle: Yomitan's translator in node over the public
        corpus; committed golden (no glosses) and test.
7.  [x] Bench in the default mode; tune if targets missed (see
        Measurements: met in split mode, not in group/merge full lookups).
8.  [x] Independent review; fix findings.

## Results

### Yomitan's translator tests

All in-scope cases match whole entries (tags, frequencies, pronunciations
included): 32 split, 1 group, 2 merge, 1 term, 1 simple, 3 kanji. Image
size keys excluded as in milestone 2. Mutations of the redundant-tag flag,
tag sort and kanji stat sort make cases fail; mutations of merge-mode
definition dedupe, headword order in grouped entries and keeping
deinflection references in merge-related entries do not (the fixture does
not exercise them), hence the differential runs below.

### Differential runs against Yomitan's own translator

scripts/port/yomitan-oracle.mjs runs Yomitan's translator.js (pinned
commit, real `Intl.Collator`) over a jisho store's rows with an IndexedDB
fake that returns rows in IndexedDB's order; the store's settings are
passed as Yomitan's backend would. crates/jisho-lookup/tests/corpus_parity.rs
prints jisho's results in the same compact form (every field of every
entry except glossary content, which is compared by item count and
deinflection-reference count). Queries: every start position of
bench/sentences.txt, 16 characters (923).

| Store                                         | Settings                                   | Result                          |
|-----------------------------------------------|--------------------------------------------|---------------------------------|
| Public corpus (JMdict, KANJIDIC)              | defaults                                    | split, group, merge, term, kanji: 923/923 each |
| User subset: JMdict, Shinwaei (shin-wa-ei), NHK, Kanjium, JPDB, BCCWJ, KANJIDIC, JPDB Kanji, Wikipedia Kanji (local only) | A: defaults           | split, group, merge, term, simple, kanji: 923/923 each |
| same                                           | B: order changed, Shinwaei (shin-wa-ei) secondary searches, aliases, Kanjium disabled, JPDB sort frequency ascending | split, group, merge, term, simple, kanji: 923/923 each |
| Crafted dictionary (tests/fixtures/parity)     | defaults                                    | 45/45, committed as a test       |

What the oracle cannot see: it reads jisho's own store, so importer
behaviour (sorted object keys, number formats, text items flattened,
empty readings replaced, kanji stat order) is shared by both sides; the
importer is checked against Yomitan's import separately (milestone 1).

The crafted dictionary covers what neither corpus has: rows with
deinflection references in a merge group, duplicate glossaries, two
readings of one term, every frequency form, pitch tags. The mutations the
Yomitan fixture missed all fail one of these runs.

Fixed along the way:
- Merge mode numbered frequencies in sorted order; Yomitan numbers them
  walking all entries before sorting (an `index` difference when an entry
  has two readings of one term). Now recorded before cutting at the limit.
- Collation: the final tie-breaks (headwords, tag names, kanji stats) used
  code points; with Yomitan's real collator 3% of group-mode scans had a
  different order within the first 32 entries (kana compare by letter
  before script and dakuten; kanji are not in code point order). jisho now
  compares by per-character weights measured from Intl.Collator
  (crates/jisho-lookup/data/collation.json: the Basic Multilingual Plane,
  55,485 characters; characters the collator expands, e.g. U+3006, and
  combining marks are left out and sort after all others by code point).
  0 of 288,846 random pairs of covered strings differ (prototype on the
  Japanese ranges); the committed collator golden (6,000 strings) matches.

The committed golden (tests/fixtures/parity/public-corpus-golden.json)
keeps headwords, readings, inflection chains and order of the first 32
entries for 462 queries in group and merge mode; CI checks it against the
pinned public corpus.

### Measurements (2026-11-10)

User collection (99 dictionaries, 9.06M terms; migrated in place to store
v3), 967 scans of 16 characters, limit 32, after a warm-up pass:

| Mode            | Find, group, rank, tags, metadata p50 / p99 | Full lookup with glossaries p50 / p99 |
|-----------------|-----------------------------------------------|----------------------------------------|
| split           | 2.8 / 16.6 ms                                 | 4.3 / 19.6 ms                          |
| group (default) | 3.9 / 19.6 ms                                 | 17.2 / 70.8 ms                         |
| merge (main JMdict) | 6.7 / 65.0 ms                             | 21.6 / 101.9 ms                        |

Split mode meets the targets (p50 5 ms, p99 20 ms). Grouped results are a
different amount of work: with 99 dictionaries the 32 grouped entries
hold 190-510 definitions and 450-900 KB of glossary JSON (a 2.5-5 MB
response); the full lookup is then mostly parsing those glossaries
(6.6 ms mean), building the JSON (1.8 ms) and measuring it for the
response cap (2.3 ms). Finding and ranking stays within the targets in
group mode.

Fixed while measuring:
- SQLite picked the per-dictionary index for the metadata queries once a
  few dozen terms were asked (240 ms instead of 0.2 ms); the queries now
  rule it out (`+dictionary`).
- The glossary size check and the merge-mode duplicate check read whole
  glossaries (`length()` on text); `octet_length` reads the size from the
  record header. Group full lookup p50 47 -> 17 ms, merge find p99 366 ->
  70 ms.
- Glossaries were cloned for every definition; now moved unless a row
  backs several entries.
- The final tie-break comparison was not a total order when one string had
  a character outside the collation table (the sort panicked on the full
  collection); such characters now get weights after the table's.

### Known differences from Yomitan

- Characters outside the collation table (CJK extension B and other
  supplementary planes, characters the collator expands, combining marks)
  sort after all others, by code point, in the final tie-break.
- Merge-mode definition dedupe compares glossaries as jisho stores them,
  with object keys sorted, so two glossaries that differ only in key order
  count as duplicates in jisho and not in Yomitan.
- Two distinct tag names the collator calls equal (e.g. differing only by
  a zero-width character) on pronunciations: with a limit, jisho orders
  them by their first appearance among the kept entries, Yomitan among all
  entries.
- Image glossary sizes, textReplacements and non-Japanese languages as in
  milestone 2.

## Independent review (2026-11-10)

The reviewer read translator.js, dictionary-database.js and backend.js
against the port and ran its own differential runs: public corpus at
limit 32, and crafted multi-dictionary stores (shared tag names,
frequency/pitch/IPA with tags, two readings per term) under four settings
variants at limits 1, 2, 3 and 100 in every mode; all identical except the
findings below. It confirmed the fake IndexedDB's result order for every
query the translator makes, the tag registration order, merge mode, the
metadata-after-limit path and the sort-frequency path.

| Id | Finding                                                          | Resolution                                                        |
|----|------------------------------------------------------------------|-------------------------------------------------------------------|
| 1  | Merge dedupe loaded whole groups' glossaries outside any cap (300 x 1 MB rows: 609 MB) | Contents hashed one row at a time, only for definitions that can have a twin (same stored size, or references dropped); equal hashes confirmed |
| 2  | Kanji stat key dedupe quadratic (1.4M keys: about 45 min)         | Hash map of key positions                                          |
| 3  | `1` and `1.0` in glossaries count as different in merge dedupe    | Integral numbers written as integers at import and when hashing   |
| 4  | Collator-equal pronunciation tag names ordered by kept entries only | Known difference (above)                                         |
| 5  | Dedupe missed twins across SQL batches of 2000                    | Gone with finding 1's hashing                                      |
| 6  | Docs said simple mode has no metadata (it has the sort dictionary's) | Docs, help and code comments fixed                              |
| 7  | No committed differential at a limit; oracle shares the importer  | Crafted test also runs at limits 1 and 2 against Yomitan's output cut there; limitation noted above |
| 8  | Main dictionary claims: removal clears it (Yomitan does not); migration picked by priority | Wording fixed; migration picks by import order |
| 9  | v2 stats claim too narrow                                         | Moot: version 4 requires re-importing                              |
| 10 | `serve --read-only` migrates a v2 store                           | Moot: no migration in version 4                                    |
| 11 | Oracle's `jisho` collator option stale                            | Removed                                                            |

## Storage and response format (after the review)

Grouped results carry the definitions of every dictionary, so a full
lookup was mostly glossary handling (group mode, 99 dictionaries, mean per
lookup: parse 5 ms, build JSON 1.8 ms, serialize to measure for the cap
2.3 ms, plus a second serialization over HTTP). Changes, chosen by the
user:

1. Serialize once: each entry is serialized one time; that text is
   measured for the response cap and copied into the response
   (`serde_json` `raw_value`).
2. Glossaries copied, not parsed: the stored glossary JSON goes into the
   response as it is; only rows whose deinflection references must be
   dropped are parsed.
3. Plain text rendered at import and stored with the glossary.
4. zstd compression (user approved the dependency): each glossary row's
   glossary and text JSON in one payload, compressed at level 9 with a
   ~110 KB dictionary trained per imported dictionary on its first 4 MB of
   glossaries (crates/jisho-store/src/glossary_codec.rs). Prototype on
   30,001 of the user's glossaries: zlib (already a dependency) 3.05x at
   128 MB/s decompression, zstd without dictionary 2.77x at 493 MB/s, zstd
   with a trained dictionary 4.0-4.5x at 750-850 MB/s.

The oracle export (crates/jisho-store/tests/export_oracle.rs) now reads
glossaries through the store, replacing scripts/port/export-store.py.
Public corpus after the change: 5,538/5,538 queries identical to Yomitan;
store 225 MB -> 116 MB with the text added; import 7 s -> 13 s.

User collection after the change (99 dictionaries re-imported into store
v4; 5 zips skipped as duplicate titles, as before): store 11.7 GB ->
3.6 GB with the text added; import 723 s -> 879 s, peak 121 MB. Scans as
above (967, 16 characters, limit 32), main dictionary JMdict:

| Mode            | Find, group, rank p50 / p99 | Full lookup p50 / p99 (before -> now)   |
|-----------------|-----------------------------|-----------------------------------------|
| split           | 2.8 / 16.3 ms               | 4.3 / 19.6 -> 3.8 / 17.7 ms             |
| group (default) | 4.1 / 22.0 ms               | 17.2 / 70.8 -> 9.0 / 33.2 ms            |
| merge           | 7.3 / 71.3 ms               | 21.6 / 101.9 -> 12.1 / 76.8 ms          |
