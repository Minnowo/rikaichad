# jisho

A Japanese dictionary engine for Yomitan dictionaries
(https://github.com/yomidevs/yomitan): it imports the same dictionary zips
and looks words up the way Yomitan does, outside the browser, as a
command-line tool and a local HTTP/JSON API. Text only: images and audio
inside dictionaries are skipped.

The lookup is a port of Yomitan's own code (its Japanese deinflection
rules, text normalization, translator and sentence parser), checked
against Yomitan itself: its test suite, and a differential run of its real
translator against jisho on the same data (docs/plan/milestone-3.md).

## What it does

- **Import** Yomitan dictionary zips (format 3): terms with
  structured-content glossaries, frequencies, pitch accents, IPA, kanji,
  tags. Dictionaries are stored compressed in one SQLite file you can copy
  to another machine.
- **Look up** the start of a text like Yomitan's popup: longest match,
  inflections undone (taberarenakatta finds taberu through "potential or
  passive < negative < -ta"), kana/width/romaji variants tried.
- **Group** results like Yomitan: by word (`group`, the default), by the
  main dictionary's entries (`merge`), by term, or one entry per
  definition (`split`), with tags, frequencies and pitch accents.
- **Kanji** readings, meanings, stats and frequencies.
- **Parse** sentences into words with furigana.
- **Serve** all of it over HTTP (docs/api.md), including dictionary
  upload, removal, order and settings, with a `--read-only` mode.

Measured on 99 dictionaries (9 million entries): a full grouped lookup of
a 16-character scan takes 9 ms at the median, 3.6 GB of store
(docs/plan/milestone-3.md).

## Quick start

Build from source (see docs/install.md for requirements and options):

```sh
cargo build --release
cp target/release/jisho ~/.local/bin/    # or anywhere on your PATH
```

Then:

```sh
jisho import JMdict_english.zip KANJIDIC_english.zip
jisho lookup taberarenakatta     # Japanese text or romaji
jisho kanji KANJI                # e.g. a kanji character
jisho parse SENTENCE             # words with furigana
jisho lookup --result-mode merge --json iku   # the API's JSON

jisho list                       # dictionaries, in lookup order
jisho order JMdict               # put JMdict first
jisho dictionary 3 --disable     # stop using dictionary 3
jisho settings --sort-frequency JPDB --sort-frequency-order ascending
jisho remove JMdict

jisho serve                      # HTTP API on 127.0.0.1:47613
```

`jisho --help` and `jisho COMMAND --help` list everything.

## Documentation

- docs/install.md: requirements, building, the store, upgrading, running
  the server, uninstalling.
- docs/api.md: the HTTP API.
- docs/brief.md: what the project is and why it is built this way.
- docs/plan/: milestone plans, measurements, parity results and the
  known differences from Yomitan.

## Status

Version 0.1.0, before a first release. The store format may change between
versions; jisho then refuses the old store and you import your dictionaries
again (keep your zips). The HTTP API may change until 1.0.

## Develop

```sh
scripts/check.sh    # format, lint, tests, ASCII, license and dependency checks
jisho bench         # lookup latency on your store (bench/sentences.txt)
```

Read AGENTS.md before contributing.

## License

AGPL-3.0-or-later (LICENSE). Files ported from Yomitan stay under
GPL-3.0-or-later; see NOTICE for them and for third-party data, and
docs/install.md for what to ship with a binary. If you
run a modified jisho as a network service for others, AGPL-3.0 requires
you to offer them its source: publish it and pass `--source-url` to
`jisho serve` (GET /v1/source shows it with the commit).
