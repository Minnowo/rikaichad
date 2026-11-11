# Parity fixtures

Inputs and Yomitan outputs for rikaichad's differential parity tests
(docs/plan/milestone-3.md step 6).

| File                               | What                                                        |
|------------------------------------|-------------------------------------------------------------|
| merge-deinflection/                | A small dictionary written for rikaichad (AGPL-3.0-or-later): one sequence with rows holding deinflection references, duplicate glossaries, two readings of one term, frequencies of every form, pitch accents with tags |
| merge-deinflection-texts.txt       | Queries                                                     |
| merge-deinflection-yomitan.jsonl   | Yomitan's results for the queries in split, group, merge, term and simple mode, in the compact form of crates/rikaichad-lookup/tests/corpus_parity.rs |

Regenerate the Yomitan output (Yomitan @ ee81f8dcf13e985bb4d7961009fb49a764b4d924,
Intl.Collator as in a browser):

    scripts/port/prepare-yomitan-node.sh <ynode>
    (cd tests/fixtures/parity/merge-deinflection && zip -X ../../../../md.zip *)
    rchad --store md.sqlite import md.zip
    RIKAICHAD_EXPORT_STORE=$PWD/md.sqlite RIKAICHAD_EXPORT_OUT=$PWD/md-rows.jsonl \
        cargo test --release -p rikaichad-store --test export_oracle -- --ignored
    node scripts/port/yomitan-oracle.mjs <ynode> md-rows.jsonl \
        tests/fixtures/parity/merge-deinflection-texts.txt split,group,merge,term,simple,parse \
        > tests/fixtures/parity/merge-deinflection-yomitan.jsonl
