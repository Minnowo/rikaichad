// SPDX-License-Identifier: AGPL-3.0-or-later
// Sentence parse: the text split into words with furigana (Yomitan's
// scanning parser); clicking a word shows its lookup below.
import { ApiError, lookupTerms, parseText } from './api.js';
import { el, message, ruby } from './dom.js';
import { href, replaceParams } from './page.js';
import { loadPageState, savePageState } from './storage.js';
import { renderTermEntries } from './term-view.js';
/**
 * Builds the parse page into `root`; `page` is aborted when the user
 * navigates away. The page comes back as it was left in this tab (box
 * text, parsed text, selected word), unless the URL asks for another text
 * (a link).
 */
export function renderParsePage(root, params, page) {
    const state = loadPageState('parse');
    const linked = params.get('q');
    if (linked !== null && linked !== state.shown) {
        state.text = linked;
        state.shown = linked;
        state.selected = -1;
    }
    const input = el('textarea', '');
    input.rows = 3;
    input.placeholder = 'A Japanese sentence or paragraph';
    input.setAttribute('aria-label', 'Text to parse');
    input.lang = 'ja';
    input.value = state.text;
    const button = el('button', 'btn-primary', 'Parse');
    button.type = 'submit';
    const form = el('form', 'flex flex-col gap-2', input, el('div', 'flex justify-end gap-2', button));
    const save = () => {
        state.text = input.value;
        savePageState('parse', state);
    };
    const sentence = el('p', 'parsed-text');
    sentence.lang = 'ja';
    const lookup = el('div', 'flex flex-col gap-2');
    // One request of each kind at a time; a new one aborts the old, and
    // leaving the page aborts both.
    let parseController;
    let lookupController;
    page.addEventListener('abort', () => {
        parseController?.abort();
        lookupController?.abort();
    });
    const show = async (token, node, index) => {
        for (const selected of sentence.querySelectorAll('.token-selected')) {
            selected.classList.remove('token-selected');
        }
        node.classList.add('token-selected');
        state.selected = index;
        save();
        lookupController?.abort();
        lookupController = new AbortController();
        if (page.aborted) {
            return;
        }
        try {
            const response = await lookupTerms(token.text, 'group', lookupController.signal);
            lookup.replaceChildren(renderTermEntries(response.entries, {
                search: (query) => {
                    location.hash = href('lookup', { q: query });
                },
                kanjiHref: (character) => href('kanji', { q: character }),
            }));
        }
        catch (error) {
            if (error instanceof ApiError) {
                lookup.replaceChildren(message('error', error.message));
                return;
            }
            if (!(error instanceof DOMException && error.name === 'AbortError')) {
                throw error;
            }
        }
    };
    /** Parses `text`; then shows the lookup of word `selected` if >= 0. */
    const parse = async (text, selected) => {
        parseController?.abort();
        lookupController?.abort();
        if (page.aborted) {
            return;
        }
        const controller = new AbortController();
        parseController = controller;
        // A restore (selected >= 0) keeps the saved selection until the word
        // is shown again, so leaving before the response does not lose it;
        // any other parse starts without one.
        if (text !== state.shown || selected < 0) {
            state.shown = text;
            state.selected = -1;
            save();
        }
        replaceParams('parse', text === '' ? {} : { q: text });
        lookup.replaceChildren();
        if (text === '') {
            sentence.replaceChildren();
            return;
        }
        button.disabled = true;
        try {
            const response = await parseText(text, controller.signal);
            const nodes = response.tokens.map((token, index) => renderToken(token, index, show));
            sentence.replaceChildren(...nodes);
            const node = nodes[selected];
            const token = response.tokens[selected];
            if (node instanceof HTMLElement && token !== undefined && token.headwords.length > 0) {
                void show(token, node, selected);
            }
        }
        catch (error) {
            if (error instanceof ApiError) {
                sentence.replaceChildren(message('error', error.message));
                return;
            }
            if (!(error instanceof DOMException && error.name === 'AbortError')) {
                throw error;
            }
        }
        finally {
            if (parseController === controller) {
                button.disabled = false;
            }
        }
    };
    form.addEventListener('submit', (event) => {
        event.preventDefault();
        void parse(input.value.trim(), -1);
    });
    input.addEventListener('input', save);
    input.addEventListener('keydown', (event) => {
        if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            void parse(input.value.trim(), -1);
        }
    });
    root.replaceChildren(el('section', 'flex flex-col gap-4', el('div', 'surface-1 flex flex-col gap-4', form, sentence), lookup));
    input.focus();
    if (state.shown !== '') {
        void parse(state.shown, state.selected);
    }
}
/** A matched word is a button that shows its lookup; other text is plain. */
function renderToken(token, index, show) {
    if (token.headwords.length === 0) {
        return el('span', 'token', ruby(token.segments));
    }
    const node = el('button', 'token token-word', ruby(token.segments));
    node.type = 'button';
    node.title = token.headwords
        .flat()
        .map((h) => (h.reading === '' || h.reading === h.term ? h.term : `${h.term} (${h.reading})`))
        .join('\n');
    node.addEventListener('click', () => void show(token, node, index));
    return node;
}
