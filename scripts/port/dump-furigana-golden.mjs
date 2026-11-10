// SPDX-License-Identifier: AGPL-3.0-or-later
// Differential-test oracle for crates/jisho-ja/src/furigana.rs: runs
// Yomitan's distributeFurigana and distributeFuriganaInflected over
// generated (term, reading, source) triples and writes the results.
//
// Triples: terms built from kanji and kana runs with plausible readings
// (kana runs read as written, sometimes in the other script), sources that
// keep a stem and change the ending, mismatching readings, and random
// strings; all seeded, so the file is reproducible.
//
// Usage: node scripts/port/dump-furigana-golden.mjs <yomitan> > tests/fixtures/yomitan/furigana-golden.json

import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const [yomitan] = process.argv.slice(2);
const jp = await import(pathToFileURL(join(yomitan, 'ext/js/language/ja/japanese.js')).href);

let seed = 0x1b873593;
const next = () => { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return (seed >>> 0) / 4294967296; };
const pick = (list) => list[Math.floor(next() * list.length)];
const range = (a, b) => Array.from({length: b - a + 1}, (_, i) => String.fromCodePoint(a + i));
const hiragana = range(0x3041, 0x3093);
const kanji = [...'\u98df\u884c\u6765\u898b\u672c\u65e5\u4eba\u5927\u5c0f\u5f15\u6253\u8fbc\u8aad\u66f8\u805e\u8a71\u6642\u9593\u4e00\u751f\u61f8\u547d\u3005', '\u{20b9f}'];
const katakana = (s) => [...s].map((c) => String.fromCodePoint(c.codePointAt(0) + 0x60)).join('');
const kanaRun = () => Array.from({length: 1 + Math.floor(next() * 3)}, () => pick(hiragana)).join('');

const triples = [];
for (let n = 0; n < 3000; ++n) {
    let term = '';
    let reading = '';
    const groups = 1 + Math.floor(next() * 4);
    let kanjiNext = next() < 0.7;
    for (let g = 0; g < groups; ++g) {
        if (kanjiNext) {
            const length = 1 + Math.floor(next() * 2);
            for (let i = 0; i < length; ++i) { term += pick(kanji); }
            reading += kanaRun();
        } else {
            const run = kanaRun();
            const r = next();
            term += r < 0.15 ? katakana(run) : run;
            reading += r > 0.9 ? katakana(run) : run;
        }
        kanjiNext = !kanjiNext;
    }
    if (next() < 0.1) { reading = kanaRun() + kanaRun(); }
    if (next() < 0.05) { reading = term; }
    let source;
    const s = next();
    if (s < 0.4) {
        const chars = [...term];
        source = chars.slice(0, Math.max(1, chars.length - 1 - Math.floor(next() * 2))).join('') + kanaRun();
    } else if (s < 0.6) {
        const chars = [...reading];
        source = chars.slice(0, Math.max(0, chars.length - 1)).join('') + kanaRun();
    } else if (s < 0.7) {
        source = katakana(reading);
    } else if (s < 0.8) {
        source = Array.from({length: 1 + Math.floor(next() * 5)}, () => pick([...hiragana, ...kanji])).join('');
    } else {
        source = term;
    }
    triples.push([term, reading, source]);
}
// Lone surrogates have no JSON form; the generator never makes them, this
// is a guard.
const wellFormed = (t) => t.every((x) => x.isWellFormed());
const out = triples.filter(wellFormed).map(([term, reading, source]) => ({
    term, reading, source,
    furigana: jp.distributeFurigana(term, reading).map((x) => [x.text, x.reading]),
    inflected: jp.distributeFuriganaInflected(term, reading, source).map((x) => [x.text, x.reading]),
}));
process.stdout.write(JSON.stringify(out) + '\n');
console.error(`${out.length} triples`);
