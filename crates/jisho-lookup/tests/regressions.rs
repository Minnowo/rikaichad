// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lookup regressions found by differential review against Yomitan's
//! translator (run in node on the same data).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use jisho_format::Limits;
use jisho_lookup::{EnabledDictionaries, FindTermsOptions, ResultMode, find_terms};
use jisho_store::Store;
use jisho_testkit::{TempDir, fixture};

/// Yomitan shares each deinflection's rule-chain arrays between all entries
/// made from it and merges into them in place, so a merge for one entry
/// changes the chains, and with them the sort, of others. For "itte-ru"
/// Yomitan returns the score-5 entry (r2) before the score-0 one (r1).
#[test]
fn merged_chains_are_shared_between_entries_of_a_deinflection() {
    let dir = TempDir::new("regress-shared");
    let mut store = Store::open(&dir.path().join("store.sqlite")).unwrap();
    let zip = fixture::dictionary(
        &fixture::index("Shared"),
        &[(
            "term_bank_1.json",
            r#"[["いる","いる","","v1 v5",0,["r1"],0,""],["いる","いる","","v1",5,["r2"],0,""]]"#,
        )],
    );
    let path = dir.write("shared.zip", &zip);
    store
        .import(&path, &Limits::default(), &mut |_| {})
        .unwrap();
    let dictionaries = EnabledDictionaries::from_store(&store).unwrap();
    let result = find_terms(
        &store,
        "いってーる",
        &dictionaries,
        &FindTermsOptions {
            mode: ResultMode::Split,
            ..FindTermsOptions::default()
        },
    )
    .unwrap();
    let ids: Vec<i64> = result.entries.iter().map(|e| e.definitions[0].id).collect();
    assert_eq!(ids, [2, 1], "Yomitan order is r2 (id 2) then r1 (id 1)");
}
