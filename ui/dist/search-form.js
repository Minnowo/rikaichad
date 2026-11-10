// SPDX-License-Identifier: AGPL-3.0-or-later
// The search row of the lookup and kanji pages: the text box, extra
// controls, and a Search button shown while auto-search is off. Enter
// always searches at once.
import { el } from './dom.js';
import { AUTO_SEARCH_EVENT, autoSearch } from './auto-search.js';
/** Builds the form. Listeners end with `page`. */
export function searchForm(input, extras, page, handlers) {
    const button = el('button', 'btn-primary', 'Search');
    button.type = 'submit';
    const form = el('form', 'flex flex-wrap items-center gap-2', input, ...extras, button);
    const showButton = () => {
        button.hidden = autoSearch();
    };
    showButton();
    window.addEventListener(AUTO_SEARCH_EVENT, () => {
        showButton();
        handlers.onAutoSearch(autoSearch());
    }, { signal: page });
    input.addEventListener('input', () => handlers.onType());
    form.addEventListener('submit', (event) => {
        event.preventDefault();
        handlers.searchNow();
    });
    return form;
}
