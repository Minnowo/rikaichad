# Reference documents

Authoritative references downloaded for this project. Fetched 2026-11-10.
These are third-party files under their own licenses (GPL-3.0-or-later for
both Yomitan repositories).

| Path                              | Source                                                              | Version                                          |
|-----------------------------------|---------------------------------------------------------------------|--------------------------------------------------|
| `yomitan-schemas/*.json`          | https://github.com/yomidevs/yomitan/tree/master/ext/data/schemas    | yomitan@ee81f8dcf13e985bb4d7961009fb49a764b4d924 |
| `making-yomitan-dictionaries.md`  | https://github.com/yomidevs/yomitan/blob/master/docs/               | yomitan@ee81f8dcf13e985bb4d7961009fb49a764b4d924 |
| `gpl-3.0.txt`, `agpl-3.0.txt`    | https://www.gnu.org/licenses/                                       | GPLv3, AGPLv3 (fetched 2026-11-10)               |
| `yomitan-api/*.md`                | https://github.com/yomidevs/yomitan-api/tree/master/docs/api_paths  | yomitan-api@88b9d85c1d34f07297fdaea56f93861b68afeaf1 |

Notes:

- The JSON schemas are the dictionary format spec (index, term bank v3,
  term meta bank v3, kanji bank v3, kanji meta bank v3, tag bank v3).
- The yomitan-api docs describe the HTTP API that Yomitan exposes through
  its native messaging component (default 127.0.0.1:19633, POST only).
  They state the responses are Yomitan's internal format with no stability
  guarantee.

## Source checkouts (not committed)

`reference-src/` in the project root holds read-only clones for porting
and for checking behaviour. It is git-ignored.

| Path                       | Repository                                  | Pinned at                                  |
|----------------------------|---------------------------------------------|--------------------------------------------|
| `reference-src/yomitan`    | https://github.com/yomidevs/yomitan         | ee81f8dcf13e985bb4d7961009fb49a764b4d924   |
| `reference-src/zipjs`      | https://github.com/gildas-lormeau/zip.js    | tag v2.7.54 (the version Yomitan locks)    |
| `reference-src/zip2`       | https://github.com/zip-rs/zip2              | b03a868e (zip crate 9.5.0)                 |
