// SPDX-License-Identifier: AGPL-3.0-or-later
// Applies dictionaries' styles.css to their definitions only, like
// Yomitan's addScopeToCss: each dictionary's rules are nested inside
// `[data-dictionary-id="N"]`, the attribute on its definitions. The CSS is
// parsed by the browser first and re-serialized rule by rule, so a
// stylesheet with unbalanced braces cannot close the scope and style the
// rest of the page. Network loads in it (url(), @import) are blocked by
// the page's Content-Security-Policy.
//
// As in Yomitan, scoped CSS can still use nesting selectors to reach the
// definition item itself (`&`) and its siblings (`& ~ *`); app.css
// contains paint inside each definition's body so it cannot cover the
// page.

import {type DictionaryInfo, getDictionaryStyles} from './api.js';

/**
 * Fetches the styles of the dictionaries that have them and adopts them
 * into the document. Failures are logged and skipped: a dictionary without
 * its styles still reads fine.
 */
export async function loadDictionaryStyles(dictionaries: DictionaryInfo[]): Promise<void> {
    const styled = dictionaries.filter((d) => d.has_styles);
    const results = await Promise.all(
        styled.map(async (dictionary) => {
            try {
                return scopedSheet(await getDictionaryStyles(dictionary.id), dictionary.id);
            } catch (error) {
                console.warn(`styles of dictionary ${dictionary.id} not applied`, error);
                return null;
            }
        }),
    );
    const sheets = results.filter((sheet) => sheet !== null);
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, ...sheets];
}

function scopedSheet(css: string, id: number): CSSStyleSheet {
    const parsed = new CSSStyleSheet();
    parsed.replaceSync(css);
    const rules = Array.from(parsed.cssRules, (rule) => rule.cssText).join('\n');
    const scoped = new CSSStyleSheet();
    scoped.replaceSync(`[data-dictionary-id="${id}"] {\n${rules}\n}`);
    return scoped;
}
