// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conjugated forms of a verb or i-adjective, for showing a word's
//! whole conjugation table. This is our own table, not a Yomitan port:
//! Yomitan only deinflects. The tests check that lookup (the ported
//! transformer) takes each form back to its dictionary form, apart from
//! the exceptions listed there, and pin exact forms for irregular words.
//!
//! Supported word classes (Yomitan condition ids from a term's `rules`):
//! v1, v5, vk (kuru), adj-i, and vs (suru, and nouns that take suru).
//! Irregular words: iku, aru, the -u verbs with -ute (tou, kou, ...), the
//! honorific -aru verbs (irassharu, ...), kureru, kuru, suru and ii.
//! Not handled: single-kanji suru verbs that conjugate partly like godan
//! (aisuru -> aisanai), since dictionaries tag them plain vs; they get
//! the regular suru table.

use crate::kana::is_string_entirely_kana;

/// Form names in table order.
pub const FORM_NAMES: &[&str] = &[
    "non-past",
    "past",
    "te",
    "adverbial",
    "conditional",
    "tara",
    "volitional",
    "imperative",
    "desire",
    "progressive",
    "potential",
    "passive",
    "causative",
    "causative-passive",
    "noun",
];

/// Longest term or reading accepted, in characters.
pub const MAX_CONJUGATION_INPUT: usize = 64;

/// Word classes in the order they are tried.
const WORD_CLASSES: &[&str] = &["v1", "v5", "vk", "adj-i", "vs"];

/// Rows of the godan table: dictionary-form ending, then the a, i, e and
/// o rows and the -te ending.
const GODAN_ROWS: &[[&str; 6]] = &[
    ["う", "わ", "い", "え", "お", "って"],
    ["く", "か", "き", "け", "こ", "いて"],
    ["ぐ", "が", "ぎ", "げ", "ご", "いで"],
    ["す", "さ", "し", "せ", "そ", "して"],
    ["つ", "た", "ち", "て", "と", "って"],
    ["ぬ", "な", "に", "ね", "の", "んで"],
    ["ぶ", "ば", "び", "べ", "ぼ", "んで"],
    ["む", "ま", "み", "め", "も", "んで"],
    ["る", "ら", "り", "れ", "ろ", "って"],
];

// The irregular-verb lists below are the ones Yomitan deinflects
// (ikuVerbs, godanUSpecialVerbs, fuVerbTeConjugations and
// specialHonorificMasuVerbs in japanese-transforms.js), so lookup
// recognises what the table shows.

/// Verbs whose -te form is -tte (itte). yuku is added (not in Yomitan's
/// list): it shares iku's -te form, which lookup finds under iku.
const IKU_VERBS: &[&str] = &["いく", "行く", "逝く", "往く", "ゆく"];

/// Godan -u verbs whose -te form is -ute (toute).
const UTE_VERBS: &[&str] = &[
    "こう",
    "とう",
    "請う",
    "乞う",
    "恋う",
    "問う",
    "訪う",
    "宣う",
    "曰う",
    "給う",
    "賜う",
    "揺蕩う",
];

/// -u verbs whose -te root changes vowel (notamau -> notamoute), by reading.
const FU_VERB_TE_ROOTS: &[(&str, &str)] = &[
    ("のたまう", "のたもう"),
    ("たまう", "たもう"),
    ("たゆたう", "たゆとう"),
];

/// Honorific verbs whose -masu stem and imperative end in -i.
const HONORIFIC_VERBS: &[&str] = &[
    "いらっしゃる",
    "ござる",
    "なさる",
    "くださる",
    "下さる",
    "おっしゃる",
    "仰る",
    "仰有る",
];

/// One conjugated form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    /// What the form is: one of [`FORM_NAMES`].
    pub name: &'static str,
    /// The polite (-masu, -desu) variant.
    pub polite: bool,
    /// The negative variant.
    pub negative: bool,
    /// The form as the term is written (kanji kept).
    pub term: String,
    /// The form in kana.
    pub reading: String,
}

