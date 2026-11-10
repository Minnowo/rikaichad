// SPDX-License-Identifier: AGPL-3.0-or-later
// Dumps Yomitan's Japanese deinflection table as JSON, with every rule
// expanded to plain data, for crates/jisho-ja/data/japanese-transforms.json.
//
// Usage: node scripts/port/dump-japanese-transforms.mjs <yomitan checkout> > out.json
//
// Rules are emitted in Yomitan's order (transform object order, then rule
// order), which the Rust port keeps: deinflection results come out in the
// same breadth-first order as Yomitan's LanguageTransformer.transform.

import {pathToFileURL} from 'node:url';
import {join} from 'node:path';

const root = process.argv[2];
if (!root) {
    console.error('usage: dump-japanese-transforms.mjs <yomitan checkout>');
    process.exit(2);
}
const moduleUrl = pathToFileURL(join(root, 'ext/js/language/ja/japanese-transforms.js')).href;
const {japaneseTransforms} = await import(moduleUrl);

const conditions = Object.entries(japaneseTransforms.conditions).map(([id, c]) => ({
    id,
    name: c.name,
    isDictionaryForm: c.isDictionaryForm,
    subConditions: c.subConditions ?? null,
}));

const transforms = Object.entries(japaneseTransforms.transforms).map(([id, t]) => ({
    id,
    name: t.name,
    description: t.description ?? null,
    rules: t.rules.map((rule) => {
        const source = rule.isInflected.source;
        let inflected;
        let deinflected;
        if (rule.type === 'suffix') {
            if (!source.endsWith('$')) { throw new Error(`suffix rule without $: ${source}`); }
            inflected = source.slice(0, -1);
            deinflected = rule.deinflected;
        } else if (rule.type === 'wholeWord') {
            if (!source.startsWith('^') || !source.endsWith('$')) { throw new Error(`bad whole-word rule: ${source}`); }
            inflected = source.slice(1, -1);
            deinflected = rule.deinflect('');
        } else {
            throw new Error(`unsupported rule type ${rule.type} in ${id}`);
        }
        if (/[.*+?^${}()|[\]\\]/.test(inflected)) { throw new Error(`regex metacharacter in ${inflected}`); }
        if (inflected.length === 0) { throw new Error(`empty inflected text in ${id}`); }
        return {
            type: rule.type,
            inflected,
            deinflected,
            conditionsIn: rule.conditionsIn,
            conditionsOut: rule.conditionsOut,
        };
    }),
}));

process.stdout.write(JSON.stringify({language: japaneseTransforms.language, conditions, transforms}, null, 1) + '\n');
