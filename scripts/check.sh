#!/usr/bin/env bash
# Runs every project check: format, lint, tests (must actually run), ASCII
# rule, license headers, dependency licenses, and no tracked dictionaries.
# Exits non-zero on the first failure. Requires cargo, git, awk and python3;
# npm for the web UI check (skipped with a notice without it).
set -euo pipefail

cd "$(dirname "$0")/.."

# Paths allowed to contain non-ASCII text (data that is non-ASCII by nature).
# Inside .rs files, string and char literals may always hold non-ASCII.
# ui/dist is build output (the CSS minifier turns escapes into characters);
# its ASCII sources are checked, and the web UI step checks dist matches.
# Images are binary, not text.
ASCII_EXCEPTIONS='^(docs/reference/|tests/fixtures/|crates/jisho-(ja|lookup)/data/.*[.]json$|bench/sentences.txt$|LICENSE$|ui/dist/)|[.](png|jpe?g|gif|webp|ico)$'

# Licenses allowed for dependencies (SPDX ids). See scripts/license_check.py
# for how OR/AND/WITH expressions are evaluated.
ALLOWED_LICENSES='MIT MIT-0 Apache-2.0 BSD-2-Clause BSD-3-Clause ISC Zlib Unicode-3.0 Unicode-DFS-2016 Unlicense 0BSD CC0-1.0 MPL-2.0 GPL-3.0-or-later AGPL-3.0-or-later LLVM-exception BSL-1.0'

step() { printf '\n== %s\n' "$1"; }

project_files() {
    git ls-files --cached --others --exclude-standard
}

step "format"
cargo fmt --all --check
# The fuzz crate is outside the workspace (nightly-only build).
cargo fmt --manifest-path fuzz/Cargo.toml --check

step "web ui"
# ui/dist is committed and embedded in the binary: it must be what the
# sources build to.
if command -v npm > /dev/null; then
    if [ ! -d ui/node_modules ]; then
        npm ci --prefix ui --no-fund --no-audit
    fi
    ui_out=$(mktemp -d)
    npm run --silent --prefix ui build -- --out "$ui_out"
    if ! diff -r "$ui_out" ui/dist; then
        rm -rf "$ui_out"
        echo "check: ui/dist is stale; run npm run build in ui/ and commit it" >&2
        exit 1
    fi
    rm -rf "$ui_out"
    echo ok
else
    echo "skipped: npm not installed (ui/dist not checked against ui/src)"
fi

step "clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

step "tests"
test_log=$(mktemp)
trap 'rm -f "$test_log"' EXIT
cargo test --workspace --locked 2>&1 | tee "$test_log"
passed=$(grep -E '^test result: ok\.' "$test_log" | sed -E 's/.* ok\. ([0-9]+) passed.*/\1/' | awk '{ s += $1 } END { print s + 0 }')
if [ -z "$passed" ] || [ "$passed" -eq 0 ]; then
    echo "check: no tests ran" >&2
    exit 1
fi
echo "tests passed: $passed"

step "ascii"
ascii_files=()
while IFS= read -r f; do
    [ -f "$f" ] || continue
    if printf '%s\n' "$f" | grep -qE "$ASCII_EXCEPTIONS"; then continue; fi
    ascii_files+=("$f")
done < <(project_files)
if ! python3 -I scripts/ascii_check.py "${ascii_files[@]}"; then
    echo "check: non-ASCII characters outside string literals (AGENTS.md rule 1)" >&2
    exit 1
fi
echo ok

step "license headers"
bad=0
while IFS= read -r f; do
    case "$f" in crates/*.rs | ui/src/*.ts | ui/src/*.css) ;; *) continue ;; esac
    [ -f "$f" ] || continue
    if grep -qE '^(//|/\*) Ported from Yomitan ' "$f"; then
        if ! grep -qE '^(//| \*) SPDX-License-Identifier: GPL-3.0-or-later$' "$f"; then
            echo "$f: ported file without GPL-3.0-or-later SPDX line" >&2; bad=1
        fi
        if ! grep -q 'Copyright (C)' "$f"; then
            echo "$f: ported file without upstream copyright lines" >&2; bad=1
        fi
        if ! grep -qF "$f" NOTICE; then
            echo "$f: ported file not listed in NOTICE" >&2; bad=1
        fi
    elif ! head -n 1 "$f" | grep -qE '^(//|/\*) SPDX-License-Identifier: AGPL-3.0-or-later$'; then
        echo "$f: first line must be the AGPL-3.0-or-later SPDX line" >&2; bad=1
    fi
done < <(project_files)
if [ "$bad" -ne 0 ]; then exit 1; fi

step "dependency licenses"
cargo metadata --format-version 1 --locked | python3 -I scripts/license_check.py $ALLOWED_LICENSES
echo ok

step "no tracked dictionaries"
if [ -n "$(git ls-files '*.zip' 2>/dev/null)" ]; then
    echo "check: zip files are tracked; dictionaries must never be committed" >&2
    git ls-files '*.zip' >&2
    exit 1
fi
echo ok

printf '\nall checks passed\n'