/// A word's conjugation table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conjugation {
    /// The word class the table was built for, e.g. "v5".
    pub word_class: &'static str,
    /// The dictionary form (for a noun taking suru, the noun plus suru).
    pub term: String,
    /// The dictionary form in kana.
    pub reading: String,
    /// Forms in table order: by name in [`FORM_NAMES`] order, then plain
    /// before polite, affirmative before negative. A cell may hold more
    /// than one form (e.g. -nakute and -naide), and some words lack some
    /// cells (aru has no progressive, suru's potential is dekiru).
    pub forms: Vec<Form>,
}

/// The conjugation table of `term` read as `reading` (an empty reading
/// means the term is kana), using the first supported class in
/// `word_classes`. Returns `None` if no class is supported, the input is
/// empty or longer than [`MAX_CONJUGATION_INPUT`], or the word does not
/// end the way its class needs (e.g. a v5 verb not ending in u-row kana).
pub fn conjugate<S: AsRef<str>>(
    term: &str,
    reading: &str,
    word_classes: &[S],
) -> Option<Conjugation> {
    let reading = if reading.is_empty() { term } else { reading };
    if term.is_empty()
        || term.chars().count() > MAX_CONJUGATION_INPUT
        || reading.chars().count() > MAX_CONJUGATION_INPUT
    {
        return None;
    }
    let word = Word {
        term: term.to_string(),
        reading: reading.to_string(),
    };
    for &class in WORD_CLASSES {
        if !word_classes.iter().any(|c| c.as_ref() == class) {
            continue;
        }
        let table = match class {
            "v1" => ichidan(&word),
            "v5" => godan(&word),
            "vk" => kuru(&word),
            "adj-i" => adjective(&word),
            _ => suru(&word),
        };
        if let Some((dictionary, forms)) = table {
            return Some(Conjugation {
                word_class: class,
                term: dictionary.term,
                reading: dictionary.reading,
                forms,
            });
        }
    }
    None
}

/// A written form and its kana, built up in parallel.
#[derive(Debug, Clone)]
struct Word {
    term: String,
    reading: String,
}

impl Word {
    /// Removes `ending` from both the term and the reading; `None` if
    /// either does not end with it.
    fn strip(&self, ending: &str) -> Option<Word> {
        self.strip_pair(ending, ending)
    }

    /// Removes `term_ending` from the term and `reading_ending` from the
    /// reading (they differ when the ending is written in kanji).
    fn strip_pair(&self, term_ending: &str, reading_ending: &str) -> Option<Word> {
        Some(Word {
            term: self.term.strip_suffix(term_ending)?.to_string(),
            reading: self.reading.strip_suffix(reading_ending)?.to_string(),
        })
    }

    /// This word with `suffix` (kana) appended to both.
    fn plus(&self, suffix: &str) -> Word {
        self.plus_pair(suffix, suffix)
    }

    /// This word with `term_suffix` appended to the term and
    /// `reading_suffix` to the reading.
    fn plus_pair(&self, term_suffix: &str, reading_suffix: &str) -> Word {
        Word {
            term: format!("{}{term_suffix}", self.term),
            reading: format!("{}{reading_suffix}", self.reading),
        }
    }
}

/// The stems a verb's forms are built from.
struct VerbStems {
    dictionary: Word,
    /// Irrealis stem, before -nai.
    negative: Word,
    /// Stem before -masu: the infinitive, except for the honorific verbs
    /// (irasshai-masu but irasshari-tai).
    masu: Word,
    te: Word,
    ta: Word,
    /// Before -ba.
    conditional: Word,
    volitional: Word,
    /// Plain imperatives.
    imperatives: Vec<Word>,
    /// The polite imperative (-nasai), if the verb has one.
    nasai: Option<Word>,
    /// The -tai stem, if the verb has the desire forms.
    desire: Option<Word>,
    /// The plain negative (-nai) when it is not stem + nai: aru -> nai.
    nai: Option<Word>,
    /// Ichidan verbs derived from this one, in dictionary form; None for
    /// verbs without that form.
    progressive: Option<Word>,
    potential: Option<Word>,
    passive: Option<Word>,
    causative: Option<Word>,
    causative_passive: Option<Word>,
}

