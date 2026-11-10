# jisho HTTP API

Status: draft. Shapes may change until jisho 1.0; after that, breaking
changes need a new version prefix (`/v2`).

`jisho serve` exposes this API. Default address `127.0.0.1:47613`;
`--bind <addr:port>` changes it. There is no authentication.

## Conventions

- Every path starts with `/v1`.
- Requests with a body use `POST`/`PUT` and `Content-Type:
  application/json` (UTF-8), except dictionary upload, which uses
  `application/zip`. Any other content type gets `415`.
- Responses are JSON (`application/json; charset=utf-8`).
- Field names are `snake_case`. Unknown request fields are rejected
  (`400`), so typos do not silently change behaviour.
- Text positions and lengths are given in two units, because clients
  differ: `utf16` (JavaScript string indexes, what Yomitan uses) and
  `chars` (Unicode scalar values, what Rust and Python index by).

### Errors

Every error response has this body:

```json
{"error": {"code": "read_only", "message": "server is running with --read-only"}}
```

| HTTP | `code`                    | When                                                  |
|------|---------------------------|-------------------------------------------------------|
| 400  | `bad_request`             | Malformed JSON, unknown field, invalid value          |
| 403  | `read_only`               | Mutating request while `--read-only`                  |
| 403  | `forbidden_origin`        | `Origin` header not in the allowlist                  |
| 403  | `forbidden_host`          | `Host` header not allowed (DNS rebinding guard)       |
| 404  | `not_found`               | Unknown path, dictionary or job                       |
| 405  | `method_not_allowed`      | Wrong method for the path                             |
| 409  | `conflict`                | Dictionary title already imported                     |
| 409  | `busy`                    | Another import is running (one at a time)             |
| 413  | `payload_too_large`       | Body over its cap                                     |
| 415  | `unsupported_media_type`  | Wrong `Content-Type`                                  |
| 422  | `invalid_dictionary`      | The zip is not a valid dictionary                     |
| 500  | `internal`                | Bug or storage failure                                |
| 503  | `timeout`                 | Request exceeded its time limit                       |

### Limits

| Limit                           | Default | Behaviour when exceeded          |
|---------------------------------|---------|----------------------------------|
| JSON request body               | 64 KiB  | `413`                            |
| Lookup `text`                   | 1000 chars | `400`                         |
| Results per lookup (`limit`)    | 32, max 1000 | Extra results dropped, `truncated: true` |
| Response body                   | 16 MiB  | Results dropped from the end, `truncated: true` |
| Request time                    | 10 s    | `503 timeout`                    |
| Dictionary upload               | 4 GiB (`--max-upload`) | `413`             |

### Browser access

- `Origin`: requests without an `Origin` header (curl, scripts, apps) are
  allowed. Requests with one are allowed only if the origin is in the
  `allowed_origins` config list; allowed origins get CORS headers.
- `Host`: when bound to a loopback address, `Host` must be `localhost`,
  `127.0.0.1` or `[::1]` (with or without the port). When bound to a
  specific non-loopback address, `Host` must be that address or a name in
  `allowed_hosts`. When bound to a wildcard address (`0.0.0.0`, `[::]`)
  the check applies only if `allowed_hosts` is set.

## Endpoints

| Method | Path                         | Milestone | Mutates |
|--------|------------------------------|-----------|---------|
| GET    | `/v1/health`                 | 1         | no      |
| POST   | `/v1/terms/lookup`           | 1 (exact), 2 (scan), 3 (grouping, metadata) | no |
| GET    | `/v1/dictionaries`           | 1         | no      |
| POST   | `/v1/kanji/lookup`           | 3         | no      |
| POST   | `/v1/text/parse`             | 4         | no      |
| POST   | `/v1/dictionaries`           | 4         | yes     |
| DELETE | `/v1/dictionaries/{id}`      | 4         | yes     |
| PUT    | `/v1/dictionaries/order`     | 4         | yes     |
| GET    | `/v1/jobs/{id}`              | 4         | no      |
| DELETE | `/v1/jobs/{id}`              | 4         | yes     |
| GET    | `/v1/source`                 | 4         | no      |

With `--read-only`, every "Mutates: yes" endpoint returns `403 read_only`.

### GET /v1/health

