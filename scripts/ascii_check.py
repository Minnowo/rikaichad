# SPDX-License-Identifier: AGPL-3.0-or-later
"""ASCII rule check (AGENTS.md rule 1).

Usage: python3 -I scripts/ascii_check.py FILE...

Any non-ASCII character fails, except inside Rust string and char literals
in .rs files (Japanese test data and tables must live there). Prints
file:line:col for each violation and exits 1 if any were found.
"""

import sys


def rust_violations(text):
    """Yields (line, col) of non-ASCII characters outside literals."""
    i = 0
    n = len(text)
    line = 1
    col = 1

    def advance(count):
        nonlocal i, line, col
        for _ in range(count):
            if i < n and text[i] == "\n":
                line += 1
                col = 1
            else:
                col += 1
            i += 1

    while i < n:
        c = text[i]
        if c == "/" and text.startswith("//", i):
            while i < n and text[i] != "\n":
                if ord(text[i]) > 127:
                    yield (line, col)
                advance(1)
            continue
        if c == "/" and text.startswith("/*", i):
            depth = 0
            while i < n:
                if text.startswith("/*", i):
                    depth += 1
                    advance(2)
                elif text.startswith("*/", i):
                    depth -= 1
                    advance(2)
                    if depth == 0:
                        break
                else:
                    if ord(text[i]) > 127:
                        yield (line, col)
                    advance(1)
            continue
        raw = raw_string_hashes(text, i)
        if raw is not None:
            prefix_len, hashes = raw
            advance(prefix_len)
            closing = '"' + "#" * hashes
            while i < n and not text.startswith(closing, i):
                advance(1)
            advance(len(closing))
            continue
        if c == '"' or (c == "b" and text.startswith('b"', i)) or (c == "c" and text.startswith('c"', i)):
            advance(1 if c == '"' else 2)
            while i < n and text[i] != '"':
                advance(2 if text[i] == "\\" else 1)
            advance(1)
            continue
        if c == "'":
            end = char_literal_end(text, i)
            if end is not None:
                advance(end - i)
                continue
        if ord(c) > 127:
            yield (line, col)
        advance(1)


def raw_string_hashes(text, i):
    """If a raw string literal starts at i, returns (prefix length, hashes)."""
    j = i
    if text.startswith("br", j) or text.startswith("cr", j):
        j += 2
    elif text.startswith("r", j):
        j += 1
    else:
        return None
    if i > 0 and (text[i - 1].isalnum() or text[i - 1] == "_"):
        return None
    hashes = 0
    while j < len(text) and text[j] == "#":
        hashes += 1
        j += 1
    if j < len(text) and text[j] == '"':
        return (j - i + 1, hashes)
    return None


def char_literal_end(text, i):
    """Returns the index after a char literal starting at i, or None for a
    lifetime or label."""
    j = i + 1
    if j < len(text) and text[j] == "\\":
        k = text.find("'", j + 2)
        return k + 1 if 0 < k < j + 12 else None
    if j + 1 < len(text) and text[j + 1] == "'":
        return j + 2
    return None


def main(paths):
    bad = 0
    for path in paths:
        try:
            with open(path, encoding="utf-8") as fh:
                text = fh.read()
        except UnicodeDecodeError:
            print(f"{path}: not valid UTF-8")
            bad += 1
            continue
        if path.endswith(".rs"):
            hits = list(rust_violations(text))
        else:
            hits = []
            for lineno, line in enumerate(text.split("\n"), 1):
                for colno, ch in enumerate(line, 1):
                    if ord(ch) > 127:
                        hits.append((lineno, colno))
        for lineno, colno in hits:
            print(f"{path}:{lineno}:{colno}: non-ASCII character")
        bad += len(hits)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
