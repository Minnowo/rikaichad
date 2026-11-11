// SPDX-License-Identifier: AGPL-3.0-or-later
// The "Search as you type" setting: kept across visits in localStorage
// (default on) and shared by open tabs. Changes fire AUTO_SEARCH_EVENT on
// window, also when another tab changed it.

import {read, write} from './storage.js';

const AUTO_SEARCH_KEY = 'rikaichad.autoSearch';
/** Fired on window when the auto-search setting changes. */
export const AUTO_SEARCH_EVENT = 'rikaichad:auto-search';

/** The setting as last read or set; kept here so a failed write still
 * takes effect for this page load. */
let autoSearchValue: boolean | null = null;

/** Whether lookups run while typing (default on). */
export function autoSearch(): boolean {
    if (autoSearchValue === null) {
        autoSearchValue = read(() => localStorage.getItem(AUTO_SEARCH_KEY)) !== 'off';
    }
    return autoSearchValue;
}

/** Saves the auto-search setting and tells the open page. */
export function setAutoSearch(on: boolean): void {
    autoSearchValue = on;
    write(() => localStorage.setItem(AUTO_SEARCH_KEY, on ? 'on' : 'off'));
    window.dispatchEvent(new Event(AUTO_SEARCH_EVENT));
}

window.addEventListener('storage', (event) => {
    if (event.key === AUTO_SEARCH_KEY) {
        autoSearchValue = event.newValue !== 'off';
        window.dispatchEvent(new Event(AUTO_SEARCH_EVENT));
    }
});
