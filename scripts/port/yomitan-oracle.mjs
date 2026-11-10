// SPDX-License-Identifier: AGPL-3.0-or-later
// Differential oracle: runs Yomitan's own Translator (findTerms in each
// result mode, findKanji) over a jisho store's rows and prints the results
// in the compact form that crates/jisho-lookup/tests/corpus_parity.rs
// prints for jisho, one JSON line per (mode, text).
//
// The database is a fake with the result order of Yomitan's IndexedDB
// queries (dictionary-database.js _findMultiBulk/_findFirstBulk): requests
// in order, for each the expression index before the reading index, rows by
// id; findTermsBulk returns each row once, to the first request that finds
// it. Options are those Yomitan's backend builds from default settings plus
// the store's own (dictionary order, enable flags, per-dictionary settings,
// main and sort-frequency dictionary).
//
// Usage:
//   scripts/port/prepare-yomitan-node.sh <ynode>
//   JISHO_EXPORT_STORE=<store> JISHO_EXPORT_OUT=rows.jsonl \
//       cargo test --release -p jisho-store --test export_oracle -- --ignored
//   node --max-old-space-size=8192 scripts/port/yomitan-oracle.mjs <ynode> rows.jsonl texts.txt \
//       <modes, e.g. split,group,merge,kanji> > yomitan.jsonl

import {createReadStream, readFileSync} from 'node:fs';
import {join} from 'node:path';
import {createInterface} from 'node:readline';
import {pathToFileURL} from 'node:url';

const [ynode, rowsPath, textsPath, modesArg] = process.argv.slice(2);
const {Translator} = await import(pathToFileURL(join(ynode, 'ext/js/language/translator.js')).href);
const rows = {terms: [], termMeta: [], kanji: [], kanjiMeta: [], tagMeta: []};
for await (const line of createInterface({input: createReadStream(rowsPath, 'utf8'), crlfDelay: Infinity})) {
    if (line.length === 0) { continue; }
    const value = JSON.parse(line);
    if (Array.isArray(value)) {
        rows[value[0]].push(value[1]);
    } else {
        Object.assign(rows, value);
    }
}
const texts = readFileSync(textsPath, 'utf8').split('\n').filter((line) => line.length > 0);
const modes = modesArg.split(',');

/** Rows of `list` grouped by `key`, each group in id order. */
function index(list, key) {
    const map = new Map();
    for (const row of [...list].sort((a, b) => a.id - b.id)) {
        const k = row[key];
        if (!map.has(k)) { map.set(k, []); }
        map.get(k).push(row);
    }
    return map;
}
const termsByExpression = index(rows.terms, 'expression');
const termsByReading = index(rows.terms, 'reading');
const termsBySequence = index(rows.terms, 'sequence');
const termMetaByExpression = index(rows.termMeta, 'expression');
const kanjiByCharacter = index(rows.kanji, 'character');
const kanjiMetaByCharacter = index(rows.kanjiMeta, 'character');
const tagsByName = index(rows.tagMeta, 'name');

const split = (s) => (typeof s === 'string' && s.length > 0 ? s.split(' ') : []);
const term = (row, matchSource, itemIndex) => ({
    index: itemIndex, matchType: 'exact', matchSource, term: row.expression, reading: row.reading,
    definitionTags: split(row.definitionTags), termTags: split(row.termTags), rules: split(row.rules),
    definitions: structuredClone(row.glossary), score: row.score, dictionary: row.dictionary, id: row.id,
    sequence: typeof row.sequence === 'number' ? row.sequence : -1,
});

const database = {
    async findTermsBulk(termList, dictionaries) {
        const visited = new Set();
        const out = [];
        termList.forEach((item, i) => {
            for (const [source, map] of [['term', termsByExpression], ['reading', termsByReading]]) {
                for (const row of map.get(item) ?? []) {
                    if (!dictionaries.has(row.dictionary) || visited.has(row.id)) { continue; }
                    visited.add(row.id);
                    out.push(term(row, source, i));
                }
            }
        });
        return out;
    },
    async findTermsExactBulk(termList, dictionaries) {
        const out = [];
        termList.forEach((item, i) => {
            for (const row of termsByExpression.get(item.term) ?? []) {
                if (row.reading === item.reading && dictionaries.has(row.dictionary)) { out.push(term(row, 'term', i)); }
            }
        });
        return out;
    },
    async findTermsBySequenceBulk(items) {
        const out = [];
        items.forEach((item, i) => {
            for (const row of termsBySequence.get(item.query) ?? []) {
                if (row.dictionary === item.dictionary) { out.push(term(row, 'sequence', i)); }
            }
        });
        return out;
    },
    async findTermMetaBulk(termList, dictionaries) {
        const out = [];
        termList.forEach((item, i) => {
            for (const row of termMetaByExpression.get(item) ?? []) {
                if (dictionaries.has(row.dictionary)) {
                    out.push({index: i, term: row.expression, mode: row.mode, data: structuredClone(row.data), dictionary: row.dictionary});
                }
            }
        });
        return out;
    },
    async findKanjiBulk(kanjiList, dictionaries) {
        const out = [];
        kanjiList.forEach((item, i) => {
            for (const row of kanjiByCharacter.get(item) ?? []) {
                if (!dictionaries.has(row.dictionary)) { continue; }
                out.push({
                    index: i, character: row.character, onyomi: split(row.onyomi), kunyomi: split(row.kunyomi),
                    tags: split(row.tags), definitions: [...row.meanings], stats: {...row.stats}, dictionary: row.dictionary,
                });
            }
        });
        return out;
    },
    async findKanjiMetaBulk(kanjiList, dictionaries) {
        const out = [];
        kanjiList.forEach((item, i) => {
            for (const row of kanjiMetaByCharacter.get(item) ?? []) {
                if (dictionaries.has(row.dictionary)) {
                    out.push({index: i, character: row.character, mode: row.mode, data: structuredClone(row.data), dictionary: row.dictionary});
                }
            }
        });
        return out;
    },
    async findTagMetaBulk(items) {
        return items.map(({query, dictionary}) => {
            const row = (tagsByName.get(query) ?? []).find((r) => r.dictionary === dictionary);
            return row === undefined ? undefined : {name: row.name, category: row.category, order: row.order, notes: row.notes, score: row.score, dictionary: row.dictionary};
        });
    },
    async getDictionaryInfo() {
        return rows.dictionaries.map((d) => ({title: d.title, frequencyMode: d.frequencyMode ?? undefined}));
    },
};