type Table = (Word, Vec<Form>);

fn ichidan(word: &Word) -> Option<Table> {
    let stem = word.strip("る")?;
    // kureru's imperative is kure.
    let kureru = word.reading.ends_with("くれる")
        && (word.term.ends_with("くれる") || word.term.ends_with("呉れる"));
    let te = stem.plus("て");
    let stems = VerbStems {
        dictionary: word.clone(),
        negative: stem.clone(),
        masu: stem.clone(),
        ta: stem.plus("た"),
        conditional: stem.plus("れ"),
        volitional: stem.plus("よう"),
        imperatives: vec![if kureru {
            stem.clone()
        } else {
            stem.plus("ろ")
        }],
        nasai: Some(stem.clone()),
        desire: Some(stem.clone()),
        nai: None,
        progressive: Some(te.plus("いる")),
        potential: Some(stem.plus("られる")),
        passive: Some(stem.plus("られる")),
        causative: Some(stem.plus("させる")),
        causative_passive: Some(stem.plus("させられる")),
        te,
    };
    Some((word.clone(), verb_forms(&stems)))
}

fn godan(word: &Word) -> Option<Table> {
    let row = GODAN_ROWS
        .iter()
        .find(|row| word.term.ends_with(row[0]) && word.reading.ends_with(row[0]))?;
    let [ending, a, i, e, o, te_ending] = *row;
    let stem = word.strip(ending)?;
    let honorific =
        HONORIFIC_VERBS.iter().any(|h| word.term.ends_with(h)) && word.reading.ends_with('る');
    let aru = word.reading == "ある";
    let te_root = godan_te_root(word, &stem, te_ending);
    let voiced = te_ending.ends_with('で');
    let te = te_root.plus(if voiced { "で" } else { "て" });
    let ta = te_root.plus(if voiced { "だ" } else { "た" });
    let infinitive = stem.plus(i);
    let masu = if honorific {
        stem.plus("い")
    } else {
        infinitive.clone()
    };
    // gozaru has no imperative in use.
    let imperatives = if !honorific {
        vec![stem.plus(e)]
    } else if word.reading.ends_with("ござる") {
        Vec::new()
    } else {
        vec![stem.plus("い")]
    };
    let derived = |w: Word| (!aru).then_some(w);
    let stems = VerbStems {
        dictionary: word.clone(),
        negative: stem.plus(a),
        masu,
        ta,
        conditional: stem.plus(e),
        volitional: stem.plus(o).plus("う"),
        imperatives,
        nasai: (!honorific).then(|| infinitive.clone()),
        desire: (!honorific).then(|| infinitive.clone()),
        nai: aru.then(|| Word {
            term: "ない".to_string(),
            reading: "ない".to_string(),
        }),
        progressive: derived(te.plus("いる")),
        potential: derived(stem.plus(e).plus("る")),
        passive: derived(stem.plus(a).plus("れる")),
        causative: derived(stem.plus(a).plus("せる")),
        causative_passive: derived(stem.plus(a).plus("せられる")),
        te,
    };
    Some((word.clone(), verb_forms(&stems)))
}

/// What comes before -te/-ta in a godan verb: the regular -tte/-ite/...
/// root (`te_ending` without its final te/de), or iku's itt-, or the
/// -u verbs' -u (toute, notamoute).
fn godan_te_root(word: &Word, stem: &Word, te_ending: &str) -> Word {
    if IKU_VERBS.iter().any(|v| word.term.ends_with(v)) {
        // yuku's -te form is itte, as iku's.
        let reading = match word.reading.strip_suffix("ゆく") {
            Some(prefix) => format!("{prefix}いっ"),
            None => format!("{}っ", stem.reading),
        };
        return Word {
            term: format!("{}っ", stem.term),
            reading,
        };
    }
    let ute = UTE_VERBS.iter().any(|v| {
        if is_string_entirely_kana(v) {
            word.term == *v
        } else {
            word.term.ends_with(v)
        }
    });
    let fu_root = FU_VERB_TE_ROOTS.iter().find_map(|(verb, root)| {
        let prefix = word.reading.strip_suffix(verb)?;
        (prefix.is_empty() || !is_string_entirely_kana(&word.term))
            .then(|| format!("{prefix}{root}"))
    });
    if ute || fu_root.is_some() {
        return Word {
            term: word.term.clone(),
            reading: fu_root.unwrap_or_else(|| word.reading.clone()),
        };
    }
    let root = te_ending
        .strip_suffix('て')
        .or_else(|| te_ending.strip_suffix('で'))
        .unwrap_or(te_ending);
    stem.plus(root)
}

