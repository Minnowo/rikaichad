# rikaichad HTTP API

Status: draft. Shapes may change until rikaichad 1.0; after that, breaking
changes need a new version prefix (`/v2`).

`rchad serve` exposes this API. Default address `127.0.0.1:47613`;
`--bind <addr:port>` changes it. There is no authentication.

## Conventions

- Every API path starts with `/v1` (the web UI is served at `/` and
  `/ui/`).
- Requests with a body use `POST`/`PUT` and `Content-Type:
  application/json` (UTF-8), except dictionary upload, which uses
  `application/zip`. Any other content type gets `415`.
- Responses are JSON (`application/json; charset=utf-8`), except
  dictionary styles (CSS) and the web UI's files.
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
| 408  | `timeout`                 | An upload stopped sending data or is too slow         |
| 409  | `conflict`                | Dictionary title already imported                     |
| 409  | `busy`                    | Another import, removal or settings change is running (one write at a time) |
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
| Request time                    | 10 s (not for uploads and removals) | `503 timeout` |
| Dictionary upload               | 4 GiB (`--max-upload`) | `413`             |
| Upload with no data arriving    | 27 s (just under the 30 s connection idle limit) | `408 timeout` |
| Upload slower than 64 KiB/s on average, after 60 s | - | `408 timeout`   |
| Parse `text`                    | 1000 chars | `400`                         |

### Browser access

- `Origin`: requests without an `Origin` header (curl, scripts, apps) are
  allowed. Requests with one are allowed if the origin is the server's
  own (`http://` or `https://` followed by exactly the request's `Host`
  header, which is checked first) and that Host is a loopback name, an IP
  literal or listed in `allowed_hosts`: the web UI's requests. (A DNS name
  must be listed because on a wildcard bind any Host passes, which DNS
  rebinding would exploit.) A reverse proxy must keep the original `Host`.
  Other origins must be in the `allowed_origins` config list; they get
  CORS headers.
- `Host`: when bound to a loopback address, `Host` must be `localhost`,
  `127.0.0.1` or `[::1]` (with or without the port). When bound to a
  specific non-loopback address, `Host` must be that address or a name in
  `allowed_hosts`. When bound to a wildcard address (`0.0.0.0`, `[::]`)
  the check applies only if `allowed_hosts` is set.

## Endpoints

| Method | Path                           | Milestone                                   | Mutates |
|--------|--------------------------------|---------------------------------------------|---------|
| GET    | `/v1/health`                   | 1                                           | no      |
| POST   | `/v1/terms/lookup`             | 1 (exact), 2 (scan), 3 (grouping, metadata) | no      |
| GET    | `/v1/dictionaries`             | 1                                           | no      |
| POST   | `/v1/kanji/lookup`             | 3                                           | no      |
| POST   | `/v1/text/parse`               | 4                                           | no      |
| POST   | `/v1/dictionaries`             | 4                                           | yes     |
| DELETE | `/v1/dictionaries/{id}`        | 4                                           | yes     |
| PATCH  | `/v1/dictionaries/{id}`        | 4                                           | yes     |
| PUT    | `/v1/dictionaries/order`       | 4                                           | yes     |
| GET    | `/v1/settings`                 | 4                                           | no      |
| PATCH  | `/v1/settings`                 | 4                                           | yes     |
| GET    | `/v1/jobs/{id}`                | 4                                           | no      |
| DELETE | `/v1/jobs/{id}`                | 4                                           | yes     |
| GET    | `/v1/source`                   | 4                                           | no      |
| GET    | `/v1/dictionaries/{id}/styles` | 6                                           | no      |
| GET    | `/`, `/ui/{file}`              | 6 (web UI)                                  | no      |
| POST   | `/v1/terms/forms`              | 7 (conjugation tables)                      | no      |

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
(exact for the characters rikaichad's table covers; see
crates/rikaichad-lookup/src/collation.rs).

Response:

```json
{
  "entries": [
    {
      "headwords": [
        {
          "term": "...",
          "reading": "...",
          "furigana": [{"text": "...", "reading": "..."}],
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
- `furigana` splits the headword's term into pieces with their readings
  (Yomitan's distributeFurigana); an empty `reading` means the piece is
  read as written.
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
  item, made at import: block elements start new lines, list items become
  `- ` lines (`1. `, `2. `, ... in ordered lists), table cells are joined
  with ` | `, ruby becomes `base(reading)` unless the dictionary gives its
  own `rp` parentheses, images become their alt, title or description
  text, or `[image]`. Yomitan itself renders HTML; clients that need its
  look use `glossary`.
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
main/sort-frequency dictionary options; the CLI changes them (`rchad
dictionary`, `rchad order`, `rchad settings`), the API from milestone 4.
`main_dictionary` is set at import to the first dictionary with sequence
numbers, as Yomitan's settings page does; removing that dictionary clears
it (Yomitan keeps the stale name), so the next sequenced import takes its
place.

### GET /v1/dictionaries/{id}/styles

The dictionary's `styles.css` as imported, `text/css; charset=utf-8`;
`404 not_found` when the dictionary has none (`has_styles: false`) or
does not exist. Yomitan applies it to the dictionary's definitions only,
nested in `[data-dictionary="title"] { ... }`; the web UI does the same
with `[data-dictionary-id="id"]`. The CSS is the dictionary author's:
clients should scope it and not let it load resources.

### POST /v1/text/parse

Splits a text into words with Yomitan's scanning parser: from each
position, the longest match of a lookup (`"simple"` result mode) is one
token, with furigana; characters nothing matches form tokens of their
own. One lookup per token, so `text` is capped at 1000 characters.

Request: `{"text": "...", "scan_length": 16}`. `scan_length` (default 16,
1 to 32) is how far each lookup reads, in UTF-16 units as in Yomitan.

```json
{
  "tokens": [
    {
      "text": "\u98df\u3079\u305f",
      "segments": [{"text": "\u98df", "reading": "\u305f"}, {"text": "\u3079\u305f", "reading": ""}],
      "headwords": [[{"term": "\u98df\u3079\u308b", "reading": "\u305f\u3079\u308b"}]]
    },
    {"text": "\u3002", "segments": [{"text": "\u3002", "reading": ""}], "headwords": []}
  ]
}
```

- `segments` split the token for furigana; an empty `reading` means the
  piece is read as written.
- `headwords` are those whose lookup matched exactly this token, grouped
  per result entry in result order; empty for unmatched text.

### POST /v1/terms/forms

The conjugation table of a verb or i-adjective, generated from the
headword alone (the store is not read). Pass a headword as
`/v1/terms/lookup` returns it.

Request: `{"term": "...", "reading": "...", "word_classes": ["v5"]}`.
`reading` may be omitted or empty only when the term is all kana.
`term` and `reading` are at most 64 characters, `word_classes` at most
32 entries of at most 32 characters.

The first supported class in `word_classes` is used: `v1`, `v5`, `vk`,
`vs` (suru verbs, and nouns that take suru: `term` gets suru appended)
or `adj-i`. Other classes give `word_class: null` and no forms (status
200).

```json
{
  "word_class": "v5",
  "term": "\u66f8\u304f",
  "reading": "\u304b\u304f",
  "forms": [
    {"name": "past", "polite": false, "negative": false,
     "term": "\u66f8\u3044\u305f", "reading": "\u304b\u3044\u305f",
     "furigana": [{"text": "\u66f8", "reading": "\u304b"}, {"text": "\u3044\u305f", "reading": ""}]}
  ]
}
```

- `name` is one of, in table order: `non-past`, `past`, `te`,
  `adverbial`, `conditional` (-ba), `tara`, `volitional`, `imperative`,
  `desire` (-tai), `progressive` (-te iru), `potential`, `passive`,
  `causative`, `causative-passive`, `noun` (-sa). Forms come in that
  order, plain before polite and affirmative before negative. One
  `(name, polite, negative)` cell can hold several forms (-nakute and
  -naide), and not every word has every cell (aru has no progressive,
  potential, passive or causative; the honorific verbs have no -tai or
  -nasai forms; gozaru has no imperative).
- suru's potential is the verb dekiru (`benkyou dekiru`). The kanji
  spelling of suru is returned in kana.
- Irregular words: iku/yuku (itte), aru (negative nai), the -u verbs
  that take -ute (tou, kou, notamau -> notamoute; the same list Yomitan
  deinflects), the honorific verbs irassharu, ossharu, kudasaru,
  nasaru, gozaru (-imasu, irasshai), kureru (imperative kure), kuru,
  suru, ii (yoku).
- Not handled: single-kanji suru verbs that partly conjugate like godan
  (aisuru -> aisanai, aiseru). Dictionaries tag them plain `vs`, so they
  get the regular suru table (aishinai).
- Lookup deinflects every form back to the dictionary form except:
  separate words at the end (-naide, prohibitive -na, -desu), aru's
  negatives (found under nai), ii's forms (under yoi), yuku's itte
  forms (under iku), suru's potential (dekiru), and the honorific -i
  imperative and the -imasu forms of honorific compounds.

### POST /v1/dictionaries

Uploads a dictionary zip and imports it in the background.

- Body: the zip, `Content-Type: application/zip`. The optional query
  parameter `filename` (e.g. `?filename=JMdict.zip`) is recorded as the
  source file name (a base name; `upload.zip` if missing).
- The upload is saved next to the store file, so the store's disk needs
  room for it, and deleted after the import.
- One write runs at a time (an import or removal, or a dictionary or
  settings change): otherwise `409 busy`, before the body is read. The
  upload holds the slot while it is being received.
- Not subject to the 10 s request limit; it fails with `408 timeout` if no
  data arrives for 27 s or it averages under 64 KiB/s after its first
  minute, and `413` past `--max-upload`.
- Stopping the server (Ctrl-C) cancels a running import; it rolls back.
- Response `202 Accepted`, `Location: /v1/jobs/{id}`, body `{"job": ...}`
  as for `GET /v1/jobs/{id}`.

### GET /v1/jobs/{id}

```json
{
  "job": {
    "id": 1,
    "kind": "import",
    "state": "succeeded",
    "source_name": "JMdict.zip",
    "progress": {"files_done": 12, "files_total": 12, "rows": 527699},
    "created_at": "2026-11-10T12:00:00Z",
    "finished_at": "2026-11-10T12:00:13Z",
    "dictionary": {"id": 3, "title": "JMdict", "...": "as in GET /v1/dictionaries"},
    "warnings": [],
    "error": null
  }
}
```

- `state`: `running`, `succeeded` (`dictionary` set), `failed` (`error`
  has the `code` and `message` the error envelope would carry, e.g.
  `conflict` for a title already imported, `invalid_dictionary`) or
  `cancelled`.
- `progress` is updated after each bank file.
- The 32 most recent finished jobs are kept; jobs are lost when the
  server restarts (the import itself is either committed or not).

### DELETE /v1/jobs/{id}

A running job is cancelled (`202`, `state` still `running`; it ends as
`cancelled` within about 10,000 rows and nothing is imported, unless the
import was already committing, in which case it ends `succeeded`). A
finished job is forgotten (`200`). Either way the body is `{"job": ...}` as
it was.

### DELETE /v1/dictionaries/{id}

Removes the dictionary and all its entries; `200`
`{"removed": <dictionary as in GET /v1/dictionaries>}`. Takes the write
slot like an import (`409 busy` while one runs) and is not subject to the
10 s request limit: removing a large dictionary takes a while.

### PATCH /v1/dictionaries/{id}

Changes how lookups use a dictionary. Every field is optional; only the
given ones change.

```json
{"enabled": false, "alias": "JM", "allow_secondary_searches": true,
 "parts_of_speech_filter": true, "use_deinflections": true}
