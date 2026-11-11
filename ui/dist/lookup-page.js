// SPDX-License-Identifier: AGPL-3.0-or-later
// Word lookup: Yomitan-style scan of the start of the typed text, results
// updating as you type (unless auto-search is off).
import { ApiError, lookupTerms } from './api.js';
import { el, message } from './dom.js';
import { debounced, href, replaceParams } from './page.js';
import { searchForm } from './search-form.js';
import { autoSearch } from './auto-search.js';
import { loadPageState, savePageState } from './storage.js';
import { renderTermEntries } from './term-view.js';
const RESULT_MODES = [
    { value: 'group', label: 'Group by word' },
    { value: 'merge', label: 'Merge by main dictionary' },
    { value: 'term', label: 'Group by term' },
    { value: 'split', label: 'One per definition' },
];
/**
 * Builds the lookup page into `root`; `page` is aborted when the user
 * navigates away. The page comes back as it was left in this tab (box
 * text, shown results, mode), unless the URL asks for other results (a
 * link); the page's own URL after a search (reload, Back) keeps the state.
 */
export function renderLookupPage(root, params, page) {
    const state = loadPageState('lookup');
    const linked = params.get('q');
    const linkedMode = params.get('mode');
    if (linked !== null && (linked !== state.shown || (linkedMode !== null && linkedMode !== state.mode))) {
        state.text = linked;
        state.shown = linked;
        state.mode = linkedMode ?? state.mode;
    }
    // Phones: the box gets its own row, the mode and button the next one.
    const input = el('input', 'w-full sm:w-auto sm:flex-1');
    input.type = 'search';
    input.placeholder = 'Japanese or romaji, e.g. taberarenakatta';
    input.setAttribute('aria-label', 'Text to look up');
    input.lang = 'ja';
    input.value = state.text;
    const mode = el('select', 'flex-1 sm:flex-none');
    mode.setAttribute('aria-label', 'Result grouping');
    for (const option of RESULT_MODES) {
        const node = el('option', '', option.label);
        node.value = option.value;
        mode.appendChild(node);
    }
    mode.value = RESULT_MODES.some((m) => m.value === state.mode) ? state.mode : 'group';
    const status = el('small', '');
    const results = el('div', 'flex flex-col gap-2');
    const save = () => {
        state.text = input.value;
        savePageState('lookup', state);
    };
    // The query to run: the box text, or the restored shown query.
    let query = '';
    const search = debounced(150, page, async (signal) => {
        const text = query;
        const resultMode = mode.value;
        // The saved mode is always the mode of the shown results.
        state.shown = text;
        state.mode = resultMode;
        save();
        replaceParams('lookup', text === '' ? {} : { q: text, mode: resultMode });
        if (text === '') {
            status.textContent = '';
            results.replaceChildren();
            return;
        }
        try {
            const response = await lookupTerms(text, resultMode, signal);
            if (response.entries.length === 0) {
                status.textContent = '';
                results.replaceChildren(message('secondary', 'No results.'));
                return;
            }
            const count = `${response.entries.length}${response.truncated ? '+' : ''} results`;
            status.textContent = `${count} for \u300c${response.matched.text}\u300d`;
            results.replaceChildren(renderTermEntries(response.entries, {
                search: (linkedQuery) => {
                    input.value = linkedQuery;
                    query = linkedQuery;
                    search.now();
                    window.scrollTo({ top: 0 });
                },
                kanjiHref: (character) => href('kanji', { q: character }),
                signal: page,
            }));
        }
        catch (error) {
            if (error instanceof ApiError) {
                status.textContent = '';
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
    const form = searchForm(input, [mode], page, {
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
    // Choosing a mode is not typing: it always regroups the shown results
    // (or searches the box if nothing is shown yet).
    mode.addEventListener('change', () => {
        query = state.shown !== '' ? state.shown : input.value.trim();
        search.now();
    });
    root.replaceChildren(el('section', 'flex flex-col gap-4', form, status, results));
    input.focus();
    if (state.shown !== '') {
        query = state.shown;
        search.now();
    }
}
