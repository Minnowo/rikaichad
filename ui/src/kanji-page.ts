// SPDX-License-Identifier: AGPL-3.0-or-later
// Kanji lookup: every distinct character of the input in each enabled kanji
// dictionary, with readings, meanings, stats and frequencies.

import {ApiError, type KanjiEntry, lookupKanji} from './api.js';
import {el, message} from './dom.js';
import {debounced, replaceParams} from './page.js';

/**
 * Builds the kanji page for `params` (q) into `root`; `page` is aborted
 * when the user navigates away.
 */
export function renderKanjiPage(root: HTMLElement, params: URLSearchParams, page: AbortSignal): void {
    const input = el('input', 'flex-1');
    input.type = 'search';
    input.placeholder = 'One or more kanji';
    input.setAttribute('aria-label', 'Kanji to look up');
    input.lang = 'ja';
    input.value = params.get('q') ?? '';

    const results = el('div', 'flex flex-col gap-2');
    results.lang = 'ja';

    const search = debounced(150, page, async (signal) => {
        const text = input.value.trim();
        replaceParams('kanji', text === '' ? {} : {q: text});
        if (text === '') {
            results.replaceChildren();
            return;
        }
        try {
            const response = await lookupKanji(text, signal);
            if (response.entries.length === 0) {
                results.replaceChildren(message('secondary', 'No kanji found.'));
                return;
            }
            results.replaceChildren(...response.entries.map(renderKanji));
        } catch (error) {
            if (error instanceof ApiError) {
                results.replaceChildren(message('error', error.message));
                return;
            }
            throw error;
        }
    });
    input.addEventListener('input', search);

    root.replaceChildren(el('section', 'flex flex-col gap-4', el('div', 'flex items-center gap-2', input), results));
    input.focus();
    if (input.value !== '') {
        search();
    }
}

function renderKanji(entry: KanjiEntry): HTMLElement {
    const readings = el(
        'div',
        'flex flex-col gap-1',
        readingRow('On', entry.onyomi),
        readingRow('Kun', entry.kunyomi),
    );
    const meanings = el('ol', 'kanji-meanings');
    for (const meaning of entry.meanings) {
        meanings.appendChild(el('li', '', meaning));
    }
    const header = el(
        'div',
        'flex flex-wrap items-start gap-4',
        el('p', 'kanji-character', entry.character),
        el('div', 'flex flex-1 flex-col gap-2', readings, meanings),
    );

    const tags = el('div', 'flex flex-wrap gap-1');
    for (const tag of entry.tags) {
        const node = el('span', 'tag', el('span', 'tag-label', tag.name));
        node.dataset['category'] = tag.category;
        node.title = tag.notes.join('\n');
        tags.appendChild(node);
    }
    const dictionary = el('span', 'tag', el('span', 'tag-label', entry.dictionary.title));
    dictionary.dataset['category'] = 'dictionary';
    tags.appendChild(dictionary);
    for (const f of entry.frequencies) {
        const node = el('span', 'tag', el('span', 'tag-label', f.dictionary.title), el('span', 'tag-value', f.display_value ?? String(f.frequency)));
        node.dataset['category'] = 'frequency';
        tags.appendChild(node);
    }

    const stats = el('div', 'flex flex-wrap gap-4');
    for (const group of entry.stats) {
        const table = el('table', '');
        const body = el('tbody', '');
        for (const stat of group.stats) {
            const name = el('th', '', stat.notes === '' ? stat.name : stat.notes);
            body.appendChild(el('tr', '', name, el('td', '', stat.value)));
        }
        table.appendChild(body);
        stats.appendChild(el('div', 'flex flex-col gap-1', el('h4', '', group.category), el('div', 'overflow-x-auto', table)));
    }

    return el('article', 'surface-1 flex flex-col gap-4', header, tags, entry.stats.length > 0 ? stats : null);
}

function readingRow(label: string, readings: string[]): HTMLElement | null {
    if (readings.length === 0) {
        return null;
    }
    return el('p', 'flex flex-wrap items-baseline gap-2', el('small', '', label), readings.join('\u3001'));
}
