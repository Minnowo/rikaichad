#!/usr/bin/env bash
# Downloads the public test corpus (brief section 5) into corpus/ and
# verifies each file's SHA-256. The files are CC BY-SA 4.0 data from the
# EDRDG (JMdict, KANJIDIC), built into Yomitan format by
# https://github.com/yomidevs/jmdict-yomitan. They are never committed.
# Requires curl and python3.
set -euo pipefail

cd "$(dirname "$0")/.."

RELEASE="https://github.com/yomidevs/jmdict-yomitan/releases/download/2026-11-09"

# name sha256
FILES="
JMdict_english.zip 35d9e657b32d928bea9573fa576b44f6ff6caee1c3df2a88012f527b921bda18
KANJIDIC_english.zip 1369dc093de602efc2bf9f39d6b93939f9382f24e98bbad57d16a615261066c2
"

mkdir -p corpus
echo "$FILES" | while read -r name sum; do
    [ -n "$name" ] || continue
    target="corpus/$name"
    if [ ! -f "$target" ]; then
        echo "downloading $name"
        curl -fsSL -o "$target.part" "$RELEASE/$name"
        mv "$target.part" "$target"
    fi
    actual=$(python3 -I -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$target")
    if [ "$actual" != "$sum" ]; then
        echo "checksum mismatch for $name: got $actual, want $sum" >&2
        echo "delete $target to download it again" >&2
        exit 1
    fi
    echo "ok $name"
done
