// Ported from Yomitan ext/js/display/structured-content-generator.js at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2021-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Renders a glossary's structured content as DOM with Yomitan's element
// and class names (gloss-sc-*, data-sc-*), so dictionaries' styles.css
// applies as in Yomitan. Differences: jisho stores no media, so images
// become their "[Image]" link text (a span, not Yomitan's link with image
// data attributes) with the title as tooltip; internal links
// ("?query=...") search this UI; external links get an href only if
// http(s); the language is not detected per text (the result area is
// lang="ja"), so dictionary CSS using :lang() may match differently.

import {el} from './dom.js';

type Content = string | Content[] | ElementData | null | undefined;

interface ElementData {
    tag?: unknown;
    content?: Content;
    data?: unknown;
    lang?: unknown;
    style?: unknown;
    title?: unknown;
    open?: unknown;
    colSpan?: unknown;
    rowSpan?: unknown;
    href?: unknown;
    alt?: unknown;
    description?: unknown;
}

/** Recursion bound for hostile content (the importer allows deeper JSON). */
const MAX_DEPTH = 64;

/**
 * Builds one glossary item: a string, a {type: "text"} item or a
 * {type: "structured-content"} item. Other item kinds (images, deinflection
 * references) render as their text if they have any, else nothing.
 */
export function createGlossaryItem(item: unknown, onSearch: (query: string) => void): HTMLElement {
    const node = el('span', 'gloss-content');
    if (typeof item === 'string') {
        node.textContent = item;
        return node;
    }
    if (typeof item !== 'object' || item === null) {
        return node;
    }
    const record = item as {type?: unknown; text?: unknown; content?: unknown};
    if (record.type === 'text' && typeof record.text === 'string') {
        node.textContent = record.text;
    } else if (record.type === 'structured-content') {
        node.classList.add('structured-content');
        new StructuredContentGenerator(onSearch).append(node, record.content as Content, 0);
    } else if (record.type === 'image') {
        node.appendChild(imagePlaceholder(record as ElementData));
    }
    return node;
}

class StructuredContentGenerator {
    private readonly onSearch: (query: string) => void;

    constructor(onSearch: (query: string) => void) {
        this.onSearch = onSearch;
    }

    append(container: HTMLElement, content: Content, depth: number): void {
        if (depth > MAX_DEPTH) {
            return;
        }
        if (typeof content === 'string') {
            if (content.length > 0) {
                container.appendChild(document.createTextNode(content));
            }
            return;
        }
        if (!(typeof content === 'object' && content !== null)) {
            return;
        }
        if (Array.isArray(content)) {
            for (const item of content) {
                this.append(container, item, depth + 1);
            }
            return;
        }
        const node = this.createGenericElement(content, depth);
        if (node !== null) {
            container.appendChild(node);
        }
    }

    private createGenericElement(content: ElementData, depth: number): HTMLElement | null {
        const {tag} = content;
        switch (tag) {
            case 'br':
                return this.createElement(tag, content, depth, 'simple', false, false);
            case 'ruby':
            case 'rt':
            case 'rp':
                return this.createElement(tag, content, depth, 'simple', true, false);
            case 'table':
                return el('div', 'gloss-sc-table-container', this.createElement(tag, content, depth, 'table', true, false));
            case 'thead':
            case 'tbody':
            case 'tfoot':
            case 'tr':
                return this.createElement(tag, content, depth, 'table', true, false);
            case 'th':
            case 'td':
                return this.createElement(tag, content, depth, 'table-cell', true, true);
            case 'div':
            case 'span':
            case 'ol':
            case 'ul':
            case 'li':
            case 'details':
            case 'summary':
                return this.createElement(tag, content, depth, 'simple', true, true);
            case 'img':
                return imagePlaceholder(content);
            case 'a':
                return this.createLinkElement(content, depth);
        }
        return null;
    }

