# Milestone 2 work list

Japanese engine port: Yomitan's Japanese transforms and text preprocessors
(jisho-ja), and the scan / deinflect / validate / sort core of its
translator (jisho-lookup), through CLI and HTTP. See `docs/brief.md`
section 6. Upstream: `reference-src/yomitan` at
ee81f8dcf13e985bb4d7961009fb49a764b4d924.

Done when: Yomitan's Japanese transform tests pass; Yomitan's translator
tests pass for the milestone-2 subset (below); scan targets in brief
section 5 are met on the public corpus and measured on the user's
collection.

## What is ported, from where

| Rust                              | Yomitan source                                                  | How                     |
|-----------------------------------|-----------------------------------------------------------------|-------------------------|
| jisho-ja transformer              | ext/js/language/language-transformer.js, language-transforms.js | hand port               |
| jisho-ja data/japanese-transforms.json | ext/js/language/ja/japanese-transforms.js                  | dumped by node script   |
| jisho-ja kana / width / combining / emphatic functions | ext/js/language/ja/japanese.js             | hand port               |
| jisho-ja CJK ranges, NFKD tables  | ext/js/language/CJK-util.js, String.normalize('NFKD')           | hand port + node dump   |
| jisho-ja romaji                   | ext/js/language/ja/japanese-wanakana.js, japanese-kana-romaji-dicts.js | hand port + node dump |
| jisho-ja kanji variants           | npm kanji-processor 1.0.2 (MIT), used by standardizeKanji       | node dump               |
| jisho-ja text preprocessors       | ext/js/language/ja/japanese-text-preprocessors.js, CJK-util.js normalizeRadicalCharacters, language-descriptors.js (order) | hand port |
| jisho-lookup                      | ext/js/language/translator.js: _findTermsInternal, _getDeinflections, _getDictionaryDeinflections, _getAlgorithmDeinflections, _getTextVariants, _getNextSubstring, _getDictionaryEntries, _matchEntriesToDeinflections, _sortTermDictionaryEntries, _getJapaneseChineseKoreanOnlyText | hand port |
| jisho-store bulk term query       | ext/js/dictionary/dictionary-database.js findTermsBulk           | own SQL + ported row assignment |

## Behaviour that must be kept exactly

- Transform results in breadth-first order; trace newest-first; cycle
  check per (transform, rule, text); conditions as bit flags (<= 32).
- Text variants in insertion order; chains accumulate when two variants
  produce the same text; a processor that returns its input unchanged adds
  nothing to the chain.
- kanji-processor converts only BMP variants (its regex has no `u` flag).
- findTermsBulk: each row returned once, attached to the first matching
  (candidate index, expression-before-reading) request; results in that
  request order, ids ascending within a request.
- _getDictionaryEntries: duplicate ids resolved by transformed text length
  with Yomitan's odd `originalText !== existingTransformedText` check.
- Lengths (originalText, transformedText) in UTF-16 code units.
- Sort: Yomitan's 10 criteria; final tie-break by code point instead of
  Intl.Collator('en-US') (documented deviation, brief section 5).
- Default options as Yomitan's tests: matchType exact, deinflect true,
  removeNonJapaneseCharacters true, searchResolution letter,
  partsOfSpeechFilter true, useDeinflections true, no text replacements.

## Translator test subset for this milestone

From test/data/translator-test-inputs.json and -results.json: the 38
Japanese `split`-mode tests, comparing per entry: order, headword term
and reading, sources, inflection and text-processor rule chains, isPrimary,
score, dictionaryIndex, sourceTermExactMatchCount, matchPrimaryReading,
maxOriginalTextLength, definition ids, sequences and entries; plus
originalTextLength. Not compared yet (milestone 3): tags, frequencies,
pronunciations, grouping modes. Skipped: textReplacements (5 tests; a
Yomitan option not in jisho v1), non-Japanese languages (en, ko, la).

## Steps

1.  [x] Port data with node: transforms (done), romaji table, NFKD
        tables, kanji variants; port test data (transform tests, translator
        tests and valid-dictionary1) into tests/fixtures/yomitan with NOTICE.
