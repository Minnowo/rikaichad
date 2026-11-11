#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Compares the compact results of scripts/port/yomitan-oracle.mjs (Yomitan)
and crates/rikaichad-lookup/tests/corpus_parity.rs (rikaichad) line by line.

Prints a summary per mode and the first differences; exit status 1 if any
query differs. Numbers compare by value (1 == 1.0).

Usage: python3 scripts/port/compare-parity.py yomitan.jsonl rikaichad.jsonl [max shown]
"""

import json
import sys
from collections import Counter


def first_difference(a, b, path="$"):
    """Path and values of the first place where a and b differ, or None."""
    if isinstance(a, (int, float)) and isinstance(b, (int, float)) and not isinstance(a, bool) and not isinstance(b, bool):
        return None if a == b else (path, a, b)
    if type(a) is not type(b):
        return (path, a, b)
    if isinstance(a, dict):
        for key in sorted(set(a) | set(b)):
            if key not in a or key not in b:
                return (f"{path}.{key}", a.get(key, "<missing>"), b.get(key, "<missing>"))
            found = first_difference(a[key], b[key], f"{path}.{key}")
            if found:
                return found
        return None
    if isinstance(a, list):
        for i, (x, y) in enumerate(zip(a, b)):
            found = first_difference(x, y, f"{path}[{i}]")
            if found:
                return found
        if len(a) != len(b):
            return (f"{path}.length", len(a), len(b))
        return None
    return None if a == b else (path, a, b)


def main() -> int:
    yomitan_path, rikaichad_path = sys.argv[1], sys.argv[2]
    shown_max = int(sys.argv[3]) if len(sys.argv) > 3 else 10
    totals, failures = Counter(), Counter()
    shown = 0
    with open(yomitan_path, encoding="utf-8") as fy, open(rikaichad_path, encoding="utf-8") as fj:
        for line_y, line_j in zip(fy, fj):
            y, j = json.loads(line_y), json.loads(line_j)
            if (y["mode"], y["text"]) != (j["mode"], j["text"]):
                print("files are out of step:", y["mode"], y["text"], j["mode"], j["text"])
                return 1
            totals[y["mode"]] += 1
            found = first_difference(y, j)
            if found:
                failures[y["mode"]] += 1
                if shown < shown_max:
                    shown += 1
                    path, a, b = found
                    print(f"[{y['mode']}] {y['text']}\n  at {path}\n  yomitan {json.dumps(a, ensure_ascii=False)[:400]}\n  rikaichad   {json.dumps(b, ensure_ascii=False)[:400]}")
    for mode in totals:
        print(f"{mode}: {totals[mode] - failures[mode]}/{totals[mode]} identical")
    return 1 if sum(failures.values()) else 0


if __name__ == "__main__":
    sys.exit(main())
