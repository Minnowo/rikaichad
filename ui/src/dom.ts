// SPDX-License-Identifier: AGPL-3.0-or-later
// Element construction. Text is always set as text nodes, never parsed as
// HTML, so dictionary content cannot inject markup.

export type Child = Node | string | null | undefined | false;

/**
 * Creates `<tag>` with `className` (may be empty) and children; strings
 * become text nodes, null/undefined/false are skipped.
 */
export function el<K extends keyof HTMLElementTagNameMap>(
    tag: K,
    className: string,
    ...children: Child[]
): HTMLElementTagNameMap[K] {
    const node = document.createElement(tag);
    if (className !== '') {
        node.className = className;
    }
    append(node, children);
    return node;
}

/** Appends children to `node` as in [`el`]. */
export function append(node: Node, children: Child[]): void {
    for (const child of children) {
        if (child === null || child === undefined || child === false) {
            continue;
        }
        node.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
    }
}

/** A message box in a role color, e.g. for errors and empty results. */
export function message(role: 'error' | 'secondary', text: string): HTMLElement {
    const container = role === 'error' ? 'bg-c-error-container text-c-on-error-container' : 'bg-c-secondary-container text-c-on-secondary-container';
    return el('p', `rounded-md p-3 ${container}`, text);
}

/** Ruby text for furigana segments; segments without a reading stay plain. */
export function ruby(segments: {text: string; reading: string}[]): DocumentFragment {
    const fragment = document.createDocumentFragment();
    for (const {text, reading} of segments) {
        if (reading === '') {
            fragment.appendChild(document.createTextNode(text));
        } else {
            fragment.appendChild(el('ruby', '', text, el('rp', '', '('), el('rt', '', reading), el('rp', '', ')')));
        }
    }
    return fragment;
}
