// SPDX-License-Identifier: AGPL-3.0-or-later
// Oracle for crates/rikaichad-lookup/src/collation.rs: sorts seeded random
// strings (ASCII, kana of both scripts and widths, prolonged sound marks,
// iteration marks, ideographs, full-width forms) with
// Intl.Collator('en-US'), the comparer Yomitan's translator uses for its
// final tie-breaks, and writes the sorted list.
//
// Usage: node scripts/port/dump-collator-golden.mjs > tests/fixtures/collator-golden.json

const collator = new Intl.Collator('en-US');
let seed = 0x9e3779b9;
const next = () => { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return (seed >>> 0) / 4294967296; };
const range = (first, last) => Array.from({length: last - first + 1}, (_, i) => String.fromCharCode(first + i));
const ascii = range(0x20, 0x7e);
const kana = [...range(0x3041, 0x3096), ...range(0x30a1, 0x30fa), '\u30fc', '\u309d', '\u309e', '\u30fd', '\u30fe', '\u3005', '\u3006', '\u30f6', '\u30f5'];
const halfwidth = range(0xff66, 0xff9d);
const fullwidth = range(0xff01, 0xff5e);
const ideographs = Array.from({length: 400}, () => String.fromCharCode(0x4e00 + Math.floor(next() * 0x5200)));
const near = [...'aAbB-_ 10', '\u304b', '\u304c', '\u30ab', '\u30ac', '\u3041', '\u3042', '\u30a2', '\uff71', '\u30fc', '\u884c', '\u5f80', '\u5f15'];
const sets = [ascii, kana, halfwidth, fullwidth, ideographs, near];
const strings = new Set();
while (strings.size < 6000) {
    const length = 1 + Math.floor(next() * 5);
    let s = '';
    for (let j = 0; j < length; ++j) {
        const set = sets[Math.floor(next() * sets.length)];
        s += set[Math.floor(next() * set.length)];
    }
    strings.add(s);
}
// Strings the collator calls equal have no defined order; keep one of each.
const sorted = [...strings].sort((a, b) => collator.compare(a, b))
    .filter((s, i, list) => i === 0 || collator.compare(list[i - 1], s) !== 0);
const {icu, cldr} = process.versions;
process.stdout.write(JSON.stringify({collator: 'en-US', icu, cldr, sorted}) + '\n');
