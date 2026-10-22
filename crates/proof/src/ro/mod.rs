//! Romanian proofing using the installed Hunspell `ro_RO` dictionary (never bundled; Debian and
//! Ubuntu ship it as `hunspell-ro`, LibreOffice as its `dict-ro` extension).
//!
//! - Spelling reads the dictionary. A word typed with the cedilla letters `ş`/`ţ` (old keyboards,
//!   text pasted from old documents) is flagged when the dictionary spells it with the comma-below
//!   `ș`/`ț` of the current norm, and that form is suggested first ([`cedilla_fix`]). An old
//!   dictionary written with cedillas still accepts the comma-below spelling.
//! - Hyphenated forms (`într-o`, `dă-mi`, `s-a`, `ADN-ul`) are correct when the dictionary has
//!   them whole, or has each part, a part being an elided form or a clitic ([`CLITICS`]).
//! - Suggestions use the dictionary's own MAP/REP/TRY/KEY tables with the ranking Czech uses,
//!   led by the comma-below form and the post-1993 `sunt` forms of `sînt`.
//! - Grammar is conservative ([`grammar`]): repeated words, spacing, sentence capitals and
//!   doubled punctuation, with Romanian messages.
//! - Hyphenation keeps the English fallback (no Romanian patterns are bundled).

pub mod grammar;
mod suggest;
#[cfg(test)]
pub(crate) mod test_dict;

use std::sync::{Arc, RwLock};

use crate::affix::AffixDict;

pub use suggest::suggest;

/// The Romanian dictionary: `None` until first use, then the installed one (or an empty one).
static DICT: RwLock<Option<Arc<AffixDict>>> = RwLock::new(None);

/// File names of a Romanian Hunspell dictionary.
const NAMES: &[&str] = &["ro_RO", "ro-RO", "ro", "ro_MD"];

/// Parts a hyphen joins to a word without being dictionary words of their own: the elided
/// prepositions (`într-o`, `dintr-un`, `printr-o`), the pronoun and auxiliary clitics (`dă-mi`,
/// `ți-am`, `le-a`, `v-ați`, `nu-și`, `să-l`), and the article and plural endings written after
/// an acronym (`ADN-ul`, `SMS-uri`). Compared in lower case.
const CLITICS: &[&str] = &[
    "într", "dintr", "printr", "mi", "ți", "și", "i", "îi", "îl", "îmi", "îți", "își", "le", "li", "ne", "ni", "vă", "te", "se", "o", "un", "a",
    "am", "ai", "au", "ar", "aș", "ați", "ul", "ului", "uri", "urilor", "lui", "ei", "lor", "lea",
];

/// The Romanian dictionary: the installed Hunspell `ro_RO` (read on first use), one given with
/// [`install_dictionary`], or an empty one when none is available (Romanian spelling is off
/// then; see [`has_dictionary`]).
pub fn dictionary() -> Arc<AffixDict> {
    if let Some(d) = DICT.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return d.clone();
    }
    let loaded = Arc::new(load_installed());
    let mut slot = DICT.write().unwrap_or_else(|e| e.into_inner());
    // Another thread (or an install) may have filled it meanwhile: that one wins.
    slot.get_or_insert(loaded).clone()
}

fn load_installed() -> AffixDict {
    let Some((aff, dic)) = crate::hunspell::find(NAMES) else {
        crate::hunspell::clear_load_error(NAMES[0]);
        log::info!("Romanian spelling: no Hunspell ro_RO dictionary installed");
        return AffixDict::default();
    };
    crate::hunspell::note_load(NAMES[0], &dic, crate::hunspell::load(&aff, &dic)).unwrap_or_else(|e| {
        log::warn!("Romanian spelling: {e}");
        AffixDict::default()
    })
}

/// Use this Romanian dictionary from now on (Hunspell `.aff` and `.dic` text, decoded to
/// UTF-8), for example one the user picked. On error the current dictionary stays.
pub fn install_dictionary(aff: &str, dic: &str) -> Result<(), String> {
    let d = AffixDict::from_hunspell(aff, dic, &[])?;
    *DICT.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(d));
    Ok(())
}

