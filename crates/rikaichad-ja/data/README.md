# Ported data

Generated, not hand-edited. Regenerate from the pinned sources in
`reference-src/` (see `docs/reference/README.md`):

```sh
node scripts/port/dump-japanese-transforms.mjs reference-src/yomitan > crates/rikaichad-ja/data/japanese-transforms.json
node scripts/port/dump-ja-data.mjs reference-src/yomitan reference-src/kanji-processor-1.0.2 crates/rikaichad-ja/data
```

| File                      | Source                                                              | License           |
|---------------------------|---------------------------------------------------------------------|-------------------|
| japanese-transforms.json  | Yomitan ext/js/language/ja/japanese-transforms.js @ ee81f8dcf13e985bb4d7961009fb49a764b4d924              | GPL-3.0-or-later  |
| romaji-to-hiragana.json   | Yomitan ext/js/language/ja/japanese-kana-romaji-dicts.js @ ee81f8dcf13e985bb4d7961009fb49a764b4d924       | GPL-3.0-or-later  |
| nfkd.json                 | Yomitan ext/js/language/CJK-util.js ranges @ ee81f8dcf13e985bb4d7961009fb49a764b4d924; values from Node's String.normalize (Unicode version recorded in the file) | GPL-3.0-or-later (ranges); Unicode-3.0 (normalization data) |
| halfwidth-katakana.json   | Yomitan ext/js/language/ja/japanese.js convertHalfWidthKanaToFullWidth, evaluated for U+FF61-U+FF9F | GPL-3.0-or-later |
| prolonged-sound.json      | Yomitan ext/js/language/ja/japanese.js convertKatakanaToHiragana(kana + U+30FC), evaluated for U+3040-U+30FF | GPL-3.0-or-later |
| kanji-variants.json       | npm kanji-processor 1.0.2 dist/full_list.json, dist/itaiji_list.json | MIT (see NOTICE)  |

Copyright of the Yomitan-derived files: (C) 2023-2026 Yomitan Authors,
(C) 2016-2022 Yomichan Authors.