const translator = new Translator(database);
translator.prepare();
// Options as backend.js _getTranslatorFindTermsOptions builds them.
function enabledDictionaryMap() {
    const map = new Map();
    for (const d of rows.dictionaries) {
        if (!d.enabled) { continue; }
        map.set(d.title, {
            index: map.size, alias: d.alias, allowSecondarySearches: d.allowSecondarySearches,
            partsOfSpeechFilter: d.partsOfSpeechFilter, useDeinflections: d.useDeinflections,
        });
    }
    return map;
}
function termOptions(mode) {
    const {mainDictionary, sortFrequencyDictionary, sortFrequencyDictionaryOrder} = rows.settings;
    const map = enabledDictionaryMap();
    let excludeDictionaryDefinitions = null;
    if (mode === 'merge' && !map.has(mainDictionary)) {
        map.set(mainDictionary, {index: map.size, alias: mainDictionary, allowSecondarySearches: false, partsOfSpeechFilter: true, useDeinflections: true});
        excludeDictionaryDefinitions = new Set([mainDictionary]);
    }
    return {
        matchType: 'exact', deinflect: true, primaryReading: '', mainDictionary,
        sortFrequencyDictionary: sortFrequencyDictionary ?? null, sortFrequencyDictionaryOrder,
        removeNonJapaneseCharacters: false, searchResolution: 'letter', textReplacements: [null],
        enabledDictionaryMap: map, excludeDictionaryDefinitions, language: 'ja', useAllFrequencyDictionaries: false,
    };
}

// Compact form (keep in sync with corpus_parity.rs).
const tag = (t) => [t.name, t.category, t.order, t.score, t.content, t.dictionaries, t.redundant];
const countArrays = (entries) => entries.filter((e) => Array.isArray(e)).length;
function compactEntry(e) {
    return {
        h: e.headwords.map((h) => [h.term, h.reading, h.sources.map((s) => [s.originalText, s.transformedText, s.deinflectedText, s.matchSource, s.isPrimary]), h.tags.map(tag), h.wordClasses]),
        d: e.definitions.map((d) => [d.dictionary, d.id, d.sequences, d.isPrimary, d.headwordIndices, d.tags.map(tag), d.entries.length, countArrays(d.entries), d.score, d.dictionaryIndex, d.dictionaryAlias, d.frequencyOrder]),
        i: e.inflectionRuleChainCandidates.map((c) => [c.source, c.inflectionRules.map((r) => r.name)]),
        tp: e.textProcessorRuleChainCandidates,
        s: e.score, di: e.dictionaryIndex, da: e.dictionaryAlias, fo: e.frequencyOrder, pr: e.isPrimary,
        ex: e.sourceTermExactMatchCount, mx: e.maxOriginalTextLength,
        f: e.frequencies.map((f) => [f.index, f.headwordIndex, f.dictionary, f.dictionaryIndex, f.hasReading, f.frequency, f.displayValue, f.displayValueParsed, f.frequencyMode]),
        p: e.pronunciations.map((p) => [p.index, p.headwordIndex, p.dictionary, p.dictionaryIndex, p.pronunciations.map((x) => (
            x.type === 'pitch-accent' ? ['pitch', x.positions, x.nasalPositions, x.devoicePositions, x.tags.map(tag)] : ['ipa', x.ipa, x.tags.map(tag)]
        ))]),
    };
}
function compactKanji(e) {
    return [
        e.character, e.dictionary, e.dictionaryIndex, e.dictionaryAlias, e.onyomi, e.kunyomi, e.tags.map(tag),
        Object.entries(e.stats).map(([category, list]) => [category, list.map((s) => [s.name, s.category, s.content, s.order, s.score, s.value])]),
        e.definitions,
        e.frequencies.map((f) => [f.index, f.dictionary, f.dictionaryIndex, f.frequency, f.displayValue, f.displayValueParsed, f.frequencyMode]),
    ];
}

console.warn = () => {};
for (const mode of modes) {
    for (const text of texts) {
        if (mode === 'kanji') {
            const entries = await translator.findKanji(text, {enabledDictionaryMap: enabledDictionaryMap(), removeNonJapaneseCharacters: false});
            process.stdout.write(JSON.stringify({mode, text, kanji: entries.map(compactKanji)}) + '\n');
        } else {
            const {dictionaryEntries, originalTextLength} = await translator.findTerms(mode, text, termOptions(mode));
            process.stdout.write(JSON.stringify({mode, text, len: originalTextLength, entries: dictionaryEntries.map(compactEntry)}) + '\n');
        }
    }
}
