# AGENTS.md

Instructions for AI agents (and humans) working in this repository. The rules in
"Core rules" apply everywhere. "Project specifics" at the bottom is filled in
per project. Subdirectories may carry their own CLAUDE.md / AGENTS.md with rules
for that module; the closer file wins on conflicts.

## Core rules

### 1. ASCII only

- Write code, comments, docs, commit messages and logs in plain ASCII.
- No em/en dashes, smart quotes, ellipsis characters, arrows, bullets, emoji or
  box-drawing characters. Use `-`, `--`, `"`, `'`, `...`, `->`, `*`.
- Exceptions: string literals that must hold non-ASCII text, translation files,
  test fixtures for unicode handling, and data that is non-ASCII by nature.
  If there is no concrete reason for a non-ASCII character, do not use it.
- Before finishing, check your changes: `grep -nP '[^\x00-\x7F]' <files>`.

### 2. Keep it simple

Favour explicit control flow that a reader can follow top to bottom, but
write idiomatic Rust, not Go in Rust syntax.

- Idiomatic Rust is fine and expected: `?`, `match`, `if let`/`let else`,
  short iterator chains (`map`/`filter`/`collect`, `any`, `find`), `Option`
  and `Result` combinators where they read naturally.
- Switch to a plain `for` loop with `if` and early `return` when a chain
  gets long, nests closures, needs side effects or makes errors awkward to
  handle. Avoid clever combinator puzzles, macro or trait metaprogramming,
  and generics without a second real use.
- One way of doing a thing. No abstraction until there is a second real user
  of it. No speculative options, flags or extension points.
- Handle errors explicitly at the point they occur. Do not swallow them, and do
  not use panicking shortcuts (`unwrap`, `!`, bare `except`, etc.) outside tests
  unless failure is provably impossible.
- Treat every number that comes from input (files, network, users, agents) as
  hostile: check bounds before indexing, guard divisions, cap allocations,
  bound recursion.
- No side effects inside asserts, debug-only code or log statements. They may
  be compiled out or skipped, and the side effect goes with them.

### 3. Minimal dependencies

- Never add a dependency without asking first. This includes dev and build
  dependencies.
- If a library would only give us one or two functions, vendor that code in
  instead (with its license header and a note of where it came from), or write it.
- Prefer the standard library. Ensure license compatability, if the project license
  does not work with the library license, do not use it.


### 4. Segmentation

- Keep features in their own modules so they can be added or removed by
  touching as few places as possible: plug in, plug out.
- One concept per file. For example, one API endpoint, one command, or one
  handler per file, named after what it does. Avoid catch-all files like
  `utils`, `helpers`, `misc`, `*_commands`.
- Shared types and transport code live in their own modules, not inside a
  feature module.
- Dependencies between modules point one way. Lower layers never import higher
  ones.
- Removing a feature should mean deleting its directory and its registration
  line, not hunting through the codebase.

### 5. Fix bugs test-first

For logic bugs:

1. Write a test that reproduces the bug and fails.
2. Run it and confirm it fails for the reason you expect.
3. Fix the bug.
4. Run it and confirm it passes, along with the rest of the suite.

Skip this only where a test does not make sense (UI styling, layout, copy
changes). New features come with at least one test that would fail without them.
Never weaken, skip or delete a test or a check to make something pass.

- Prefer tests at the external interface (CLI, API, files written, protocol
  messages). They survive refactors and even rewrites; tests of private
  internals break with them.
- Check that tests actually ran. A skipped, filtered or empty test run is not
  a pass.

### 6. Read the spec

When implementing or talking to a service, file format, protocol or API, find
the authoritative reference (spec, RFC, official docs, schema) and read it
before writing code. Download it into `docs/reference/` (or note the URL and
version in the module header) so the facts are on hand. Do not guess field
names, limits, encodings or error semantics from memory or from examples.
If no spec exists, say so and record what behaviour was observed.

### 7. Comments

