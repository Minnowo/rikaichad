#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Builds tests/fixtures/parity/public-corpus-golden.json from the
oracle's output for the public corpus (scripts/port/yomitan-oracle.mjs with
the intl collator). The golden keeps, per query and mode, only what the
brief allows in committed goldens: the order of the first 32 entries
(Yomitan's default maxResults) and, for each, its headwords (term,
reading) and inflection chains; no glossaries. Modes group (Yomitan's
default) and merge.

Usage: python3 scripts/port/make-corpus-golden.py yomitan.jsonl texts.txt > golden.json
"""

import json
import sys


MAX_RESULTS = 32


def main() -> None:
    modes = ["group", "merge"]
    # Every second query keeps the file small; neighbours overlap anyway.
    texts = [line for line in open(sys.argv[2], encoding="utf-8").read().split("\n") if line][::2]
    wanted = set(texts)
    results = {mode: {} for mode in modes}
    for line in open(sys.argv[1], encoding="utf-8"):
        row = json.loads(line)
        if row["mode"] not in modes:
            continue
        if row["text"] not in wanted:
            continue
        results[row["mode"]][row["text"]] = [
            [[h[0], h[1]] for h in entry["h"]] + [entry["i"]] for entry in row["entries"][:MAX_RESULTS]
        ]
    golden = {
        "yomitan": "ee81f8dcf13e985bb4d7961009fb49a764b4d924",
        "collator": "Intl.Collator('en-US'), node v22.23.4 (ICU 78.3)",
        "corpus": {
            "JMdict_english.zip": "35d9e657b32d928bea9573fa576b44f6ff6caee1c3df2a88012f527b921bda18",
            "KANJIDIC_english.zip": "1369dc093de602efc2bf9f39d6b93939f9382f24e98bbad57d16a615261066c2",
        },
        "max_results": MAX_RESULTS,
        "settings": "Yomitan defaults; dictionaries in this order: JMdict, KANJIDIC; main dictionary JMdict",
        "entry": "headwords as [term, reading]..., then the inflection chains as [source, [rule names]]",
        "texts": texts,
        "results": {mode: [results[mode][t] for t in texts] for mode in modes},
    }
    json.dump(golden, sys.stdout, ensure_ascii=False, separators=(",", ":"))
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
