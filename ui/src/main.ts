// SPDX-License-Identifier: AGPL-3.0-or-later
// Entry point: theme, navigation, the auto-search setting, routing between
// the pages, dictionary styles and the source link (AGPL-3.0 section 13).

import {getSource, listDictionaries} from './api.js';
import {loadDictionaryStyles} from './dictionary-styles.js';
import {el} from './dom.js';
import {renderKanjiPage} from './kanji-page.js';
import {renderLookupPage} from './lookup-page.js';
import {type PageName, currentRoute, href} from './page.js';
import {renderParsePage} from './parse-page.js';
import {AUTO_SEARCH_EVENT, autoSearch, setAutoSearch} from './auto-search.js';

const PAGES: {name: PageName; label: string}[] = [
    {name: 'lookup', label: 'Lookup'},
    {name: 'kanji', label: 'Kanji'},
    {name: 'parse', label: 'Parse'},
];

/** Aborted when the user leaves the current page. */
let pageController = new AbortController();

followSystemTheme();
const main = requireElement('main');
const nav = requireElement('nav');
for (const page of PAGES) {
    const link = el('a', 'nav-link', page.label);
    link.href = href(page.name, {});
    link.dataset['page'] = page.name;
    nav.appendChild(link);
}
addAutoSearchToggle();
window.addEventListener('hashchange', route);
route();
void showSource();
listDictionaries()
    .then(loadDictionaryStyles)
    .catch((error: unknown) => console.warn('dictionary styles not loaded', error));

function route(): void {
    pageController.abort();
    pageController = new AbortController();
    const signal = pageController.signal;
    const {page, params} = currentRoute();
    for (const link of nav.querySelectorAll<HTMLElement>('.nav-link')) {
        link.toggleAttribute('aria-current', link.dataset['page'] === page);
    }
    switch (page) {
        case 'lookup':
            renderLookupPage(main, params, signal);
            break;
        case 'kanji':
            renderKanjiPage(main, params, signal);
            break;
        case 'parse':
            renderParsePage(main, params, signal);
            break;
    }
}

/** The "Search as you type" checkbox in the header (saved across visits). */
function addAutoSearchToggle(): void {
    const box = el('input', '');
    box.type = 'checkbox';
    box.checked = autoSearch();
    box.addEventListener('change', () => setAutoSearch(box.checked));
    // Another tab changed it.
    window.addEventListener(AUTO_SEARCH_EVENT, () => {
        box.checked = autoSearch();
    });
    requireElement('header').appendChild(el('label', 'ml-auto flex items-center gap-2', box, 'Search as you type'));
}

/** ponkotsu's theme is dark unless the root has data-theme="light". */
function followSystemTheme(): void {
    const light = window.matchMedia('(prefers-color-scheme: light)');
    const apply = (): void => {
        document.documentElement.dataset['theme'] = light.matches ? 'light' : 'dark';
    };
    apply();
    light.addEventListener('change', apply);
}

async function showSource(): Promise<void> {
    const footer = requireElement('footer');
    try {
        const source = await getSource();
        const text = `jisho ${source.version}, AGPL-3.0-or-later`;
        if (source.repository === null) {
            footer.replaceChildren(el('small', '', text));
            return;
        }
        const link = el('a', '', 'source');
        link.href = source.repository;
        link.rel = 'noreferrer noopener';
        footer.replaceChildren(el('small', '', `${text}, `, link));
    } catch (error) {
        console.warn('source information not loaded', error);
    }
}

function requireElement(selector: string): HTMLElement {
    const node = document.querySelector<HTMLElement>(selector);
    if (node === null) {
        throw new Error(`index.html has no ${selector}`);
    }
    return node;
}
