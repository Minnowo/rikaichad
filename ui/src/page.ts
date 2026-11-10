// SPDX-License-Identifier: AGPL-3.0-or-later
// What a page gets from the router, and the URL scheme: #/<page>?q=<text>
// plus page-specific parameters.

export type PageName = 'lookup' | 'kanji' | 'parse';

export interface Route {
    page: PageName;
    params: URLSearchParams;
}

/** Builds the hash URL of a page. */
export function href(page: PageName, params: Record<string, string>): string {
    const query = new URLSearchParams(params).toString();
    return query === '' ? `#/${page}` : `#/${page}?${query}`;
}

/** Reads the current route from `location.hash`; unknown pages are lookup. */
export function currentRoute(): Route {
    const hash = location.hash.replace(/^#\/?/, '');
    const split = hash.indexOf('?');
    const name = split === -1 ? hash : hash.slice(0, split);
    const params = new URLSearchParams(split === -1 ? '' : hash.slice(split + 1));
    const page: PageName = name === 'kanji' || name === 'parse' ? name : 'lookup';
    return {page, params};
}

/**
 * Rewrites the current URL's parameters without a navigation (no
 * hashchange event), so typing does not re-render the page.
 */
export function replaceParams(page: PageName, params: Record<string, string>): void {
    history.replaceState(null, '', href(page, params));
}

/**
 * Runs `task` `delay` ms after the last call, aborting the previous run's
 * signal, so only the latest input's request finishes. When `page` is
 * aborted (the user left the page), a pending run is dropped and a running
 * one aborted.
 */
export function debounced(delay: number, page: AbortSignal, task: (signal: AbortSignal) => Promise<void>): () => void {
    let timer: number | undefined;
    let controller: AbortController | undefined;
    const cancel = (): void => {
        window.clearTimeout(timer);
        controller?.abort();
    };
    page.addEventListener('abort', cancel);
    return () => {
        cancel();
        if (page.aborted) {
            return;
        }
        const current = new AbortController();
        controller = current;
        timer = window.setTimeout(() => {
            task(current.signal).catch((error: unknown) => {
                if (!(error instanceof DOMException && error.name === 'AbortError')) {
                    console.error(error);
                }
            });
        }, delay);
    };
}
