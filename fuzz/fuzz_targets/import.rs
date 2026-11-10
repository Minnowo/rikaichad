// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arbitrary bytes imported end to end as a dictionary zip into a store:
//! a failed import must leave the store as it was; a successful one is
//! looked up, checked and removed again.
#![no_main]

use std::sync::{Mutex, OnceLock};

use jisho_format::Limits;
use jisho_store::Store;
use jisho_testkit::TempDir;
use libfuzzer_sys::fuzz_target;

struct State {
    dir: TempDir,
    store: Store,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        let dir = TempDir::new("fuzz-import");
        let store = Store::open(&dir.path().join("store.sqlite")).expect("store");
        Mutex::new(State { dir, store })
    })
}

fuzz_target!(|data: &[u8]| {
    let mut state = state()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = state.dir.write("input.zip", data);
    let State { store, .. } = &mut *state;
    match store.import(&path, &Limits::default(), &mut |_| {}) {
        Ok(report) => {
            let terms: Vec<String> = Vec::new();
            let _ = store.find_terms(&terms, None, 10);
            let _ = store.lookup_exact("a", 10);
            assert!(store.check_integrity().expect("integrity").is_empty());
            store.remove(report.id).expect("remove");
        }
        Err(_) => {
            assert!(
                store.dictionaries().expect("list").is_empty(),
                "failed import left data"
            );
        }
    }
});
