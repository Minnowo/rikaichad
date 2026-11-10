# SPDX-License-Identifier: AGPL-3.0-or-later
"""Dependency license check.

Usage: cargo metadata --format-version 1 | python3 -I scripts/license_check.py ALLOWED...

Evaluates each third-party package's SPDX expression: an OR is satisfied
by any allowed alternative, an AND needs all parts allowed, WITH exceptions
are accepted when the base license is allowed. Packages without a license
field fail. Exits 1 if any package fails.
"""

import json
import re
import sys

TOKEN = re.compile(r"\(|\)|[A-Za-z0-9.+\-]+")


def allowed_expression(expr, allowed):
    tokens = TOKEN.findall(expr.replace("/", " OR "))
    pos = 0

    def peek():
        return tokens[pos] if pos < len(tokens) else None

    def take():
        nonlocal pos
        pos += 1
        return tokens[pos - 1]

    def parse_or():
        result = parse_and()
        while peek() == "OR":
            take()
            right = parse_and()
            result = result or right
        return result

    def parse_and():
        result = parse_atom()
        while peek() == "AND":
            take()
            right = parse_atom()
            result = result and right
        return result

    def parse_atom():
        if peek() == "(":
            take()
            result = parse_or()
            if peek() != ")":
                raise ValueError("unbalanced parentheses")
            take()
        else:
            token = take()
            if token is None:
                raise ValueError("unexpected end")
            result = token in allowed
        if peek() == "WITH":
            take()
            take()
        return result

    value = parse_or()
    if pos != len(tokens):
        raise ValueError("trailing tokens")
    return value


def main(allowed):
    bad = []
    for pkg in json.load(sys.stdin)["packages"]:
        if pkg.get("source") is None:
            continue
        lic = pkg.get("license") or ""
        name = f'{pkg["name"]} {pkg["version"]}'
        try:
            ok = bool(lic) and allowed_expression(lic, allowed)
        except (ValueError, IndexError) as e:
            bad.append(f"{name}: unparseable license {lic!r} ({e})")
            continue
        if not ok:
            bad.append(f"{name}: {lic or '(none)'}")
    for line in bad:
        print("disallowed or missing license:", line, file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(set(sys.argv[1:])))
