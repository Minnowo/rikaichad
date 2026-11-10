# Yomitan test data

Copied or extracted from Yomitan @ ee81f8dcf13e985bb4d7961009fb49a764b4d924 (GPL-3.0-or-later,
(C) 2023-2026 Yomitan Authors, (C) 2016-2022 Yomichan Authors). Used as
the parity oracle for jisho's port.

| File                            | Source                                                     |
|---------------------------------|------------------------------------------------------------|
| japanese-transforms-tests.json  | test/language/japanese-transforms.test.js (tests array, extracted by scripts/port/dump-yomitan-tests.mjs) |
| translator-test-inputs.json     | test/data/translator-test-inputs.json                       |
| translator-test-results.json    | test/data/translator-test-results.json                      |
| valid-dictionary1/              | test/data/dictionaries/valid-dictionary1                     |
| furigana-golden.json            | Output of ext/js/language/ja/japanese.js distributeFurigana and distributeFuriganaInflected on generated inputs (scripts/port/dump-furigana-golden.mjs) |
