// SPDX-License-Identifier: AGPL-3.0-or-later
// Types and fetch helpers for the rikaichad HTTP API (docs/api.md). The UI is
// served by the same server, so requests go to its own origin.

export type ResultMode = 'group' | 'merge' | 'term' | 'split' | 'simple';

export interface DictionaryRef {
    id: number;
    title: string;
}

export interface Tag {
    name: string;
    category: string;
    order: number;
    score: number;
    notes: string[];
    dictionaries: string[];
    redundant: boolean;
}

export interface FuriganaSegment {
    text: string;
    reading: string;
}

export interface Headword {
    term: string;
    reading: string;
    furigana: FuriganaSegment[];
    sources: {original_text: string; match_type: string; match_source: string; is_primary: boolean}[];
    tags: Tag[];
    word_classes: string[];
}

export interface Definition {
    dictionary: DictionaryRef;
    headword_indices: number[];
    id: number;
    tags: Tag[];
    glossary: unknown[];
    glossary_text: string[];
}

export interface Frequency {
    headword_index: number;
    dictionary: DictionaryRef;
    frequency: number;
    display_value: string | null;
    has_reading: boolean;
}

export type PronunciationItem =
    | {type: 'pitch_accent'; position: number | string; nasal_positions: number[]; devoice_positions: number[]; tags: Tag[]}
    | {type: 'phonetic_transcription'; ipa: string; tags: Tag[]};

export interface Pronunciation {
    headword_index: number;
    dictionary: DictionaryRef;
    items: PronunciationItem[];
}

export interface TermEntry {
    headwords: Headword[];
    definitions: Definition[];
    inflections: {source: string; rules: {name: string; description: string}[]}[];
    frequencies: Frequency[];
    pronunciations: Pronunciation[];
}

export interface LookupResponse {
    entries: TermEntry[];
    matched: {text: string; length: {utf16: number; chars: number}};
    truncated: boolean;
}

export interface KanjiStat {
    name: string;
    notes: string;
    value: string;
}

export interface KanjiEntry {
    character: string;
    dictionary: DictionaryRef;
    onyomi: string[];
    kunyomi: string[];
    tags: Tag[];
    stats: {category: string; stats: KanjiStat[]}[];
    meanings: string[];
    frequencies: {dictionary: DictionaryRef; frequency: number; display_value: string | null}[];
}

export interface KanjiResponse {
    entries: KanjiEntry[];
    truncated: boolean;
}

export interface ParseToken {
    text: string;
    segments: FuriganaSegment[];
    headwords: {term: string; reading: string}[][];
}

export interface ParseResponse {
    tokens: ParseToken[];
}

export interface DictionaryInfo {
    id: number;
    title: string;
    enabled: boolean;
    has_styles: boolean;
}

export interface Source {
    version: string;
    repository: string | null;
    commit: string;
}

/** An error response from the API, or a network failure (status 0). */
export class ApiError extends Error {
    readonly status: number;

    constructor(status: number, message: string) {
        super(message);
        this.status = status;
    }
}

export function lookupTerms(text: string, resultMode: ResultMode, signal: AbortSignal): Promise<LookupResponse> {
    return post('/v1/terms/lookup', {text, result_mode: resultMode}, signal);
}

export function lookupKanji(text: string, signal: AbortSignal): Promise<KanjiResponse> {
    return post('/v1/kanji/lookup', {text}, signal);
}

export function parseText(text: string, signal: AbortSignal): Promise<ParseResponse> {
    return post('/v1/text/parse', {text}, signal);
}

export async function listDictionaries(): Promise<DictionaryInfo[]> {
    const body = await request<{dictionaries: DictionaryInfo[]}>('/v1/dictionaries', {});
    return body.dictionaries;
}

export function getSource(): Promise<Source> {
    return request('/v1/source', {});
}

/** The dictionary's styles.css text. */
export async function getDictionaryStyles(id: number): Promise<string> {
    const response = await fetchOrThrow(`/v1/dictionaries/${id}/styles`, {});
    return response.text();
}

function post<T>(path: string, body: unknown, signal: AbortSignal): Promise<T> {
    return request(path, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify(body),
        signal,
    });
}

async function request<T>(path: string, init: RequestInit): Promise<T> {
    const response = await fetchOrThrow(path, init);
    return (await response.json()) as T;
}

async function fetchOrThrow(path: string, init: RequestInit): Promise<Response> {
    let response: Response;
    try {
        response = await fetch(path, init);
    } catch (error) {
        if (error instanceof DOMException && error.name === 'AbortError') {
            throw error;
        }
        throw new ApiError(0, 'cannot reach the rikaichad server');
    }
    if (!response.ok) {
        let message = `${response.status} ${response.statusText}`;
        try {
            const body = (await response.json()) as {error?: {message?: string}};
            message = body.error?.message ?? message;
        } catch {
            // Not the JSON error envelope; keep the status line.
        }
        throw new ApiError(response.status, message);
    }
    return response;
}
