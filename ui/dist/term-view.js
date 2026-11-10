// SPDX-License-Identifier: AGPL-3.0-or-later
// Renders term lookup entries: headwords with furigana, tags, inflection
// chains, frequencies, pitch accents and definitions by dictionary, laid
// out like Yomitan's popup. Redundant tags are hidden as in Yomitan.
import { el } from './dom.js';
import { pitchAccent } from './pitch.js';
import { createGlossaryItem } from './structured-content.js';
/** A kanji character (CJK unified ideographs and extension A). */
const KANJI = /[\u3400-\u4dbf\u4e00-\u9fff]/u;
export function renderTermEntries(entries, nav) {
    const list = el('div', 'flex flex-col gap-2');
    list.lang = 'ja';
    for (const entry of entries) {
        list.appendChild(renderEntry(entry, nav));
    }
    return list;
}
function renderEntry(entry, nav) {
    const many = entry.headwords.length > 1;
    const node = el('article', 'surface-1 flex flex-col gap-2 entry');
    const heads = el('div', 'flex flex-wrap items-end gap-x-4 gap-y-2');
    for (const headword of entry.headwords) {
        heads.appendChild(renderHeadword(headword, nav));
    }
    node.appendChild(heads);
    for (const chain of entry.inflections) {
        if (chain.rules.length === 0) {
            continue;
        }
        const names = chain.rules.map((r) => r.name).join(' \u00ab ');
        const description = chain.rules.map((r) => r.description).join('\n');
        const line = el('small', 'inflection', names);
        line.title = description;
        node.appendChild(line);
    }
    const frequencies = el('div', 'flex flex-wrap gap-2');
    for (const f of entry.frequencies) {
        const term = many ? `${headwordLabel(entry.headwords[f.headword_index])} ` : '';
        const value = f.display_value ?? String(f.frequency);
        frequencies.appendChild(chip('frequency', `${f.dictionary.title}`, `${term}${value}`));
    }
    if (frequencies.childElementCount > 0) {
        node.appendChild(frequencies);
    }
    const pronunciations = renderPronunciations(entry);
    if (pronunciations !== null) {
        node.appendChild(pronunciations);
    }
    const definitions = el('ol', 'definition-list');
    for (const definition of entry.definitions) {
        definitions.appendChild(renderDefinition(definition, entry, nav));
    }
    node.appendChild(definitions);
    return node;
}
/**
 * One row per distinct pronunciation of a headword, listing the
 * dictionaries that give it (Yomitan groups them the same way).
 */
function renderPronunciations(entry) {
    const groups = new Map();
    for (const p of entry.pronunciations) {
        const reading = entry.headwords[p.headword_index]?.reading ?? '';
        for (const item of p.items) {
            const value = item.type === 'pitch_accent' ? `pitch:${item.position}` : `ipa:${item.ipa}`;
            const key = `${p.headword_index}\n${value}`;
            const group = groups.get(key);
            if (group === undefined) {
                groups.set(key, { reading, item, dictionaries: [p.dictionary.title] });
            }
            else if (!group.dictionaries.includes(p.dictionary.title)) {
                group.dictionaries.push(p.dictionary.title);
            }
        }
    }
    if (groups.size === 0) {
        return null;
    }
    const node = el('div', 'flex flex-col gap-1');
    for (const { reading, item, dictionaries } of groups.values()) {
        const shown = item.type === 'pitch_accent' ? pitchAccent(reading, item.position) : el('span', 'ipa', item.ipa);
        const sources = el('small', '', dictionaries.join(', '));
        node.appendChild(el('div', 'flex flex-wrap items-center gap-2', shown, sources));
    }
    return node;
}
function renderHeadword(headword, nav) {
    const term = el('span', 'headword-term');
    for (const segment of headword.furigana) {
        const base = linkKanji(segment.text, nav);
        if (segment.reading === '') {
            term.appendChild(base);
        }
        else {
            term.appendChild(el('ruby', '', base, el('rp', '', '('), el('rt', '', segment.reading), el('rp', '', ')')));
        }
    }
    const node = el('div', 'headword flex flex-wrap items-center gap-2', term);
    for (const tag of visibleTags(headword.tags)) {
        node.appendChild(tagChip(tag));
    }
    return node;
}
/** Kanji in `text` become links to the kanji page; the rest stays text. */
function linkKanji(text, nav) {
    const fragment = document.createDocumentFragment();
    for (const char of text) {
        if (KANJI.test(char)) {
            const link = el('a', 'headword-kanji', char);
            link.href = nav.kanjiHref(char);
            fragment.appendChild(link);
        }
        else {
            fragment.appendChild(document.createTextNode(char));
        }
    }
    return fragment;
}
function renderDefinition(definition, entry, nav) {
    // The li stays a block (list-item): flex would hide its number.
    const node = el('li', 'definition-item');
    node.dataset['dictionary'] = definition.dictionary.title;
    node.dataset['dictionaryId'] = String(definition.dictionary.id);
    const body = el('div', 'definition-body flex flex-col gap-1');
    node.appendChild(body);
    const tags = el('div', 'flex flex-wrap items-center gap-1');
    for (const tag of visibleTags(definition.tags)) {
        tags.appendChild(tagChip(tag));
    }
    tags.appendChild(chip('dictionary', definition.dictionary.title, null));
    if (entry.headwords.length > 1 && definition.headword_indices.length < entry.headwords.length) {
        const only = definition.headword_indices.map((i) => headwordLabel(entry.headwords[i])).join(', ');
        tags.appendChild(el('small', '', `(${only})`));
    }
    body.appendChild(tags);
    const glossary = el('ul', 'gloss-list');
    if (definition.glossary.length === 1) {
        glossary.classList.add('gloss-list-single');
    }
    for (const item of definition.glossary) {
        glossary.appendChild(el('li', 'gloss-item', createGlossaryItem(item, (q) => nav.search(q))));
    }
    body.appendChild(glossary);
    return node;
}
function headwordLabel(headword) {
    if (headword === undefined) {
        return '';
    }
    return headword.reading === '' || headword.reading === headword.term ? headword.term : `${headword.term} (${headword.reading})`;
}
function visibleTags(tags) {
    return tags.filter((t) => !t.redundant);
}
function tagChip(tag) {
    const node = chip(tag.category, tag.name, null);
    if (tag.notes.length > 0) {
        node.title = tag.notes.join('\n');
    }
    return node;
}
/** A small label; `category` picks its color (see app.css `.tag`). */
function chip(category, label, value) {
    const node = el('span', 'tag', el('span', 'tag-label', label), value === null ? null : el('span', 'tag-value', value));
    node.dataset['category'] = category;
    return node;
}