2.  [x] jisho-ja: transformer + transform table; transform tests pass.
3.  [x] jisho-ja: kana/width/combining/emphatic/romaji/CJK/kanji-variant
        functions and the 9 preprocessors; differential test against
        Yomitan's own functions on 16k inputs (scripts/port/dump-ja-golden.mjs)
        instead of transcribing test/japanese-util.test.js.
4.  [x] jisho-store: `form_of` and `content_count` columns at import;
        bulk term query returning the matched expression/reading.
5.  [x] jisho-lookup: variants, deinflections, bulk matching, dictionary
        deinflections, entry creation, duplicate handling, sort.
6.  [x] Translator parity test (subset above).
7.  [x] API `mode: "scan"` (default) and CLI `lookup` using it; response
        gains sources and rule chains; docs/api.md updated.
8.  [x] bench with real scans; record numbers; tune if targets missed.
9.  [x] Independent review; fix findings.

## Results so far

- Translator parity: all 32 in-scope cases (Japanese, split mode, no text
  replacements) match Yomitan's expected results exactly, except the size
  keys of image glossary items (width, height, preferredWidth,
  preferredHeight), which Yomitan rewrites at import from the image files
  and jisho, being text only, keeps as the dictionary states them.
  Mutations (no sort; reading claims before term) make 4 and 3 cases fail.
- The task description said 38 cases; the correct count is 32 (the other
  split cases use textReplacements or other languages).
- Default removeNonJapaneseCharacters is false, as in Yomitan with its
  default scanning.alphanumeric = true (the translator test preset sets it
  to true; the parity test passes its value through). This makes romaji
  input work.
- Exact mode is Yomitan's deinflect = false: only the whole text, no
  variants.
- Public corpus (JMdict + KANJIDIC), 967 real scans of 16 chars: find and
  rank p50 0.71 ms / p99 1.95 ms; full lookup with glossaries of 100
  entries p50 1.00 ms / p99 2.75 ms.

## Measurements (2026-11-10)

User collection (99 dictionaries, 9.06M terms, store v2), 967 real scans
of 16 chars, 3 runs: find and rank p50 2.29 ms / p99 14.4-14.9 ms; full
lookup with glossaries of 32 entries p50 3.44-3.49 ms / p99 17.4-17.6 ms.
Targets (p50 5 ms, p99 20 ms) met. With 100 entries the full lookup was
p50 6.9 / p99 23.5 ms; 32 is Yomitan's default maxResults. Import of all
104 zips: 723 s (run alongside the review agent), peak 91 MB.

## Independent review (2026-11-10)

The reviewer ran Yomitan's translator.js in node against the same JMdict
data with an IndexedDB fake reproducing findTermsBulk's claim order: 1,917
generated inputs matched except the documented collation tie-break and
finding 2.

| Id | Finding                                                         | Resolution                                                       |
|----|-----------------------------------------------------------------|------------------------------------------------------------------|
| 1  | 1000-char repetitive input: n^3-n^4 time/memory (300 chars: 9 s, 2.3 GB); response cap ignored chains | Scan length (Yomitan scanning.length, default 16, max 32); whole serialized entries count against the byte cap. Worst repetitive 1000-char inputs now 10-50 ms |
| 2  | Yomitan shares chain arrays between entries of a deinflection; merges change them all (order differs for a crafted case) | Chain lists kept per deinflection by id and shared (dictionary deinflections share the parent's text-processor list); regression test with the reviewer's case |
| 3  | Schema changed without a version bump                           | STORE_VERSION 2; message no longer mentions the missing reimport command |
| 4  | Hit cap dropped rows silently                                   | find_terms_bulk reports it; lookup sets `truncated`              |
| 5  | Empty chains dropped from JSON                                  | Kept; docs/api.md explains them                                  |
| 6  | Lone-surrogate prefixes skipped                                 | No change: equivalent because import rejects lone surrogates (noted) |

Also from profiling: the enabled-dictionary list is read with a cheap query
instead of parsing every index.json per lookup.
