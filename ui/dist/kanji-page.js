// SPDX-License-Identifier: AGPL-3.0-or-later
// Kanji lookup: every distinct character of the input in each enabled kanji
// dictionary, with readings, meanings, stats and frequencies.
import { ApiError, lookupKanji } from './api.js';
import { el, message } from './dom.js';
import { debounced, replaceParams } from './page.js';
import { searchForm } from './search-form.js';
import { autoSearch } from './auto-search.js';
import { loadPageState, savePageState } from './storage.js';
/**
 * Builds the kanji page into `root`; `page` is aborted when the user
 * navigates away. The page comes back as it was left in this tab, unless
 * the URL asks for other results (a link).
 */
export function renderKanjiPage(root, params, page) {
    const state = loadPageState('kanji');
    const linked = params.get('q');
    if (linked !== null && linked !== state.shown) {
        state.text = linked;
        state.shown = linked;
    }
    const input = el('input', 'flex-1');
    input.type = 'search';
    input.placeholder = 'One or more kanji';
    input.setAttribute('aria-label', 'Kanji to look up');
    input.lang = 'ja';
    input.value = state.text;
    const results = el('div', 'flex flex-col gap-2');
    results.lang = 'ja';
    const save = () => {
        state.text = input.value;
        savePageState('kanji', state);
    };
    // The query to run: the box text, or the restored shown query.
    let query = '';
    const search = debounced(150, page, async (signal) => {
        const text = query;
        state.shown = text;
        save();
        replaceParams('kanji', text === '' ? {} : { q: text });
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
        }
        catch (error) {
            if (error instanceof ApiError) {
                results.replaceChildren(message('error', error.message));
                return;
            }
            throw error;
        }
    });
    const fromBox = (run) => {
        query = input.value.trim();
        run();
    };
    const form = searchForm(input, [], page, {
        onType: () => {
            save();
            if (autoSearch()) {
                fromBox(search.schedule);
            }
        },
        searchNow: () => fromBox(search.now),
        onAutoSearch: (on) => {
            if (!on) {
                search.cancelPending();
            }
            else if (input.value.trim() !== state.shown) {
                fromBox(search.now);
            }
        },
    });
    root.replaceChildren(el('section', 'flex flex-col gap-4', form, results));
    input.focus();
    if (state.shown !== '') {
        query = state.shown;
        search.now();
    }
}
function renderKanji(entry) {
    const readings = el('div', 'flex flex-col gap-1', readingRow('On', entry.onyomi), readingRow('Kun', entry.kunyomi));
    const meanings = el('ol', 'kanji-meanings');
    for (const meaning of entry.meanings) {
        meanings.appendChild(el('li', '', meaning));
    }
    const header = el('div', 'flex flex-wrap items-start gap-4', el('p', 'kanji-character', entry.character), el('div', 'flex flex-1 flex-col gap-2', readings, meanings));
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
function readingRow(label, readings) {
    if (readings.length === 0) {
        return null;
    }
    return el('p', 'flex flex-wrap items-baseline gap-2', el('small', '', label), readings.join('\u3001'));
}
