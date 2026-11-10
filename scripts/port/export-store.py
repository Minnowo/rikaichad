#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exports a jisho store as the rows Yomitan's IndexedDB would hold, for
scripts/port/yomitan-oracle.mjs: dictionaries (with jisho's lookup settings),
terms, term meta, kanji, kanji meta and tags, ids kept. JSON lines: first
{"settings", "dictionaries"}, then one ["table", row] per row.

Usage: python3 scripts/port/export-store.py <store.sqlite> > rows.jsonl
"""

import json
import sqlite3
import sys


def main() -> None:
    con = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
    out = sys.stdout
    meta = dict(con.execute("SELECT key, value FROM meta"))
    dictionaries = []
    titles = {}
    for row in con.execute(
        "SELECT id, title, enabled, alias, allow_secondary_searches, parts_of_speech_filter,"
        " use_deinflections, index_json FROM dictionaries ORDER BY priority, id"
    ):
        index = json.loads(row[7])
        titles[row[0]] = row[1]
        dictionaries.append(
            {
                "title": row[1],
                "enabled": bool(row[2]),
                "alias": row[3] or row[1],
                "allowSecondarySearches": bool(row[4]),
                "partsOfSpeechFilter": bool(row[5]),
                "useDeinflections": bool(row[6]),
                "frequencyMode": index.get("frequencyMode"),
            }
        )
    settings = {
        "mainDictionary": meta.get("main_dictionary", ""),
        "sortFrequencyDictionary": meta.get("sort_frequency_dictionary"),
        "sortFrequencyDictionaryOrder": meta.get("sort_frequency_order", "descending"),
    }
    out.write(json.dumps({"settings": settings, "dictionaries": dictionaries}, ensure_ascii=False) + "\n")

    def rows(name, sql, convert):
        for row in con.execute(sql):
            out.write(json.dumps([name, convert(row)], ensure_ascii=False) + "\n")

    rows(
        "terms",
        "SELECT t.id, t.dictionary, t.expression, t.reading, t.definition_tags, t.rules,"
        " t.score, t.sequence, t.term_tags, g.glossary"
        " FROM terms t JOIN term_glossaries g ON g.id = t.id ORDER BY t.id",
        lambda r: {
            "id": r[0], "dictionary": titles[r[1]], "expression": r[2], "reading": r[3],
            "definitionTags": r[4], "rules": r[5], "score": r[6],
            "sequence": r[7], "termTags": r[8], "glossary": json.loads(r[9]),
        },
    )
    rows(
        "termMeta",
        "SELECT id, dictionary, expression, mode, data FROM term_meta ORDER BY id",
        lambda r: {"id": r[0], "dictionary": titles[r[1]], "expression": r[2],
                   "mode": r[3], "data": json.loads(r[4])},
    )

    def stats(text):
        value = json.loads(text)
        return dict(value) if isinstance(value, list) else value

    rows(
        "kanji",
        "SELECT id, dictionary, character, onyomi, kunyomi, tags, meanings, stats"
        " FROM kanji ORDER BY id",
        lambda r: {"id": r[0], "dictionary": titles[r[1]], "character": r[2], "onyomi": r[3],
                   "kunyomi": r[4], "tags": r[5], "meanings": json.loads(r[6]),
                   "stats": stats(r[7])},
    )
    rows(
        "kanjiMeta",
        "SELECT id, dictionary, character, mode, data FROM kanji_meta ORDER BY id",
        lambda r: {"id": r[0], "dictionary": titles[r[1]], "character": r[2],
                   "mode": r[3], "data": json.loads(r[4])},
    )
    rows(
        "tagMeta",
        "SELECT id, dictionary, name, category, sort_order, notes, score FROM tags ORDER BY id",
        lambda r: {"id": r[0], "dictionary": titles[r[1]], "name": r[2], "category": r[3],
                   "order": r[4], "notes": r[5], "score": r[6]},
    )


if __name__ == "__main__":
    main()
