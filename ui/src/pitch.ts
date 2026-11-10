// SPDX-License-Identifier: AGPL-3.0-or-later
// Pitch accent as text: the reading split into morae, high morae
// overlined, and a drop marked after the mora where the pitch falls (the
// usual dictionary notation), followed by the downstep number.

import {el} from './dom.js';

/** Small kana that join the previous kana into one mora. */
const SMALL_KANA = new Set(Array.from('\u3083\u3085\u3087\u3041\u3043\u3045\u3047\u3049\u308e\u30e3\u30e5\u30e7\u30a1\u30a3\u30a5\u30a7\u30a9\u30ee'));

/**
 * Renders `reading` with the pitch of `position`: a downstep number (0 is
 * flat, heiban) or an H/L string with one letter per mora.
 */
export function pitchAccent(reading: string, position: number | string): HTMLElement {
    const morae = splitMorae(reading);
    const node = el('span', 'pitch');
    for (let i = 0; i < morae.length; i++) {
        const high = isHigh(i, position);
        const drop = high && !isHigh(i + 1, position);
        const classes = ['pitch-mora', high ? 'pitch-high' : '', drop ? 'pitch-drop' : ''].filter((c) => c !== '');
        node.appendChild(el('span', classes.join(' '), morae[i] ?? ''));
    }
    node.appendChild(el('small', 'pitch-number', `[${position}]`));
    return node;
}

/** Splits kana into morae: a small kana belongs to the kana before it. */
export function splitMorae(reading: string): string[] {
    const morae: string[] = [];
    for (const char of reading) {
        const last = morae.length - 1;
        if (SMALL_KANA.has(char) && last >= 0) {
            morae[last] += char;
        } else {
            morae.push(char);
        }
    }
    return morae;
}

/**
 * Whether mora `index` is high. Index `morae.length` is the particle after
 * the word, so a drop after the last mora (odaka) shows.
 */
function isHigh(index: number, position: number | string): boolean {
    if (typeof position === 'string') {
        return position[index]?.toUpperCase() === 'H';
    }
    switch (position) {
        case 0:
            return index > 0;
        case 1:
            return index < 1;
        default:
            return index > 0 && index < position;
    }
}
