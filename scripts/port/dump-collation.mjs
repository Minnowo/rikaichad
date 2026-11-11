// SPDX-License-Identifier: AGPL-3.0-or-later
// Collation table for crates/rikaichad-lookup/src/collation.rs: the order of
// Intl.Collator('en-US') (ICU's CLDR root collation), which Yomitan's
// translator uses for its final tie-breaks, measured per character.
//
// Every character of the ranges below that is not ignorable is sorted with
// the collator; `levels` has one digit per sorted character saying at which
// strength it differs from the previous one: 3 primary (a different
// letter), 2 secondary (accents, dakuten), 1 tertiary (case, kana type,
// width), 0 not at all. Comparing strings by these per-character weights
// (all primaries, then secondaries, then tertiaries) matches the collator
// for characters that have one primary weight. Characters the collator
// expands to several (e.g. U+3006 sorts like "shime", U+33E0 like "1 day")
// are left out, so strings with them fall back to code point order: one is
// detected when its predecessor followed by the largest character sorts
// after it (its first weight equals the predecessor's).
//
// Usage: node scripts/port/dump-collation.mjs > crates/rikaichad-lookup/data/collation.json

const full = new Intl.Collator('en-US');
const base = new Intl.Collator('en-US', {sensitivity: 'base'});
const accent = new Intl.Collator('en-US', {sensitivity: 'accent'});
// The whole Basic Multilingual Plane except surrogates and private use.
const ranges = [[0x20, 0xd7ff], [0xf900, 0xfffd]];
const chars = [];
// Ignorable at every level (variation selectors, zero-width marks): skipped.
let ignorable = '';
for (const [first, last] of ranges) {
    for (let c = first; c <= last; ++c) {
        const s = String.fromCharCode(c);
        if (full.compare(s, '') === 0) {
            ignorable += s;
        } else if (base.compare(s, '') !== 0) {
            chars.push(s);
        }
    }
}
chars.sort(full.compare);
const largest = chars[chars.length - 1];
const kept = [];
let levels = '';
let expansion = false;
for (let i = 0; i < chars.length; ++i) {
    const [a, b] = [chars[i - 1], chars[i]];
    const level = i === 0 || base.compare(a, b) !== 0 ? 3 : accent.compare(a, b) !== 0 ? 2 : full.compare(a, b) !== 0 ? 1 : 0;
    if (level === 3) {
        // Same primary as a dropped expansion: an expansion too.
        expansion = i > 0 && full.compare(a + largest, b) > 0;
    }
    if (expansion) { continue; }
    // A level is relative to the previous kept character.
    const previous = kept[kept.length - 1];
    const keptLevel = previous === undefined || base.compare(previous, b) !== 0 ? 3 : accent.compare(previous, b) !== 0 ? 2 : full.compare(previous, b) !== 0 ? 1 : 0;
    kept.push(b);
    levels += String(keptLevel);
}
const {icu, cldr, unicode} = process.versions;
process.stdout.write(JSON.stringify({collator: 'en-US', icu, cldr, unicode, dropped: chars.length - kept.length, ignorable, chars: kept.join(''), levels}) + '\n');
