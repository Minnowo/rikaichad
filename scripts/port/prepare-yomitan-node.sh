#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Makes a copy of Yomitan's ext/ that plain node can import, for
# scripts/port/yomitan-oracle.mjs: ext/lib (built by Yomitan's bundler) gets
# kanji-processor with its JSON imports marked as JSON, and a stub for
# hangul-js (Korean only; never called for Japanese).
#
# Usage: scripts/port/prepare-yomitan-node.sh <out dir>
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
out=$1
yomitan="$root/reference-src/yomitan"
kp="$root/reference-src/kanji-processor-1.0.2"
rm -rf "$out"
mkdir -p "$out/ext/lib/kp"
cp -R "$yomitan/ext/js" "$out/ext/js"
cp -R "$yomitan/ext/data" "$out/ext/data"
printf '{"type": "module"}\n' > "$out/package.json"
cp "$kp/dist/full_list.json" "$kp/dist/itaiji_list.json" "$out/ext/lib/kp/"
sed "s#from '\(\./[a-z_]*\.json\)';#from '\1' with {type: 'json'};#" "$kp/dist/index.js" > "$out/ext/lib/kp/index.js"
printf "export * from './kp/index.js';\n" > "$out/ext/lib/kanji-processor.js"
printf "export const Hangul = {disassemble: (s) => [...s], assemble: (a) => a.join('')};\n" > "$out/ext/lib/hangul-js.js"
echo "$out"
