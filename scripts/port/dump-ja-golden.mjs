// SPDX-License-Identifier: AGPL-3.0-or-later
// Differential-test oracle: runs Yomitan's Japanese text functions (and
// kanji-processor's convertVariants) over a large input set and writes
// [input, output] pairs, which crates/rikaichad-ja/tests/yomitan_text_functions.rs
// must reproduce exactly.
//
// Inputs: string literals from test/japanese-util.test.js, every character
// in the relevant Unicode blocks alone and followed by marks, the bench
// sentences, and seeded random strings.
//
// Usage: node scripts/port/dump-ja-golden.mjs <yomitan> <kanji-processor dir> <bench sentences> > out.json

import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const [yomitan, kanjiProcessor, sentencesPath] = process.argv.slice(2);
const load = (p) => import(pathToFileURL(join(yomitan, p)).href);
const jp = await load('ext/js/language/ja/japanese.js');
const jpw = await load('ext/js/language/ja/japanese-wanakana.js');
const cjk = await load('ext/js/language/CJK-util.js');

// kanji-processor dist/index.js, reproduced verbatim (it imports JSON,
// which plain node cannot).
const mappingList = JSON.parse(readFileSync(join(kanjiProcessor, 'dist/full_list.json'), 'utf8'));
const itaijiList = JSON.parse(readFileSync(join(kanjiProcessor, 'dist/itaiji_list.json'), 'utf8'));
const variantRegex = new RegExp(`[${itaijiList.join('')}]`, 'g');
const conversionMap = {};
for (const mapping of mappingList) {
    for (const itaiji of mapping.itaiji) { conversionMap[itaiji] = mapping.oyaji; }
}
const convertVariants = (text) => text.replace(variantRegex, (match) => conversionMap[match] || match);

const inputs = new Set();
const testSource = readFileSync(join(yomitan, 'test/japanese-util.test.js'), 'utf8');
for (const match of testSource.matchAll(/'((?:[^'\\\n]|\\.)*)'/g)) {
    try { inputs.add(new Function(`return '${match[1]}';`)()); } catch { /* not a plain literal */ }
}
for (const line of readFileSync(sentencesPath, 'utf8').split('\n')) { if (line) { inputs.add(line); } }
const blocks = [[0x3000, 0x30ff], [0xff00, 0xffef], [0x2e80, 0x2fdf], [0x31c0, 0x31ef], [0x3300, 0x33ff], [0x20, 0x7e]];
const marks = ['', '\u3099', '\u309a', '\uff9e', '\uff9f', '\u30fc', '\u30fc\u30fc', '\u3063'];
for (const [min, max] of blocks) {
    for (let cp = min; cp <= max; ++cp) {
        for (const mark of marks) { inputs.add(String.fromCodePoint(cp) + mark); }
    }
}
for (const [variant] of Object.entries(conversionMap)) { inputs.add(`a${variant}b`); }
// Seeded xorshift so the oracle file is reproducible.
let seed = 0x2545f491;
const next = () => { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return (seed >>> 0) / 4294967296; };
const alphabet = [...'\u304b\u304d\u304f\u3051\u3053\u304c\u304e\u3071\u3074\u30ab\u30ad\u30af\u30b1\u30b3\u30ac\u30d1\u30d4\u30c3\u3063\u30fc\u30a1\u30e3\u30e5\u30e7\u3083\u3085\u3087\u3093\u30f3\uff76\uff77\uff78\uff8a\uff8b\uff9e\uff9fabkstnhmyrwgzdpAKSTN-0123456789\uff21\uff2b\uff53\uff11\u842c\u8207\u9f9c\u6f22\u5b57\u3000\u3001\u3002\u30fb\u3099\u309a\u{20b9f}'];
for (let i = 0; i < 4000; ++i) {
    const length = 1 + Math.floor(next() * 12);
    let s = '';
    for (let j = 0; j < length; ++j) { s += alphabet[Math.floor(next() * alphabet.length)]; }
    inputs.add(s);
}

const list = [...inputs];
const functions = {
    convertKatakanaToHiragana: (s) => jp.convertKatakanaToHiragana(s),
    convertKatakanaToHiraganaKeepProlonged: (s) => jp.convertKatakanaToHiragana(s, true),
    convertHiraganaToKatakana: (s) => jp.convertHiraganaToKatakana(s),
    convertAlphanumericToFullWidth: (s) => jp.convertAlphanumericToFullWidth(s),
    convertFullWidthAlphanumericToNormal: (s) => jp.convertFullWidthAlphanumericToNormal(s),
    convertHalfWidthKanaToFullWidth: (s) => jp.convertHalfWidthKanaToFullWidth(s),
    normalizeCombiningCharacters: (s) => jp.normalizeCombiningCharacters(s),
    normalizeCJKCompatibilityCharacters: (s) => jp.normalizeCJKCompatibilityCharacters(s),
    normalizeRadicals: (s) => cjk.normalizeRadicals(s),
    collapseEmphaticSequences: (s) => jp.collapseEmphaticSequences(s, false),
    collapseEmphaticSequencesFull: (s) => jp.collapseEmphaticSequences(s, true),
    convertAlphabeticToKana: (s) => jpw.convertAlphabeticToKana(s),
    convertVariants: (s) => convertVariants(s),
    isStringPartiallyJapanese: (s) => jp.isStringPartiallyJapanese(s),
    isStringEntirelyKana: (s) => jp.isStringEntirelyKana(s),
};
const out = {inputs: list, outputs: {}};
for (const [name, fn] of Object.entries(functions)) { out.outputs[name] = list.map(fn); }
process.stdout.write(JSON.stringify(out) + '\n');
console.error(`${list.length} inputs x ${Object.keys(functions).length} functions`);
