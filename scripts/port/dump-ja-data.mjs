// SPDX-License-Identifier: AGPL-3.0-or-later
// Dumps the data tables rikaichad-ja ports from Yomitan and kanji-processor:
//   romaji-to-hiragana.json  ROMAJI_TO_HIRAGANA entries in Object.entries order
//   nfkd.json                String.normalize('NFKD') of every code point in
//                            the CJK radical and CJK compatibility ranges that
//                            Yomitan normalizes, where the result differs
//   kanji-variants.json      [variant, standard] pairs that kanji-processor's
//                            convertVariants actually replaces
//   halfwidth-katakana.json  [halfwidth, plain, with dakuten, with handakuten]
//                            from convertHalfWidthKanaToFullWidth (null where
//                            the combination stays two characters)
//   prolonged-sound.json     [kana, hiragana vowel] from
//                            convertKatakanaToHiragana(kana + U+30FC)
//
// Usage: node scripts/port/dump-ja-data.mjs <yomitan checkout> <kanji-processor package dir> <out dir>

import {readFileSync, writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const [yomitan, kanjiProcessor, outDir] = process.argv.slice(2);
if (!yomitan || !kanjiProcessor || !outDir) {
    console.error('usage: dump-ja-data.mjs <yomitan checkout> <kanji-processor dir> <out dir>');
    process.exit(2);
}
const write = (name, value) => writeFileSync(join(outDir, name), JSON.stringify(value, null, 1) + '\n');

const dicts = await import(pathToFileURL(join(yomitan, 'ext/js/language/ja/japanese-kana-romaji-dicts.js')).href);
write('romaji-to-hiragana.json', Object.entries(dicts.ROMAJI_TO_HIRAGANA));

const cjk = await import(pathToFileURL(join(yomitan, 'ext/js/language/CJK-util.js')).href);
const ranges = [...cjk.CJK_RADICALS_RANGES, cjk.CJK_COMPATIBILITY];
const nfkd = [];
for (const [min, max] of ranges) {
    if (max > 0xffff) { throw new Error('NFKD range outside the BMP'); }
    for (let cp = min; cp <= max; ++cp) {
        const ch = String.fromCodePoint(cp);
        const normalized = ch.normalize('NFKD');
        if (normalized !== ch) { nfkd.push([cp, normalized]); }
    }
}
write('nfkd.json', {unicode: process.versions.unicode, ranges, map: nfkd});

// kanji-processor builds new RegExp(`[${itaijiList.join('')}]`, 'g') without
// the u flag, so the class holds UTF-16 code units: only single-unit (BMP)
// variants can ever match. conversionMap is filled in mapping order, later
// entries overwriting earlier ones.
const mappings = JSON.parse(readFileSync(join(kanjiProcessor, 'dist/full_list.json'), 'utf8'));
const itaiji = JSON.parse(readFileSync(join(kanjiProcessor, 'dist/itaiji_list.json'), 'utf8'));
const classString = itaiji.join('');
if (/[\]\\^-]/.test(classString)) { throw new Error('character class metacharacter in itaiji list'); }
const classUnits = new Set(classString.split(''));
const conversionMap = new Map();
for (const mapping of mappings) {
    for (const variant of mapping.itaiji) { conversionMap.set(variant, mapping.oyaji); }
}
const pairs = [];
for (const [variant, standard] of conversionMap) {
    if (variant.length === 1 && classUnits.has(variant)) { pairs.push([variant, standard]); }
}
write('kanji-variants.json', pairs);
const japanese = await import(pathToFileURL(join(yomitan, 'ext/js/language/ja/japanese.js')).href);
const halfwidth = [];
for (let cp = 0xff61; cp <= 0xff9f; ++cp) {
    const ch = String.fromCodePoint(cp);
    const plain = japanese.convertHalfWidthKanaToFullWidth(ch);
    if (plain === ch) { continue; }
    const withMark = (mark) => {
        const out = japanese.convertHalfWidthKanaToFullWidth(ch + mark);
        return out.length === 1 ? out : null;
    };
    halfwidth.push([ch, plain, withMark('\uff9e'), withMark('\uff9f')]);
}
write('halfwidth-katakana.json', halfwidth);

const prolonged = [];
for (let cp = 0x3040; cp <= 0x30ff; ++cp) {
    const ch = String.fromCodePoint(cp);
    const out = japanese.convertKatakanaToHiragana(ch + '\u30fc');
    const last = out.slice(-1);
    if (last !== '\u30fc') { prolonged.push([ch, last]); }
}
write('prolonged-sound.json', prolonged);

console.error(`romaji ${Object.keys(dicts.ROMAJI_TO_HIRAGANA).length}, nfkd ${nfkd.length} (unicode ${process.versions.unicode}), kanji variants ${pairs.length} of ${conversionMap.size}, halfwidth ${halfwidth.length}, prolonged ${prolonged.length}`);