```json
{"status": "ok", "version": "0.1.0", "read_only": false}
```

### POST /v1/terms/lookup

Request:

```json
{"text": "tabetakunai", "mode": "scan", "limit": 100}
```

| Field             | Type   | Default  | Meaning                                              |
|-------------------|--------|----------|------------------------------------------------------|
| `text`            | string | required | Text to look up (1 to 1000 chars)                    |
| `mode`            | string | `"scan"` | `"scan"`: Yomitan-style lookup of the start of `text`: its longest prefixes, with inflections undone and kana, width, romaji and kanji-variant forms tried. `"exact"`: entries whose term or reading is exactly `text` |
| `limit`           | int    | 32       | Maximum entries returned (1 to 1000; 32 is Yomitan's maxResults) |
| `result_mode`     | string | `"group"` | How entries are grouped (Yomitan's result modes): `"group"`: one entry per term, reading and inflection chain, with the definitions of every dictionary; `"merge"`: the main dictionary's entries grouped by sequence number, with other spellings of the same word and, by headword, other dictionaries' definitions; `"term"`: one entry per term and inflection chain; `"split"`: one headword and one definition per entry; `"simple"`: like split, without tags, and without frequencies or pronunciations except those of the sort-frequency dictionary when one is set (cheapest) |
| `primary_reading` | string | none     | Entries with this reading sort first                 |
| `scan_length`     | int    | 16       | Scan mode looks at only this many characters from the start of `text` (1 to 32; 16 is Yomitan's scanning length). The cost of a scan grows steeply with its length on repetitive input, hence the cap |

Entries come in Yomitan's order (longest match first, then fewer
transformations, exact matches, the sort-frequency dictionary's order if
one is set, dictionary order, score, headword text, ...). The final
tie-break compares headwords like Yomitan's `Intl.Collator('en-US')`
(exact for the characters jisho's table covers; see
crates/jisho-lookup/src/collation.rs).

Response:

```json
{
  "entries": [
    {
      "headwords": [
        {
          "term": "...",
          "reading": "...",
          "sources": [
            {
              "original_text": "...",
              "transformed_text": "...",
              "deinflected_text": "...",
              "match_type": "exact",
              "match_source": "term",
              "is_primary": true
            }
          ],
          "tags": [
            {
              "name": "P",
              "category": "popular",
              "order": -10,
              "score": 10,
              "notes": ["common word"],
              "dictionaries": ["JMdict"],
              "redundant": false
            }
          ],
          "word_classes": ["v1"]
        }
      ],
      "definitions": [
        {
          "dictionary": {"id": 1, "title": "JMdict"},
          "headword_indices": [0],
          "id": 1234,
          "score": 0,
          "sequences": [1358280],
          "is_primary": true,
          "tags": [],
          "glossary": ["to eat"],
          "glossary_text": ["to eat"]
        }
      ],
      "inflections": [
        {"source": "algorithm", "rules": [{"name": "negative", "description": "..."}]}
      ],
      "text_processors": [["convertHiraganaToKatakana"]],
      "score": 0,
      "dictionary_index": 0,
      "is_primary": true,
      "max_original_text_length": 5,
      "frequencies": [
        {
          "headword_index": 0,
          "dictionary": {"id": 3, "title": "JPDB"},
          "frequency": 1234,
          "display_value": null,
          "display_value_parsed": false,
          "has_reading": true,
          "frequency_mode": "rank-based"
        }
      ],
      "pronunciations": [
        {
          "headword_index": 0,
          "dictionary": {"id": 4, "title": "NHK"},
          "items": [
            {"type": "pitch_accent", "position": 2, "nasal_positions": [], "devoice_positions": [], "tags": []},
            {"type": "phonetic_transcription", "ipa": "...", "tags": []}
          ]
        }
      ]
    }
  ],
  "matched": {"text": "...", "length": {"utf16": 5, "chars": 5}},
  "truncated": false
}
```

- An entry has one or more headwords and one or more definitions
  (exactly one of each in `split` and `simple` mode). A definition's
  `headword_indices` say which headwords it belongs to; frequencies and
  pronunciations name theirs with `headword_index`.
- `sources[].original_text` is the input prefix that matched;
  `transformed_text` is it after text preprocessing; `deinflected_text` is
  what was looked up. `match_source` says whether the term or the reading
  matched (`"sequence"` for entries merge mode adds because they share the
  main dictionary's sequence number; those have `is_primary: false`).
- `inflections` lists one chain per way the entry was reached; each chain
  starts at the inflection nearest the dictionary form (Yomitan's order).
  An empty chain means the entry was also reached without undoing any
  inflection.
- `text_processors` lists the preprocessors applied (Yomitan's ids), one
  chain per way; an empty chain means the text also matched as typed.
- Tags come from the dictionaries' tag banks; `redundant` marks a
  part-of-speech tag that repeats the previous definition's (Yomitan hides
  those). Tags are empty in `simple` mode, also on the pronunciations it
  returns for the sort-frequency dictionary.
- `frequency` is a number; when the dictionary gives a string, it is the
  first number in it and `display_value` is the string
  (`display_value_parsed: true`). `has_reading` says the value is for this
  reading only.
- Pitch `position` is the downstep position (a number) or an H/L pattern
  (a string).
- `truncated` is also true when a lookup matched more rows than the
  internal safety cap (50,000), so results may be incomplete.
- `glossary` is the dictionary's glossary as imported, without Yomitan's
  deinflection-reference items (entries merge mode adds by sequence keep
  them, as in Yomitan); `glossary_text` has one plain-text rendering per
  item.
- `matched.text` is the longest input prefix that produced an entry.

### POST /v1/kanji/lookup

Request: `{"text": "\u98df", "limit": 32}`. Each distinct character of `text`
(1 to 100 characters) is looked up in the enabled kanji dictionaries
(Yomitan's findKanji); results in dictionary order, then character order.
`limit` (default 32, max 1000) caps the entries; `truncated` says some were
dropped.

```json
{
  "entries": [
    {
      "character": "\u98df",
      "dictionary": {"id": 2, "title": "KANJIDIC"},
      "onyomi": ["\u30b7\u30e7\u30af", "\u30b8\u30ad"],
      "kunyomi": ["\u304f.\u3046", "\u305f.\u3079\u308b"],
      "tags": [],
      "stats": [
        {"category": "misc", "stats": [{"name": "strokes", "category": "misc", "notes": "Stroke count", "order": 0, "score": 0, "value": "9"}]}
      ],
      "meanings": ["eat", "food"],
      "frequencies": [
        {"dictionary": {"id": 5, "title": "JPDB Kanji"}, "frequency": 328, "display_value": null, "display_value_parsed": false, "frequency_mode": null}
      ]
    }
  ],
  "truncated": false
}
```

Stats are the kanji's stats that have a tag of the same name in the
dictionary, grouped by the tag's category in the dictionary's order.

### GET /v1/dictionaries

```json
{
  "dictionaries": [
    {
      "id": 1,
      "title": "JMdict",
      "revision": "2026-11-01",
      "format": 3,
      "sequenced": true,
      "enabled": true,
      "priority": 0,
      "alias": null,
      "allow_secondary_searches": false,
      "parts_of_speech_filter": true,
      "use_deinflections": true,
      "author": "...",
      "url": "...",
      "description": "...",
      "attribution": "...",
      "source_language": null,
      "target_language": null,
      "frequency_mode": null,
      "counts": {"terms": 0, "term_meta": 0, "kanji": 0, "kanji_meta": 0, "tags": 0},
      "has_styles": false,
      "imported_at": "2026-11-10T12:00:00Z"
    }
  ],
  "lookup_settings": {
    "main_dictionary": "JMdict",
    "sort_frequency_dictionary": null,
    "sort_frequency_order": "descending"
  }
}
```

Dictionaries are listed in lookup order. The per-dictionary settings and
`lookup_settings` are Yomitan's dictionary settings and its `general`
main/sort-frequency dictionary options; the CLI changes them (`jisho
dictionary`, `jisho order`, `jisho settings`), the API from milestone 4.
`main_dictionary` is set at import to the first dictionary with sequence
numbers, as Yomitan's settings page does; removing that dictionary clears
it (Yomitan keeps the stale name), so the next sequenced import takes its
place.

### Milestone 4 endpoints

Specified when their milestone starts; their rows above fix the paths and
whether they mutate.
