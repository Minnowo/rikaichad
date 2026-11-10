// SPDX-License-Identifier: AGPL-3.0-or-later
// Extracts the data of Yomitan's Japanese deinflection tests
// (test/language/japanese-transforms.test.js, a vitest file) as JSON, for
// crates/jisho-ja/tests. Only the `const tests = [...]` array literal is
// evaluated; the vitest calls around it are not run.
//
// Usage: node scripts/port/dump-yomitan-tests.mjs <yomitan checkout> > out.json

import {readFileSync} from 'node:fs';
import {join} from 'node:path';

const root = process.argv[2];
if (!root) {
    console.error('usage: dump-yomitan-tests.mjs <yomitan checkout>');
    process.exit(2);
}
const source = readFileSync(join(root, 'test/language/japanese-transforms.test.js'), 'utf8');
const start = source.indexOf('const tests = [');
const end = source.indexOf('\n];\n', start);
if (start < 0 || end < 0) { throw new Error('tests array not found'); }
const literal = source.slice(start + 'const tests = '.length, end + 2);
const tests = new Function(`return ${literal};`)();
process.stdout.write(JSON.stringify(tests, null, 1) + '\n');