fn kuru(word: &Word) -> Option<Table> {
    // Written with the kanji for kuru (kanji kept, kana changes) or in kana.
    let (stem, kanji) = match word.strip_pair("来る", "くる") {
        Some(stem) => (stem, true),
        None => (word.strip("くる")?, false),
    };
    let form = |kana: &str, rest: &str| -> Word {
        if kanji {
            stem.plus_pair(&format!("来{rest}"), &format!("{kana}{rest}"))
        } else {
            stem.plus(&format!("{kana}{rest}"))
        }
    };
    let te = form("き", "て");
    let stems = VerbStems {
        dictionary: word.clone(),
        negative: form("こ", ""),
        masu: form("き", ""),
        ta: form("き", "た"),
        conditional: form("く", "れ"),
        volitional: form("こ", "よう"),
        imperatives: vec![form("こ", "い")],
        nasai: Some(form("き", "")),
        desire: Some(form("き", "")),
        nai: None,
        progressive: Some(te.plus("いる")),
        potential: Some(form("こ", "られる")),
        passive: Some(form("こ", "られる")),
        causative: Some(form("こ", "させる")),
        causative_passive: Some(form("こ", "させられる")),
        te,
    };
    Some((word.clone(), verb_forms(&stems)))
}

fn suru(word: &Word) -> Option<Table> {
    // suru written in kanji is shown in kana (its kanji reads differently in each form); a
    // noun taking suru (benkyou) gets suru appended.
    let dictionary = if let Some(prefix) = word.term.strip_suffix("為る") {
        word.reading.strip_suffix("する")?;
        Word {
            term: format!("{prefix}する"),
            reading: word.reading.clone(),
        }
    } else if word.term.ends_with("する") && word.reading.ends_with("する") {
        word.clone()
    } else {
        word.plus("する")
    };
    let stem = dictionary.strip("する")?;
    let te = stem.plus("して");
    let stems = VerbStems {
        dictionary: dictionary.clone(),
        negative: stem.plus("し"),
        masu: stem.plus("し"),
        ta: stem.plus("した"),
        conditional: stem.plus("すれ"),
        volitional: stem.plus("しよう"),
        imperatives: vec![stem.plus("しろ"), stem.plus("せよ")],
        nasai: Some(stem.plus("し")),
        desire: Some(stem.plus("し")),
        nai: None,
        progressive: Some(te.plus("いる")),
        // The potential of suru is the verb dekiru.
        potential: Some(stem.plus("できる")),
        passive: Some(stem.plus("される")),
        causative: Some(stem.plus("させる")),
        causative_passive: Some(stem.plus("させられる")),
        te,
    };
    Some((dictionary, verb_forms(&stems)))
}

