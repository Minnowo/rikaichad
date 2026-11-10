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
| Results per lookup (`limit`)    | 100, max 1000 | Extra results dropped, `truncated: true` |
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
| POST   | `/v1/terms/lookup`           | 1 (exact), 2-3 (scan) | no |
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
{"text": "taberu", "mode": "exact", "limit": 100}
```

| Field   | Type   | Default  | Meaning                                              |
|---------|--------|----------|------------------------------------------------------|
| `text`  | string | required | Text to look up (1 to 1000 chars)                    |
| `mode`  | string | `"scan"` | `"exact"`: entries whose term or reading equals `text`. `"scan"`: Yomitan-style scan from the start of `text` (milestone 2) |
| `limit` | int    | 100      | Maximum entries returned (1 to 1000)                 |

Milestone 1 implements `mode: "exact"` only; `"scan"` returns `400` until
milestone 2.

Response (milestone 1 shape; milestone 3 adds grouping, inflection
chains, frequencies and pitch):

```json
{
  "entries": [
    {
      "dictionary": {"id": 1, "title": "JMdict"},
      "term": "...",
      "reading": "...",
      "definition_tags": ["v1", "vt"],
      "rules": ["v1"],
      "score": 0,
      "sequence": 1358280,
      "term_tags": ["P"],
      "glossary": ["to eat", {"type": "structured-content", "content": []}],
      "glossary_text": ["to eat", "..."]
    }
  ],
  "matched": {"text": "...", "length": {"utf16": 3, "chars": 3}},
  "truncated": false
}
```

- `glossary` is the dictionary's glossary array as imported (raw JSON;
  `{type: "text"}` items are already flattened to strings, as in
  Yomitan). Clients that render structured content use this.
- `glossary_text` has one plain-text rendering per `glossary` item.
- Entries are ordered by dictionary order, then import order.

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
  ]
}
```

### Milestone 3 and 4 endpoints

Specified when their milestone starts; their rows above fix the paths and
whether they mutate.
