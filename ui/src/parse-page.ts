// SPDX-License-Identifier: AGPL-3.0-or-later
// Sentence parse: the text split into words with furigana (Yomitan's
// scanning parser); clicking a word shows its lookup below.

import {ApiError, type ParseToken, lookupTerms, parseText} from './api.js';
import {el, message, ruby} from './dom.js';
import {href, replaceParams} from './page.js';
import {renderTermEntries} from './term-view.js';

/**
 * Builds the parse page for `params` (q) into `root`; `page` is aborted
 * when the user navigates away.
 */
export function renderParsePage(root: HTMLElement, params: URLSearchParams, page: AbortSignal): void {
    const input = el('textarea', '');
    input.rows = 3;
    input.placeholder = 'A Japanese sentence or paragraph';
    input.setAttribute('aria-label', 'Text to parse');
    input.lang = 'ja';
    input.value = params.get('q') ?? '';
    const button = el('button', 'btn-primary', 'Parse');
    button.type = 'submit';
    const form = el('form', 'flex flex-col gap-2', input, el('div', 'flex justify-end gap-2', button));

    const sentence = el('p', 'parsed-text');
    sentence.lang = 'ja';
    const lookup = el('div', 'flex flex-col gap-2');
    // One request of each kind at a time; a new one aborts the old, and
    // leaving the page aborts both.
    let parseController: AbortController | undefined;
    let lookupController: AbortController | undefined;
    page.addEventListener('abort', () => {
        parseController?.abort();
        lookupController?.abort();
    });

    const show = async (token: ParseToken, node: HTMLElement): Promise<void> => {
        for (const selected of sentence.querySelectorAll('.token-selected')) {
            selected.classList.remove('token-selected');
        }
        node.classList.add('token-selected');
        lookupController?.abort();
        lookupController = new AbortController();
        if (page.aborted) {
            return;
        }
        try {
            const response = await lookupTerms(token.text, 'group', lookupController.signal);
            lookup.replaceChildren(
                renderTermEntries(response.entries, {
                    search: (query) => {
                        location.hash = href('lookup', {q: query});
                    },
                    kanjiHref: (character) => href('kanji', {q: character}),
                }),
            );
        } catch (error) {
            if (error instanceof ApiError) {
                lookup.replaceChildren(message('error', error.message));
                return;
            }
            if (!(error instanceof DOMException && error.name === 'AbortError')) {
                throw error;
            }
        }
    };

    const parse = async (): Promise<void> => {
        parseController?.abort();
        lookupController?.abort();
        if (page.aborted) {
            return;
        }
        const controller = new AbortController();
        parseController = controller;
        const text = input.value.trim();
        replaceParams('parse', text === '' ? {} : {q: text});
        lookup.replaceChildren();
        if (text === '') {
            sentence.replaceChildren();
            return;
        }
        button.disabled = true;
        try {
            const response = await parseText(text, controller.signal);
            sentence.replaceChildren(...response.tokens.map((token) => renderToken(token, show)));
        } catch (error) {
            if (error instanceof ApiError) {
                sentence.replaceChildren(message('error', error.message));
                return;
            }
            if (!(error instanceof DOMException && error.name === 'AbortError')) {
                throw error;
            }
        } finally {
            if (parseController === controller) {
                button.disabled = false;
            }
        }
    };

    form.addEventListener('submit', (event) => {
        event.preventDefault();
        void parse();
    });
    input.addEventListener('keydown', (event) => {
        if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            void parse();
        }
    });

    root.replaceChildren(el('section', 'flex flex-col gap-4', el('div', 'surface-1 flex flex-col gap-4', form, sentence), lookup));
    input.focus();
    if (input.value !== '') {
        void parse();
    }
}

/** A matched word is a button that shows its lookup; other text is plain. */
function renderToken(token: ParseToken, show: (token: ParseToken, node: HTMLElement) => Promise<void>): Node {
    if (token.headwords.length === 0) {
        return el('span', 'token', ruby(token.segments));
    }
    const node = el('button', 'token token-word', ruby(token.segments));
    node.type = 'button';
    node.title = token.headwords
        .flat()
        .map((h) => (h.reading === '' || h.reading === h.term ? h.term : `${h.term} (${h.reading})`))
        .join('\n');
    node.addEventListener('click', () => void show(token, node));
    return node;
}
