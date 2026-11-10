// SPDX-License-Identifier: AGPL-3.0-or-later
// Types and fetch helpers for the jisho HTTP API (docs/api.md). The UI is
// served by the same server, so requests go to its own origin.
/** An error response from the API, or a network failure (status 0). */
export class ApiError extends Error {
    status;
    constructor(status, message) {
        super(message);
        this.status = status;
    }
}
export function lookupTerms(text, resultMode, signal) {
    return post('/v1/terms/lookup', { text, result_mode: resultMode }, signal);
}
export function lookupKanji(text, signal) {
    return post('/v1/kanji/lookup', { text }, signal);
}
export function parseText(text, signal) {
    return post('/v1/text/parse', { text }, signal);
}
export async function listDictionaries() {
    const body = await request('/v1/dictionaries', {});
    return body.dictionaries;
}
export function getSource() {
    return request('/v1/source', {});
}
/** The dictionary's styles.css text. */
export async function getDictionaryStyles(id) {
    const response = await fetchOrThrow(`/v1/dictionaries/${id}/styles`, {});
    return response.text();
}
function post(path, body, signal) {
    return request(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
        signal,
    });
}
async function request(path, init) {
    const response = await fetchOrThrow(path, init);
    return (await response.json());
}
async function fetchOrThrow(path, init) {
    let response;
    try {
        response = await fetch(path, init);
    }
    catch (error) {
        if (error instanceof DOMException && error.name === 'AbortError') {
            throw error;
        }
        throw new ApiError(0, 'cannot reach the jisho server');
    }
    if (!response.ok) {
        let message = `${response.status} ${response.statusText}`;
        try {
            const body = (await response.json());
            message = body.error?.message ?? message;
        }
        catch {
            // Not the JSON error envelope; keep the status line.
        }
        throw new ApiError(response.status, message);
    }
    return response;
}