```

`"alias": null` goes back to the title. The change is one transaction.
Response: the dictionary as in `GET /v1/dictionaries`. Like the other
writes below, `409 busy` while an import or removal runs. An id that is
not a number is `400`.

### PUT /v1/dictionaries/order

`{"ids": [3, 1]}` puts these dictionaries first, in this order; the others
keep their order after them. An unknown id is `404`, a repeated one `400`.
Response: as `GET /v1/dictionaries`.

### GET /v1/settings, PATCH /v1/settings

```json
{"main_dictionary": "JMdict", "sort_frequency_dictionary": null, "sort_frequency_order": "descending"}
```

`PATCH` takes the same fields, all optional; a dictionary given as `null`
clears the setting, a title that is not imported is `400`. Response: the
settings.

### GET /v1/source

Where to get the source of this server (AGPL-3.0 section 13):

```json
{"name": "rikaichad", "version": "0.1.0", "license": "AGPL-3.0-or-later AND GPL-3.0-or-later",
 "repository": "https://...", "commit": "80747dc...", "dirty": "false"}
```

`commit` is the git commit the binary was built from (`"unknown"`
outside a checkout unless `RIKAICHAD_GIT_COMMIT` is set at build time);
`dirty` says whether the build had uncommitted changes. `repository` is
`--source-url`, or `RIKAICHAD_SOURCE_URL` at build time, or null. Anyone who
runs a modified rikaichad for others must publish their source and set it.

### GET /, GET /ui/{file}

The web UI (docs/plan/milestone-6.md): `/` is its page, `/ui/{file}` its
scripts and stylesheet, built into the binary from ui/dist. Responses
carry `Content-Security-Policy: default-src 'none'; script-src 'self';
style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none';
form-action 'none'; frame-ancestors 'none'`, `X-Content-Type-Options:
nosniff`, `Referrer-Policy: no-referrer` and `Cache-Control: no-cache`.
Unknown files are `404 not_found`. Served in `--read-only` mode too (the
UI only reads).
