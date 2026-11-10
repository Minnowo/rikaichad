// SPDX-License-Identifier: AGPL-3.0-or-later
// Entry point: theme, navigation, routing between the pages, dictionary
// styles and the source link (AGPL-3.0 section 13).
import { getSource, listDictionaries } from './api.js';
import { loadDictionaryStyles } from './dictionary-styles.js';
import { el } from './dom.js';
import { renderKanjiPage } from './kanji-page.js';
import { renderLookupPage } from './lookup-page.js';
import { currentRoute, href } from './page.js';
import { renderParsePage } from './parse-page.js';
const PAGES = [
    { name: 'lookup', label: 'Lookup' },
    { name: 'kanji', label: 'Kanji' },
    { name: 'parse', label: 'Parse' },
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
window.addEventListener('hashchange', route);
route();
void showSource();
listDictionaries()
    .then(loadDictionaryStyles)
    .catch((error) => console.warn('dictionary styles not loaded', error));
function route() {
    pageController.abort();
    pageController = new AbortController();
    const signal = pageController.signal;
    const { page, params } = currentRoute();
    for (const link of nav.querySelectorAll('.nav-link')) {
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
/** ponkotsu's theme is dark unless the root has data-theme="light". */
function followSystemTheme() {
    const light = window.matchMedia('(prefers-color-scheme: light)');
    const apply = () => {
        document.documentElement.dataset['theme'] = light.matches ? 'light' : 'dark';
    };
    apply();
    light.addEventListener('change', apply);
}
async function showSource() {
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
    }
    catch (error) {
        console.warn('source information not loaded', error);
    }
}
function requireElement(selector) {
    const node = document.querySelector(selector);
    if (node === null) {
        throw new Error(`index.html has no ${selector}`);
    }
    return node;
}
