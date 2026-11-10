// SPDX-License-Identifier: AGPL-3.0-or-later
// Word lookup: Yomitan-style scan of the start of the typed text, results
// updating as you type.
import { ApiError, lookupTerms } from './api.js';
import { el, message } from './dom.js';
import { debounced, href, replaceParams } from './page.js';
import { renderTermEntries } from './term-view.js';
const RESULT_MODES = [
    { value: 'group', label: 'Group by word' },
    { value: 'merge', label: 'Merge by main dictionary' },
    { value: 'term', label: 'Group by term' },
    { value: 'split', label: 'One per definition' },
];
/**
 * Builds the lookup page for `params` (q, mode) into `root`; `page` is
 * aborted when the user navigates away.
 */
export function renderLookupPage(root, params, page) {
    const input = el('input', 'flex-1');
    input.type = 'search';
    input.placeholder = 'Japanese or romaji, e.g. taberarenakatta';
    input.setAttribute('aria-label', 'Text to look up');
    input.lang = 'ja';
    input.value = params.get('q') ?? '';
    const mode = el('select', '');
    mode.setAttribute('aria-label', 'Result grouping');
    for (const option of RESULT_MODES) {
        const node = el('option', '', option.label);
        node.value = option.value;
        mode.appendChild(node);
    }
    const requested = params.get('mode');
    mode.value = RESULT_MODES.some((m) => m.value === requested) ? requested : 'group';
    const status = el('small', '');
    const results = el('div', 'flex flex-col gap-2');
    const search = debounced(150, page, async (signal) => {
        const text = input.value.trim();
        const resultMode = mode.value;
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
                search: (query) => {
                    input.value = query;
                    search();
                    window.scrollTo({ top: 0 });
                },
                kanjiHref: (character) => href('kanji', { q: character }),
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
    input.addEventListener('input', search);
    mode.addEventListener('change', search);
    root.replaceChildren(el('section', 'flex flex-col gap-4', el('div', 'flex flex-wrap items-center gap-2', input, mode), status, results));
    input.focus();
    if (input.value !== '') {
        search();
    }
}
