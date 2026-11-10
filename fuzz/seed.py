#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Writes seed corpora for the fuzz targets into fuzz/corpus/<target>/
from the committed fixtures (Yomitan's valid-dictionary1, the crafted
parity dictionary) and the bench sentences.

Usage: python3 -I fuzz/seed.py
"""

import hashlib
import io
import json
import os
import zipfile

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
CORPUS = os.path.join(ROOT, "fuzz", "corpus")
DICTIONARIES = [
    os.path.join(ROOT, "tests", "fixtures", "yomitan", "valid-dictionary1"),
    os.path.join(ROOT, "tests", "fixtures", "parity", "merge-deinflection"),
]
BANK_KINDS = {"term_bank": 0, "term_meta_bank": 1, "kanji_bank": 2, "kanji_meta_bank": 3, "tag_bank": 4}


def write(target: str, data: bytes) -> None:
    directory = os.path.join(CORPUS, target)
    os.makedirs(directory, exist_ok=True)
    name = hashlib.sha1(data).hexdigest()
    with open(os.path.join(directory, name), "wb") as f:
        f.write(data)


def main() -> None:
    for directory in DICTIONARIES:
        files = sorted(os.listdir(directory))
        for method in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, "w", method) as z:
                for name in files:
                    if name.endswith(".json") or name.endswith(".css"):
                        z.write(os.path.join(directory, name), name)
            write("archive", buffer.getvalue())
            write("import", buffer.getvalue())
        for name in files:
            path = os.path.join(directory, name)
            with open(path, "rb") as f:
                data = f.read()
            if name == "index.json":
                write("rows", bytes([5]) + data)
            for prefix, kind in BANK_KINDS.items():
                if name.startswith(prefix + "_"):
                    write("rows", bytes([kind]) + data)
                    if prefix == "term_bank":
                        for row in json.loads(data):
                            for item in row[5]:
                                write("render", json.dumps(item, ensure_ascii=False).encode())
    with open(os.path.join(ROOT, "bench", "sentences.txt"), encoding="utf-8") as f:
        for line in f:
            if line.strip():
                for op in range(8):
                    write("lookup", bytes([op]) + line.strip().encode())
    for text in ["\u98df\u3079\u306a\u304b\u3063\u305f", "\u3054\u98ef", "taberu",
                 "\u6253\u3061\u8fbc\u3080|\u3046\u3061\u3053\u3080|\u6253\u3061\u8fbc\u3093\u3060"]:
        for op in range(8):
            write("lookup", bytes([op]) + text.encode())


if __name__ == "__main__":
    main()
