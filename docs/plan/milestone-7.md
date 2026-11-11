# Milestone 7: conjugation tables

User request (2026-11-11): a view of all the forms of each word looked
up, after https://accentjiten.com/ (its "Show conjugation" table). A
button on the definition, not a separate page.

## Decisions

- accentjiten has no license and its tables are OJAD data (labelled
  "(OJAD)" in its page), so nothing is copied from it: the table is our
  own, generated from the headword.
- Generated in Rust (`crates/rikaichad-ja/src/conjugation.rs`) and served
  by `POST /v1/terms/forms`, so the forms can be tested against the
  ported Yomitan deinflector: every form must deinflect back to the
  dictionary form, with the exceptions listed in
  `crates/rikaichad-ja/tests/conjugation.rs` (separate tail words
  -naide/-na/-desu, aru -> nai, ii -> yoi, the honorific imperative).
- Word classes are Yomitan's condition ids from the term's `rules`
  (v1, v5, vk, vs, adj-i). vz (-zuru) is not supported. Godan
  irregulars are recognised by reading: iku/yuku, aru, tou/kou, and
  the honorific -aru verbs.
- The UI shows the "Show forms" checkbox only on headwords with a supported class (or
  a definition tagged `vs`: JMdict marks nouns taking suru only in
  definition tags) and fetches the table on first click. Rows are
  labelled in English and Japanese (-te / te-kei style), always
  both (user choice over a toggle).

## Pitch of conjugated forms (undecided)

The user first asked for each form's pitch, then for forms only, and
may come back to pitch. Research so far: the Tokyo accent of each form
follows from the dictionary form's accent by the class of the attached
ending (recessive/dominant, unaccented/preaccenting/penultimate...).
The rules with cited sources (NHK 2025, Kindaichi and Akinaga 2025) are
in Wikipedia's "Japanese pitch accent rules (verbs and adjectives)"
(revision 1383524578, CC BY-SA 4.0), sections "Conjugational verbal
forms" and "Conjugational adjectival forms", and "Japanese pitch accent
rules", section "Particles and auxiliaries" for the class definitions.
Not yet downloaded into docs/reference/.

## Review findings (2026-11-11) and what was done

| #  | Finding                                                         | Resolution                                                       |
|----|-----------------------------------------------------------------|------------------------------------------------------------------|
| 1  | kawaii treated as ii (kawayokatta)                              | ii only for ii/yoi spellings, kawaii excluded; tested            |
| 2  | kanji aru gave araranai                                         | aru detected by reading; negatives are nai; tested               |
| 3  | kana words ending in tou/kou got -ute (matoute)                 | bare kana entries match whole words only; tested                 |
| 4  | -ute list differed from Yomitan's                               | Yomitan's lists used, with the notamau te roots; tested          |
| 5  | kanji suru got suru appended                                    | shown in kana; tested                                            |
| 6  | single-kanji suru verbs (aisuru) conjugate partly like godan    | documented limitation (dictionaries tag them plain vs)           |
| 7  | yuku gave yutte                                                 | yuku's -te is itte; listed exception (found under iku)           |
| 8  | honorific compounds, gozaru imperative, -nasai/-tai cells       | gozaru imperative and honorific -nasai/-tai dropped; all tested  |
| 9  | round trip too lenient (tails stripped from every form)         | each exception tied to its cells; exact rows for edge words      |
| 10 | kureru imperative kurero; aru progressive/passive               | kure; aru rows dropped                                           |
| 11 | no potential for suru                                           | dekiru row                                                       |
| 12 | UI: no retry, stuck loading, a11y names, th scope               | retry on next click, aria-label/controls, scope, role=status     |
| 13 | API: kanji term with no reading accepted; class length unbounded | 400 for both; tested                                            |
| 14 | adj-i with a vs-tagged sense got a suru table                   | adj-i tried before vs; tested                                    |

Not changed: forms requests on the lookup page use the page signal
(results replaced by a new search just ignore a late response), and
`furigana_json` stays in lookup_json.rs, the API's JSON shapes module.

## Steps

1. [x] Forms generator and round-trip tests.
2. [x] `POST /v1/terms/forms`, API tests, docs/api.md.
3. [x] UI: "Show forms" checkbox (a button at first; user request) and table.
4. [x] Independent review; screenshot (desktop and phone, light and dark).
