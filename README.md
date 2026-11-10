# jisho

A Japanese dictionary engine that reads Yomitan dictionaries
(https://github.com/yomidevs/yomitan) and reproduces Yomitan's lookups
(deinflection, scanning, ranking) outside the browser, as a command-line
tool and a local HTTP/JSON API. Text only.

Status: early development. Nothing beyond `--help` and `--version` works
yet; see `docs/brief.md` for the plan and milestones.

## Build

Requires Rust 1.99 or newer and a C compiler (SQLite is built from source).

```sh
cargo build --release
./target/release/jisho --help
```

## Develop

```sh
scripts/check.sh    # format, lint, tests, ASCII, license and dependency checks
```

Read `AGENTS.md` before contributing.

## License

AGPL-3.0-or-later. Files ported from Yomitan stay under
GPL-3.0-or-later; see `NOTICE`.