fn adjective(word: &Word) -> Option<Table> {
    // ii conjugates on yo- (yoku, yokatta); its dictionary form stays ii.
    // kawaii only looks like it.
    let ii = word.reading.ends_with("いい")
        && !word.reading.ends_with("かわいい")
        && ["いい", "良い", "好い", "善い", "佳い"]
            .iter()
            .any(|t| word.term.ends_with(t));
    let stem = if ii {
        let reading_stem = word.reading.strip_suffix("いい")?;
        let term_stem = match word.term.strip_suffix("いい") {
            Some(term_stem) => format!("{term_stem}よ"),
            None => word.term.strip_suffix('い')?.to_string(),
        };
        Word {
            term: term_stem,
            reading: format!("{reading_stem}よ"),
        }
    } else {
        word.strip("い")?
    };
    let mut forms = Vec::new();
    let mut add = |name: &'static str, polite: bool, negative: bool, w: Word| {
        forms.push(form(name, polite, negative, w));
    };
    add("non-past", false, false, word.clone());
    add("non-past", false, true, stem.plus("くない"));
    add("non-past", true, false, word.plus("です"));
    add("non-past", true, true, stem.plus("くないです"));
    add("non-past", true, true, stem.plus("くありません"));
    add("past", false, false, stem.plus("かった"));
    add("past", false, true, stem.plus("くなかった"));
    add("past", true, false, stem.plus("かったです"));
    add("past", true, true, stem.plus("くなかったです"));
    add("past", true, true, stem.plus("くありませんでした"));
    add("te", false, false, stem.plus("くて"));
    add("te", false, true, stem.plus("くなくて"));
    add("adverbial", false, false, stem.plus("く"));
    add("adverbial", false, true, stem.plus("くなく"));
    add("conditional", false, false, stem.plus("ければ"));
    add("conditional", false, true, stem.plus("くなければ"));
    add("tara", false, false, stem.plus("かったら"));
    add("tara", false, true, stem.plus("くなかったら"));
    add("noun", false, false, stem.plus("さ"));
    Some((word.clone(), forms))
}

/// The table of a verb from its stems.
fn verb_forms(s: &VerbStems) -> Vec<Form> {
    let mut forms = Vec::new();
    // Negative forms: stem + na-; aru's are the adjective nai's.
    let negative = |suffix: &str| -> Word {
        match &s.nai {
            Some(nai) => nai
                .strip("い")
                .map_or_else(|| nai.clone(), |stem| stem.plus(suffix)),
            None => s.negative.plus(&format!("な{suffix}")),
        }
    };
    let mut add = |name: &'static str, polite: bool, negative: bool, w: Word| {
        forms.push(form(name, polite, negative, w));
    };
    add("non-past", false, false, s.dictionary.clone());
    add("non-past", false, true, negative("い"));
    add("non-past", true, false, s.masu.plus("ます"));
    add("non-past", true, true, s.masu.plus("ません"));
    add("past", false, false, s.ta.clone());
    add("past", false, true, negative("かった"));
    add("past", true, false, s.masu.plus("ました"));
    add("past", true, true, s.masu.plus("ませんでした"));
    add("te", false, false, s.te.clone());
    add("te", false, true, negative("くて"));
    add("te", false, true, negative("いで"));
    add("conditional", false, false, s.conditional.plus("ば"));
    add("conditional", false, true, negative("ければ"));
    add("tara", false, false, s.ta.plus("ら"));
    add("tara", false, true, negative("かったら"));
    add("tara", true, false, s.masu.plus("ましたら"));
    add("volitional", false, false, s.volitional.clone());
    add("volitional", true, false, s.masu.plus("ましょう"));
    for imperative in &s.imperatives {
        add("imperative", false, false, imperative.clone());
    }
    add("imperative", false, true, s.dictionary.plus("な"));
    if let Some(nasai) = &s.nasai {
        add("imperative", true, false, nasai.plus("なさい"));
    }
    if let Some(desire) = &s.desire {
        add("desire", false, false, desire.plus("たい"));
        add("desire", false, true, desire.plus("たくない"));
        add("desire", true, false, desire.plus("たいです"));
        add("desire", true, true, desire.plus("たくないです"));
    }
    let derived = [
        ("progressive", &s.progressive),
        ("potential", &s.potential),
        ("passive", &s.passive),
        ("causative", &s.causative),
        ("causative-passive", &s.causative_passive),
    ];
    for (name, verb) in derived {
        let Some(verb) = verb else {
            continue;
        };
        let Some(stem) = verb.strip("る") else {
            continue;
        };
        add(name, false, false, verb.clone());
        add(name, false, true, stem.plus("ない"));
        add(name, true, false, stem.plus("ます"));
        add(name, true, true, stem.plus("ません"));
    }
    forms
}

fn form(name: &'static str, polite: bool, negative: bool, word: Word) -> Form {
    Form {
        name,
        polite,
        negative,
        term: word.term,
        reading: word.reading,
    }
}