/// Forget the loaded dictionary, so the next check looks for an installed one again (after
/// [`crate::hunspell::set_dictionary_dir`] or a dictionary was installed).
pub fn reload_dictionary() {
    *DICT.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The `.dic` file of the Hunspell `ro_RO` dictionary found in the dictionaries folder or the
/// system locations now (File › Options › Proofing shows it); `None` when there is none. A
/// dictionary given with [`install_dictionary`] has no file and isn't reported.
pub fn installed_path() -> Option<std::path::PathBuf> {
    crate::hunspell::find(NAMES).map(|(_, dic)| dic)
}

/// Is a Romanian dictionary available (spelling is checked)?
pub fn has_dictionary() -> bool {
    !dictionary().is_empty()
}

/// Is `c` a letter of the Latin script (the only one the Romanian dictionary covers)?
fn is_latin_letter(c: char) -> bool {
    c.is_alphabetic() && (c < '\u{0250}' || ('\u{1E00}'..='\u{1EFF}').contains(&c))
}

/// Does `w` have a cedilla `ş`/`ţ` (U+015F, U+0163 and their capitals)?
fn has_cedilla(w: &str) -> bool {
    w.contains(['ş', 'ţ', 'Ş', 'Ţ'])
}

/// `w` with the cedilla letters replaced by the comma-below ones of the current norm.
fn comma_form(w: &str) -> String {
    w.chars()
        .map(|c| match c {
            'ş' => 'ș',
            'ţ' => 'ț',
            'Ş' => 'Ș',
            'Ţ' => 'Ț',
            c => c,
        })
        .collect()
}

/// `w` with the comma-below letters replaced by cedillas (how old dictionaries spell them).
fn cedilla_form(w: &str) -> String {
    w.chars()
        .map(|c| match c {
            'ș' => 'ş',
            'ț' => 'ţ',
            'Ș' => 'Ş',
            'Ț' => 'Ţ',
            c => c,
        })
        .collect()
}

/// The comma-below spelling of a word typed with cedillas, when that spelling is correct
/// (`şcoală` → `școală`); `None` for a word without cedillas or one wrong for another reason.
/// The spelling check names the cedilla as the mistake then, and suggestions offer this first.
pub fn cedilla_fix(word: &str) -> Option<String> {
    cedilla_fix_with(&dictionary(), word)
}

/// [`cedilla_fix`] against a given dictionary.
pub fn cedilla_fix_with(d: &AffixDict, word: &str) -> Option<String> {
    if d.is_empty() || !has_cedilla(word) || is_correct_with(d, word) {
        return None;
    }
    let comma = comma_form(word);
    is_correct_with(d, &comma).then_some(comma)
}

/// Is `word` spelled correctly in Romanian? Numbers, single letters, short words in capitals
/// (acronyms: `ONU`, `ANAF`), words with digits and words in other scripts are accepted, and so
/// is every word when no Romanian dictionary is installed. A hyphenated word is correct when the
/// dictionary has it whole (`într-o`, `dă-mi`) or has each part, an elided form or clitic
/// counting as known (`spune-mi`, `ADN-ul`).
pub fn is_correct(word: &str) -> bool {
    is_correct_with(&dictionary(), word)
}

/// [`is_correct`] with the File › Options › Proofing choices: with Ignore words in UPPERCASE off,
/// short words in capitals are checked against the dictionary too, and with Ignore words that
/// contain numbers off, so are words with digits (a part made of digits alone stays correct:
/// `al 2-lea`).
pub fn is_correct_with_options(word: &str, opts: &crate::Options) -> bool {
    check(&dictionary(), word, opts)
}

/// [`is_correct`] against a given dictionary (an empty one accepts everything).
pub fn is_correct_with(d: &AffixDict, word: &str) -> bool {
    check(d, word, &crate::Options::default())
}

/// [`is_correct_with_options`] against a given dictionary.
fn check(d: &AffixDict, word: &str, opts: &crate::Options) -> bool {
    if d.is_empty() {
        return true; // no Romanian dictionary installed: no spelling marks
    }
    let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’');
    let w = w.trim_matches(['\'', '’']);
    let n = w.chars().count();
    if n <= 1 || (opts.ignore_numbers && w.chars().any(|c| c.is_ascii_digit())) || !w.chars().any(char::is_alphabetic) {
        return true;
    }
    // A cedilla is wrong wherever the comma-below letter makes the word right, acronyms
    // included, unless the dictionary (an old one) or the user spells it so.
    if has_cedilla(w) && !known(d, w) && check(d, &comma_form(w), opts) {
        return false;
    }
    if opts.ignore_uppercase && is_acronym(w) {
        return true;
    }
    if !w.chars().any(is_latin_letter) {
        return true; // other scripts: no dictionary
    }
    if w.contains('-') {
        return known(d, w)
            || w.split('-').all(|p| {
                p.chars().count() <= 1
                    || p.chars().all(|c| c.is_ascii_digit())
                    || CLITICS.iter().any(|c| p.to_lowercase() == *c)
                    || (opts.ignore_uppercase && is_acronym(p))
                    || known(d, p)
            });
    }
    known(d, w)
}

/// A short word in capitals (an acronym: `ONU`, `ANAF`, `ADN`).
fn is_acronym(w: &str) -> bool {
    w.chars().all(|c| !c.is_lowercase()) && w.chars().count() <= 6
}

/// Is one word in the dictionary or the user dictionary? A comma-below word also counts when
/// the dictionary spells it with cedillas (an old dictionary); a cedilla word only when the
/// dictionary itself has it.
fn known(d: &AffixDict, w: &str) -> bool {
    if d.check(w) || crate::in_user_dictionary(&w.to_lowercase()) {
        return true;
    }
    if w.contains(['ș', 'ț', 'Ș', 'Ț']) {
        let old = cedilla_form(w);
        return d.check(&old) || crate::in_user_dictionary(&old.to_lowercase());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_lookup_accepts_romanian_file_names() {
        let dir = std::env::temp_dir().join(format!("wordcraft-ro-aliases-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["ro_RO", "ro-RO", "ro", "ro_MD"] {
            std::fs::write(dir.join(format!("{name}.aff")), test_dict::AFF).unwrap();
            std::fs::write(dir.join(format!("{name}.dic")), test_dict::DIC).unwrap();
            let (aff, dic) = crate::hunspell::find_in(&dir, NAMES).unwrap();
            let d = crate::hunspell::load(&aff, &dic).unwrap();
            assert!(is_correct_with(&d, "școală"));
            std::fs::remove_file(aff).unwrap();
            std::fs::remove_file(dic).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn romanian_words_affixes_case_and_hyphens() {
        let d = AffixDict::from_hunspell(test_dict::AFF, test_dict::DIC, &[]).unwrap();
        assert!(d.rule_count() > 0);
        for w in [
            "casă",
            "Casă",
            "CASĂ",
            "școală",
            "Școală",
            "ȘCOALĂ",
            "țară",
            "copilul",
            "nefrumos",
            "România",
            "București",
            "într-o",
            "dintr-un",
            "într-un",
            "printr-o",
            "dă-mi",
            "Dă-mi",
            "spune-mi",
            "s-a",
            "n-am",
            "mi-a",
            "ți-am",
            "să-și",
            "ADN-ul",
            "SMS-uri",
            "ONU",
            "ANAF",
        ] {
            assert!(is_correct_with(&d, w), "{w}");
        }
        for w in ["casa", "școla", "pana", "bucurești", "românie", "dă-xq", "xq-mi", "nefrumoșii", "zqinterzis", "ZQINTERZIS", "cAsă"] {
            assert!(!is_correct_with(&d, w), "{w}");
        }
    }

    #[test]
    fn cedilla_letters_are_flagged_and_fixed_to_the_comma_below_ones() {
        let d = AffixDict::from_hunspell(test_dict::AFF, test_dict::DIC, &[]).unwrap();
        for (cedilla, comma) in [("şcoală", "școală"), ("ţară", "țară"), ("Şcoală", "Școală"), ("ŞCOALĂ", "ȘCOALĂ"), ("şi", "și"), ("să-şi", "să-și")]
        {
            assert!(!is_correct_with(&d, cedilla), "{cedilla}");
            assert_eq!(cedilla_fix_with(&d, cedilla).as_deref(), Some(comma), "{cedilla}");
        }
        // Only the cedilla is wrong there; a word wrong for another reason gets no cedilla fix.
        for w in ["şcola", "școală", "casă", "xqş", ""] {
            assert_eq!(cedilla_fix_with(&d, w), None, "{w}");
        }
        assert_eq!(cedilla_fix_with(&AffixDict::default(), "şcoală"), None);
    }

    #[test]
    fn an_old_cedilla_dictionary_accepts_both_spellings() {
        let dic = "3\nşcoală\nţară\nşi\n";
        let d = AffixDict::from_hunspell("SET UTF-8\n", dic, &[]).unwrap();
        for w in ["şcoală", "școală", "Școală", "ţară", "țară", "și", "şi"] {
            assert!(is_correct_with(&d, w), "{w}");
        }
        assert_eq!(cedilla_fix_with(&d, "şcoală"), None, "the dictionary itself spells it so");
    }

    #[test]
    fn missing_dictionary_and_ignored_inputs_are_quiet() {
        let empty = AffixDict::default();
        assert!(is_correct_with(&empty, "şcola"));
        assert!(install_dictionary("FLAG long\n", "1\ncasă\n").is_err());
        let d = AffixDict::from_hunspell(test_dict::AFF, test_dict::DIC, &[]).unwrap();
        for w in ["", "a", "î", "2026", "al 2-lea", "UE", "日本語", "Москва", "-", "--", "’", "a-", "-o"] {
            assert!(is_correct_with(&d, w), "{w}");
        }
    }

    #[test]
    fn user_dictionary_applies() {
        test_dict::install();
        assert!(!is_correct("zqcuvântpropriu"));
        crate::add_word("Zqcuvântpropriu");
        assert!(is_correct("zqcuvântpropriu"));
        assert!(is_correct("Zqcuvântpropriu-mi"), "a part from the user dictionary");
    }

    #[test]
    fn romanian_dispatch_messages_options_and_mixed_spans() {
        test_dict::install();
        assert!(crate::has_dictionary(Some("ro-RO")));
        assert!(crate::is_correct_for("școală", Some("ron")));
        let text = "Casă școla";
        let issues = crate::check_text(text, &[(0..text.len(), crate::ProofLang::Ro)]);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(&text[issues[0].start..issues[0].end], "școla");
        assert_eq!(issues[0].message, "Posibilă greșeală de ortografie");
        assert_eq!(issues[0].lang, crate::ProofLang::Ro);
        assert!(crate::suggest_in("școla", 8, crate::ProofLang::Ro).contains(&"școală".into()));
        let text = "Casă şcoală";
        let issues = crate::check_text(text, &[(0..text.len(), crate::ProofLang::Ro)]);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(&text[issues[0].start..issues[0].end], "şcoală");
        assert_eq!(issues[0].message, "Se scrie cu ș și ț cu virgulă, nu cu sedilă");
        let text = "Casă this is is wrong";
        // "Casă" is five bytes.
        let issues = crate::check_text(text, &[(0..5, crate::ProofLang::Ro), (6..text.len(), crate::ProofLang::En)]);
        assert!(issues.iter().any(|i| i.lang == crate::ProofLang::En && i.message.starts_with("Repeated word")));
        assert!(issues.iter().all(|i| i.start >= 6), "{issues:?}");
        let opts = crate::Options { ignore_uppercase: false, ..Default::default() };
        assert!(!crate::is_correct_in_with("ZQINTERZIS", crate::ProofLang::Ro, &opts));
    }

    /// With Ignore words in UPPERCASE or Ignore words that contain numbers off, Romanian checks
    /// those words against the dictionary instead of accepting them.
    #[test]
    fn proofing_options_reach_the_romanian_check() {
        test_dict::install();
        let lang = crate::ProofLang::Ro;
        let defaults = crate::Options::default();
        let check_caps = crate::Options { ignore_uppercase: false, ..defaults };
        let check_digits = crate::Options { ignore_numbers: false, ..defaults };
        assert!(crate::is_correct_in_with("CASU", lang, &defaults), "short capitals are acronyms by default");
        assert!(!crate::is_correct_in_with("CASU", lang, &check_caps));
        assert!(crate::is_correct_in_with("CASĂ", lang, &check_caps));
        assert!(!crate::is_correct_in_with("XYZ-ul", lang, &check_caps), "an acronym part is checked too");
        assert!(crate::is_correct_in_with("XYZ-ul", lang, &defaults));
        assert!(crate::is_correct_in_with("casu2", lang, &defaults));
        assert!(!crate::is_correct_in_with("casu2", lang, &check_digits));
        assert!(crate::is_correct_in_with("2026", lang, &check_digits), "numbers alone stay correct");
        assert!(crate::is_correct_in_with("2-lea", lang, &check_digits), "ordinals: al 2-lea");
        assert!(is_correct("CASU"), "direct checks keep the defaults");
    }
}
