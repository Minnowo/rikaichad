// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arbitrary text through every lookup on a small dictionary (the test
//! fixture and Yomitan's test dictionary): term lookup in every result
//! mode with glossaries, kanji lookup, sentence parsing, and furigana.
#![no_main]

use std::sync::OnceLock;

use jisho_format::Limits;
use jisho_lookup::{
    EnabledDictionaries, FindTermsOptions, ResultMode, find_kanji, find_terms, load_raw_glossaries,
    parse_text,
};
use jisho_store::Store;
use jisho_testkit::{TempDir, fixture};
use libfuzzer_sys::fuzz_target;

struct State {
    _dir: TempDir,
    path: std::path::PathBuf,
}

fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(|| {
        let dir = TempDir::new("fuzz-lookup");
        let path = dir.path().join("store.sqlite");
        let mut store = Store::open(&path).expect("store");
        let zip = dir.write("a.zip", &fixture::terms_dictionary("Fixture"));
        store
            .import(&zip, &Limits::default(), &mut |_| {})
            .expect("import");
        State { _dir: dir, path }
    })
}

thread_local! {
    static STORE: Store = Store::open(&state().path).expect("store");
}

fuzz_target!(|data: &[u8]| {
    // The first byte picks the operation, so each input is one cheap call.
    let Some((&op, rest)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    if text.chars().count() > 64 {
        return;
    }
    STORE.with(|store| {
        let dictionaries = EnabledDictionaries::from_store(store).expect("dictionaries");
        let modes = [
            ResultMode::Split,
            ResultMode::Group,
            ResultMode::Merge,
            ResultMode::Term,
            ResultMode::Simple,
        ];
        match op % 8 {
            0..=4 => {
                let options = FindTermsOptions {
                    mode: modes[usize::from(op % 8)],
                    deinflect: op & 0x80 == 0,
                    primary_reading: if op & 0x40 == 0 {
                        String::new()
                    } else {
                        text.chars().take(3).collect()
                    },
                    limit: Some(4),
                    ..FindTermsOptions::default()
                };
                let mut result = find_terms(store, text, &dictionaries, &options).expect("find");
                load_raw_glossaries(store, &mut result.entries, 1 << 20).expect("glossaries");
            }
            5 => {
                find_kanji(store, text, &dictionaries, op & 0x80 == 0).expect("kanji");
            }
            6 => {
                parse_text(store, text, &dictionaries, 1 + usize::from(op >> 3) % 32)
                    .expect("parse");
            }
            _ => {
                let mut parts = text.splitn(3, '|');
                let (a, b, c) = (
                    parts.next().unwrap_or_default(),
                    parts.next().unwrap_or_default(),
                    parts.next().unwrap_or_default(),
                );
                let _ = jisho_ja::distribute_furigana(a, b);
                let _ = jisho_ja::distribute_furigana_inflected(a, b, c);
            }
        }
    });
});
