// SPDX-License-Identifier: AGPL-3.0-or-later
// The "Show forms" checkbox on a headword: shows the word's conjugation table
// (POST /v1/terms/forms) as rows of forms by plain/negative and polite,
// each row labelled in English and Japanese.

import {ApiError, conjugate, type ConjugatedForm, type Headword} from './api.js';
import {el, message} from './dom.js';

/** Word classes the server can conjugate (docs/api.md). */
const CONJUGABLE = new Set(['v1', 'v5', 'vk', 'vs', 'adj-i']);

/** Row labels by form name, English and Japanese; unknown names are shown as they are. */
const FORM_LABELS: Record<string, {en: string; ja: string}> = {
    'non-past': {en: 'Non-past', ja: '\u73fe\u5728\u5f62'},
    past: {en: 'Past', ja: '\u904e\u53bb\u5f62'},
    te: {en: '-te', ja: '\uff5e\u3066\u5f62'},
    adverbial: {en: 'Adverbial', ja: '\uff5e\u304f\u5f62'},
    conditional: {en: 'Conditional (-ba)', ja: '\uff5e\u3070\u5f62'},
    tara: {en: 'Conditional (-tara)', ja: '\uff5e\u305f\u3089\u5f62'},
    volitional: {en: 'Volitional', ja: '\u610f\u5411\u5f62'},
    imperative: {en: 'Imperative', ja: '\u547d\u4ee4\u5f62'},
    desire: {en: 'Desire (-tai)', ja: '\uff5e\u305f\u3044\u5f62'},
    progressive: {en: 'Progressive', ja: '\uff5e\u3066\u3044\u308b\u5f62'},
    potential: {en: 'Potential', ja: '\u53ef\u80fd\u5f62'},
    passive: {en: 'Passive', ja: '\u53d7\u8eab\u5f62'},
    causative: {en: 'Causative', ja: '\u4f7f\u5f79\u5f62'},
    'causative-passive': {en: 'Causative passive', ja: '\u4f7f\u5f79\u53d7\u8eab\u5f62'},
    noun: {en: 'Noun (-sa)', ja: '\u540d\u8a5e\u5f62'},
};

/** Panels made so far, for unique element ids. */
let panelCount = 0;

/** Columns: [polite, negative] and their headings. */
const COLUMNS: {polite: boolean; negative: boolean; label: string}[] = [
    {polite: false, negative: false, label: 'Plain'},
    {polite: false, negative: true, label: 'Negative'},
    {polite: true, negative: false, label: 'Polite'},
    {polite: true, negative: true, label: 'Polite negative'},
];

/**
 * A "Show forms" checkbox (in its label) that shows or hides `headword`'s conjugation table inside
 * `container`, or null if none of `wordClasses` is conjugable. The table
 * is fetched on first use; `signal` cancels that request.
 */
export function formsToggle(
    headword: Headword,
    wordClasses: string[],
    container: HTMLElement,
    signal: AbortSignal,
): HTMLLabelElement | null {
    if (!wordClasses.some((c) => CONJUGABLE.has(c))) {
        return null;
    }
    const box = el('input', '');
    box.type = 'checkbox';
    box.setAttribute('aria-label', `Show forms of ${headword.term}`);
    const panel = el('div', 'forms-panel');
    panel.id = `forms-${++panelCount}`;
    panel.hidden = true;
    box.setAttribute('aria-controls', panel.id);
    container.appendChild(panel);
    let state: 'empty' | 'loading' | 'loaded' = 'empty';
    box.addEventListener('change', () => {
        panel.hidden = !box.checked;
        if (!box.checked || state !== 'empty') {
            return;
        }
        state = 'loading';
        const loading = el('p', 'text-c-on-surface-variant', 'Loading forms...');
        loading.setAttribute('role', 'status');
        panel.replaceChildren(loading);
        void fill(panel, headword, wordClasses, signal).then((loaded) => {
            // A failed or cancelled request is tried again when checked again.
            state = loaded ? 'loaded' : 'empty';
            if (!loaded && panel.hidden) {
                box.checked = false;
            }
        });
    });
    return el('label', 'forms-toggle', box, 'Show forms');
}

/** Fetches and shows the table; true once it is shown (or known empty). */
async function fill(panel: HTMLElement, headword: Headword, wordClasses: string[], signal: AbortSignal): Promise<boolean> {
    try {
        const response = await conjugate(headword.term, headword.reading, wordClasses, signal);
        if (response.forms.length === 0) {
            panel.replaceChildren(message('secondary', `No conjugation table for ${headword.term}.`));
        } else {
            panel.replaceChildren(renderTable(response.term, response.forms));
        }
        return true;
    } catch (error) {
        if (error instanceof DOMException && error.name === 'AbortError') {
            panel.replaceChildren();
            panel.hidden = true;
            return false;
        }
        const text = error instanceof ApiError ? error.message : String(error);
        panel.replaceChildren(message('error', `${text} (check Show forms again to retry)`));
        return false;
    }
}

function renderTable(term: string, forms: ConjugatedForm[]): HTMLElement {
    const rows = new Map<string, ConjugatedForm[]>();
    for (const form of forms) {
        const row = rows.get(form.name);
        if (row === undefined) {
            rows.set(form.name, [form]);
        } else {
            row.push(form);
        }
    }
    const head = el('tr', '', el('td', ''));
    for (const column of COLUMNS) {
        const th = el('th', '', column.label);
        th.scope = 'col';
        head.appendChild(th);
    }
    const body = el('tbody', '');
    for (const [name, row] of rows) {
        const label = FORM_LABELS[name];
        const th = el('th', '', label?.en ?? name);
        if (label !== undefined) {
            th.appendChild(el('span', 'forms-label-ja', label.ja));
        }
        th.scope = 'row';
        const tr = el('tr', '', th);
        for (const column of COLUMNS) {
            const cell = el('td', '');
            for (const form of row) {
                if (form.polite === column.polite && form.negative === column.negative) {
                    // No furigana (the headword has it); the reading is a
                    // tooltip, for forms like kuru's where it changes.
                    const shown = el('div', '', form.term);
                    if (form.reading !== form.term) {
                        shown.title = form.reading;
                    }
                    cell.appendChild(shown);
                }
            }
            tr.appendChild(cell);
        }
        body.appendChild(tr);
    }
    const table = el('table', 'forms-table', el('caption', '', `Forms of ${term}`), el('thead', '', head), body);
    return el('div', 'overflow-x-auto', table);
}
