// SPDX-License-Identifier: AGPL-3.0-or-later
// Each page's state for this browser tab (sessionStorage), so switching
// pages keeps your searches. Storage can be unavailable (private mode,
// blocked site data); then nothing is remembered and pages start empty.

import type {PageName} from './page.js';

/** A page's state: the box text, the query whose results are shown, and
 * page-specific values (lookup: result mode; parse: selected word). */
export interface PageState {
    text: string;
    shown: string;
    mode: string;
    selected: number;
}

const EMPTY_STATE: PageState = {text: '', shown: '', mode: '', selected: -1};

/** The saved state of `page` in this tab, or an empty state. */
export function loadPageState(page: PageName): PageState {
    const raw = read(() => sessionStorage.getItem(pageKey(page)));
    if (raw === null) {
        return {...EMPTY_STATE};
    }
    try {
        const value = JSON.parse(raw) as Partial<Record<keyof PageState, unknown>>;
        return {
            text: typeof value.text === 'string' ? value.text : '',
            shown: typeof value.shown === 'string' ? value.shown : '',
            mode: typeof value.mode === 'string' ? value.mode : '',
            selected: typeof value.selected === 'number' ? value.selected : -1,
        };
    } catch {
        return {...EMPTY_STATE};
    }
}

/** Saves the state of `page` for this tab. */
export function savePageState(page: PageName, state: PageState): void {
    write(() => sessionStorage.setItem(pageKey(page), JSON.stringify(state)));
}

function pageKey(page: PageName): string {
    return `jisho.page.${page}`;
}

/** Runs a storage read; null when storage throws. */
export function read(get: () => string | null): string | null {
    try {
        return get();
    } catch {
        return null;
    }
}

/** Runs a storage write; failures are ignored. */
export function write(set: () => void): void {
    try {
        set();
    } catch {
        // Storage full or blocked: the UI works without remembering.
    }
}
