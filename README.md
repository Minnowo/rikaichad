# jisho

A Japanese dictionary engine that reads Yomitan dictionaries
(https://github.com/yomidevs/yomitan) and reproduces Yomitan's lookups
outside the browser, as a command-line tool and a local HTTP/JSON API.
Text only: images and audio inside dictionaries are skipped.

Status: early development (milestone 1 of 5). Import, exact lookup, the
dictionary list and the HTTP server work. Inflected lookups
(deinflection, Yomitan-style scanning) come in milestone 2; see
`docs/brief.md` for the plan.

## Build

Requires Rust 1.99 or newer and a C compiler (SQLite is built from source).

```sh
cargo build --release
./target/release/jisho --help
```

## Use

```sh
# Import Yomitan dictionary zips (format 3) into the store.
jisho import JMdict.zip KANJIDIC.zip

jisho list                    # imported dictionaries, in lookup order
jisho lookup WORD             # exact term or reading
jisho lookup --json WORD      # same JSON as the HTTP API
jisho remove JMdict           # by title or by the id from `jisho list`

# HTTP API on 127.0.0.1:47613 (docs/api.md). No authentication.
jisho serve
jisho serve --bind 0.0.0.0:47613 --read-only
```

The store is one SQLite file: `$JISHO_STORE`, or `--store PATH`, or by
default `~/.local/share/jisho/jisho.sqlite` (Linux),
`~/Library/Application Support/jisho/jisho.sqlite` (macOS),
`%APPDATA%\jisho\jisho.sqlite` (Windows). It can be copied to another
machine as is. Keep your dictionary zips: they are the source of truth if
the store ever needs rebuilding.

## Develop

```sh
scripts/check.sh    # format, lint, tests, ASCII, license and dependency checks
jisho bench         # lookup latency on your store (bench/sentences.txt)
```

Read `AGENTS.md` before contributing.

## License

AGPL-3.0-or-later. Files ported from Yomitan stay under
GPL-3.0-or-later; see `NOTICE`.
