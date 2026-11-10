// SPDX-License-Identifier: AGPL-3.0-or-later
// Builds the web UI into dist/ (committed; embedded into the jisho binary
// by crates/jisho-api/build.rs): TypeScript to ES modules with tsc, the
// stylesheet with Tailwind, and index.html copied. Output is deterministic
// so scripts/check.sh can compare it with the committed copy.
//
// Usage: npm run build [-- --out DIR]   (default DIR: dist)

import {spawnSync} from 'node:child_process';
import {copyFileSync, mkdirSync, rmSync} from 'node:fs';
import {dirname, isAbsolute, join, relative, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const out = outputDirectory();

rmSync(out, {recursive: true, force: true});
mkdirSync(out, {recursive: true});
run('tsc', ['-p', join(root, 'tsconfig.json'), '--outDir', out]);
run('tailwindcss', ['-i', join(root, 'src', 'app.css'), '-o', join(out, 'app.css'), '--minify']);
copyFileSync(join(root, 'src', 'index.html'), join(out, 'index.html'));

// The output directory is deleted first, so it must be a directory of its
// own: not missing or empty, not ui/ or a directory containing it, and
// inside ui/ only dist.
function outputDirectory() {
    const flag = process.argv.indexOf('--out');
    if (flag === -1) {
        return join(root, 'dist');
    }
    const value = process.argv[flag + 1] ?? '';
    const out = resolve(value);
    const contains = (outer, inner) => {
        const rel = relative(outer, inner);
        return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel));
    };
    const unsafe = contains(out, root) || (contains(root, out) && out !== join(root, 'dist'));
    if (value === '' || unsafe) {
        console.error(`build: refusing --out ${JSON.stringify(value)}: it would delete the UI sources`);
        process.exit(1);
    }
    return out;
}

function run(tool, args) {
    const bin = join(root, 'node_modules', '.bin', process.platform === 'win32' ? `${tool}.cmd` : tool);
    const result = spawnSync(bin, args, {cwd: root, stdio: 'inherit', shell: process.platform === 'win32'});
    if (result.status !== 0) {
        console.error(`build: ${tool} failed`);
        process.exit(1);
    }
}