    private createElement(
        tag: string,
        content: ElementData,
        depth: number,
        type: 'simple' | 'table' | 'table-cell',
        hasChildren: boolean,
        hasStyle: boolean,
    ): HTMLElement {
        const node = document.createElement(tag);
        node.className = `gloss-sc-${tag}`;
        const {data, lang} = content;
        if (typeof data === 'object' && data !== null) {
            setElementDataset(node, data as Record<string, unknown>);
        }
        if (typeof lang === 'string') {
            node.lang = lang;
        }
        if (type === 'table-cell') {
            const cell = node as HTMLTableCellElement;
            const {colSpan, rowSpan} = content;
            if (typeof colSpan === 'number') {
                cell.colSpan = colSpan;
            }
            if (typeof rowSpan === 'number') {
                cell.rowSpan = rowSpan;
            }
        }
        if (hasStyle) {
            const {style, title, open} = content;
            if (typeof style === 'object' && style !== null) {
                setElementStyle(node, style as Record<string, unknown>);
            }
            if (typeof title === 'string') {
                node.title = title;
            }
            if (typeof open === 'boolean' && open) {
                node.setAttribute('open', '');
            }
        }
        if (hasChildren) {
            this.append(node, content.content, depth + 1);
        }
        return node;
    }

    private createLinkElement(content: ElementData, depth: number): HTMLElement {
        const href = typeof content.href === 'string' ? content.href : '';
        const internal = href.startsWith('?');
        const node = document.createElement('a');
        node.className = 'gloss-link';
        node.dataset['external'] = `${!internal}`;
        const text = el('span', 'gloss-link-text');
        node.appendChild(text);
        if (typeof content.lang === 'string') {
            node.lang = content.lang;
        }
        this.append(text, content.content, depth + 1);

        if (internal) {
            const query = new URLSearchParams(href.slice(1)).get('query') ?? '';
            node.href = `#/lookup?q=${encodeURIComponent(query)}`;
            node.addEventListener('click', (event) => {
                event.preventDefault();
                this.onSearch(query);
            });
        } else {
            const icon = el('span', 'gloss-link-external-icon icon');
            icon.dataset['icon'] = 'external-link';
            node.appendChild(icon);
            if (/^https?:\/\//i.test(href)) {
                node.href = href;
                node.target = '_blank';
                node.rel = 'noreferrer noopener';
            }
        }
        return node;
    }
}

/** Yomitan's collapsed-image link text, "[Image]", with the image's title. */
function imagePlaceholder(content: ElementData): HTMLElement {
    const node = el('span', 'gloss-image-link');
    node.appendChild(el('span', 'gloss-image-link-text', 'Image'));
    const title = [content.title, content.alt, content.description].find((t) => typeof t === 'string' && t !== '');
    if (typeof title === 'string') {
        node.title = title;
    }
    return node;
}

function setElementDataset(element: HTMLElement, data: Record<string, unknown>): void {
    for (let [key, value] of Object.entries(data)) {
        // Yomitan assigns the value, which the DOM converts to a string.
        if (value === null || value === undefined) {
            continue;
        }
        if (key.length > 0) {
            key = `${key[0]?.toUpperCase() ?? ''}${key.substring(1)}`;
        }
        try {
            element.dataset[`sc${key}`] = String(value);
        } catch {
            // DOMException if the key is malformed; Yomitan skips it too.
        }
    }
}

/** Style properties Yomitan copies, by name; numbers are em for margins. */
const STRING_STYLES = [
    'fontStyle', 'fontWeight', 'fontSize', 'color', 'background', 'backgroundColor', 'verticalAlign',
    'textAlign', 'textEmphasis', 'textShadow', 'textDecorationStyle', 'textDecorationColor', 'borderColor',
    'borderStyle', 'borderRadius', 'borderWidth', 'clipPath', 'margin', 'padding', 'paddingTop', 'paddingLeft',
    'paddingRight', 'paddingBottom', 'wordBreak', 'whiteSpace', 'cursor', 'listStyleType',
] as const;
const EM_STYLES = ['marginTop', 'marginLeft', 'marginRight', 'marginBottom'] as const;

function setElementStyle(node: HTMLElement, contentStyle: Record<string, unknown>): void {
    const {style} = node;
    // The text-decoration shorthand first: it resets the style and color.
    const {textDecorationLine} = contentStyle;
    if (typeof textDecorationLine === 'string') {
        style.textDecoration = textDecorationLine;
    } else if (Array.isArray(textDecorationLine)) {
        style.textDecoration = textDecorationLine.filter((v) => typeof v === 'string').join(' ');
    }
    for (const name of STRING_STYLES) {
        const value = contentStyle[name];
        if (typeof value === 'string') {
            style[name] = value;
        }
    }
    for (const name of EM_STYLES) {
        const value = contentStyle[name];
        if (typeof value === 'number') {
            style[name] = `${value}em`;
        } else if (typeof value === 'string') {
            style[name] = value;
        }
    }
}