Code should explain itself by being explicit and simple. Comments are for what
the code cannot say.

- Every file starts with a short header comment: what this file is for.
- Public functions and types get a doc comment (javadoc / godoc / rustdoc
  style): what it does, how to call it, what it returns, and any hidden
  semantics (ownership, blocking, thread safety, units, side effects, errors).
- Inside function bodies, comment only when something is done in a
  non-obvious way or for a non-obvious reason. Say why, not what.
- No comments that restate the code, narrate changes ("now uses X", "fixed
  bug"), or mark sections for decoration.
- If a workaround needs a paragraph of comment to justify why it is OK, the
  code is wrong. Fix the code.

## File layout

Organize each file to read top to bottom, important things first:

1. Header comment, then imports.
2. Constants.
3. The primary type or function the file exists for.
4. Supporting types below it.
5. Methods: constructors, then public methods, then private helpers.
   Callers above callees.

Tests: constants and fixtures first, then test cases, then test helpers last.

## Working style

- Read before you write. Look at the surrounding code and match its naming,
  structure and idiom.
- Do what was asked. Do not refactor, rename or reformat unrelated code in the
  same change.
- Keep the build green at every step. Commit small and often; each commit is a
  working state. Commits can be squashed at the end as needed.
- Report honestly. A stub, partial implementation or untested path is reported
  as such, never as done. Say whether a number was measured or estimated.
- No stubs or placeholders to get something compiling or passing: no empty
  function bodies, hard-coded stand-in values, `todo!()`-style panics or
  disabled checks. If something cannot be finished, stop and say so.
- Prefer rules enforced by tools (types, compiler, linter, tests, check
  scripts) over rules enforced by prose. When a rule here can be checked
  automatically, add the check.
- Fix the process, not just the instance. When the same kind of mistake
  happens twice, add a rule to this file (or the module's CLAUDE.md) or a check
  that catches it.
- For large jobs (many errors, many failing tests, many files), write the work
  list to a file, try the approach on a small sample first, then work through
  the list in batches.
- For UI changes, run the app and look at the result (screenshot) before
  calling it done.
- Use logging, not print statements, outside of tests and CLI output.
- Markdown tables are space-padded so columns line up in plain text.

## Independent review

The author of a change is biased toward accepting it. For any non-trivial
change, get a review from a separate context (a subagent) before calling it
done:

- The reviewer gets the diff and the task description, not the author's
  reasoning.
- The reviewer is told to assume the code is wrong and find out why: bugs,
  edge cases, behaviour changes, rule violations from this file.
- The reviewer does not edit code. The author fixes each finding or explains
  why it does not apply.
- Only the top-level agent (the one talking to the user) spawns subagents,
  including reviewers. See "Subagents" below.

## Subagents

- A subagent never spawns another subagent. This applies to every kind:
  coding agents, reviewers, researchers, fetchers.
- A coding subagent does not start its own reviewer; it finishes, reports,
  and the top-level agent decides on and starts the review.
- A reviewer does not start a second reviewer or a "verifier" of its own
  findings; it reports, and the top-level agent decides if more review is
  needed.
- If a subagent thinks more agents are needed, it says so in its report
  instead of spawning them.
- When the top-level agent spawns a subagent, it includes this rule in the
  subagent's instructions.

## Ask a human first

Do not decide these on your own:

- adding or upgrading a dependency;
- anything that costs money or uses paid services;
- pushing, publishing, releasing or deploying;
- destructive git operations (force push, reset, deleting branches);
- deleting files that are untracked or not yours;
- changes to public APIs, wire formats, or stored data formats;
- handling personal data or credentials.

Never put personal details (names, emails) into outgoing requests, headers or
user agents.

## Parallel agents

- One git worktree per agent. Edit only the files your task owns.
- Use separate build/output directories and ports per agent.
- If someone else's in-progress change breaks the build, wait and retry; do not
  fix their files.
- Never run git commands that touch other people's work: no `git stash`,
  `git reset`, `git checkout -- .`, `git clean`, `git add -A` or `git add .`.
  Stage and commit only the specific files you changed.
- Run heavy or slow commands (full builds, stress tests) with resource limits,
  and not from many agents at once.
- Clean up your worktree, build directory and temp files when done.

## Before you finish

- [ ] Build passes, formatter and linter are clean.
- [ ] Full test suite passes, and the tests actually ran. Bug fixes have a
      regression test.
- [ ] No stubs, placeholders or disabled checks left behind.
- [ ] Non-trivial changes had an independent review, findings addressed.
- [ ] New/changed text is ASCII only (see rule 1).
- [ ] No new dependencies without approval.
- [ ] Doc comments on new public functions; file header on new files.

## Project specifics

### Overview

jisho is a Japanese dictionary engine in Rust that reads Yomitan-format
dictionary zips and reproduces Yomitan's lookup behaviour (deinflection,
scanning, ranking), exposed as a CLI and an HTTP/JSON API. Text only: no
images or audio. The full plan, decisions and milestones are in
`docs/brief.md`; read it before starting a milestone.

### Layout

```
crates/
  jisho-format/   Yomitan zip + JSON parsing/validation, hostile-input caps
  jisho-ja/       Ported Yomitan Japanese transforms and text preprocessors
  jisho-store/    SQLite store (single file), streaming import, queries
  jisho-lookup/   Ported Yomitan translator: scan, deinflect, rank, group
  jisho-api/      HTTP server (axum), one handler per file
  jisho/          Binary: one CLI command per file
docs/brief.md             plan and decisions (source of truth)
docs/dictionary-survey.md measured facts about real dictionaries
docs/reference/           downloaded specs (Yomitan schemas, API docs, licenses)
reference-src/            git-ignored clones of Yomitan, zip.js, zip crate
dictionaries/             git-ignored private user dictionaries; never commit,
                          never copy into tests
scripts/check.sh          all checks; must pass before every commit
```

Layering (enforced by crate dependencies): jisho-format and jisho-ja have
no internal deps; jisho-store -> jisho-format; jisho-lookup -> jisho-ja,
jisho-store; jisho-api -> jisho-lookup; jisho -> everything.

### Commands

```
check:   scripts/check.sh
build:   cargo build --workspace
test:    cargo test --workspace
lint:    cargo clippy --workspace --all-targets -- -D warnings
run:     cargo run -p jisho -- <command>
```

Run `scripts/check.sh` on its own and act on its exit status. Never pipe
it into `grep`/`tail` in the same command as a commit: the pipe hides the
failure (this let a failing check and a stray `__pycache__` file into a
commit once).

### Language notes

- Rust edition 2024. Errors: one error enum per crate, implementing
  `std::error::Error`; no `anyhow`/`thiserror` (no new deps without asking).
- `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` and slice
  indexing are denied by clippy outside tests (workspace lints).
- Logging: a small stderr logger module in the `jisho` binary (added with
  the first log call; no logging crate); library
  crates return errors instead of logging.
- Porting from Yomitan: read the pinned source in `reference-src/yomitan`
  (commit in `docs/reference/README.md`). A ported file starts with the
  header below and is listed in `NOTICE`. Keep Yomitan's names for
  functions and rules so a port can be diffed against upstream. Where
  Yomitan indexes strings by UTF-16 code units, keep that behaviour.

```
// Ported from Yomitan <upstream path> at <commit>.
// <every Copyright line of the upstream file, copied exactly>
// SPDX-License-Identifier: GPL-3.0-or-later
```

- Own files carry `// SPDX-License-Identifier: AGPL-3.0-or-later` as the
  first line of their header comment.
- Dictionary zips are untrusted input: no `ZipArchive::extract`, read
  only root-level exact names, cap sizes, accept CRC mismatches with a
  warning (see docs/dictionary-survey.md).
