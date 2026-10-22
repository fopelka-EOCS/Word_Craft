//! Romanian suggestions. Two fixes come first, each only when the dictionary has the result: the
//! comma-below spelling of a word typed with cedillas (`şcoală` → `școală`), and the post-1993
//! `sunt` forms of the verb written `sînt` before the reform (`sîntem` → `suntem`; MAP alone
//! would offer the unrelated `sânt`). Then the dictionary-driven ranking Czech uses
//! ([`crate::cs::suggest::suggest_in`]: case, MAP, REP, missing spaces, KEY, TRY edits and a
//! stem scan), whose tables come from the installed `ro_RO`.

use crate::affix::{AffixDict, Casing, capitalize, casing};

/// The pre-1993 spellings of the present of "a fi" whose current form isn't a MAP slip away
/// (`sînt` → `sunt`), in lower case, with the cedilla variants old keyboards type.
const SINT: &[(&str, &str)] = &[("sînt", "sunt"), ("sîntem", "suntem"), ("sînteți", "sunteți"), ("sînteţi", "sunteți")];

/// Spelling suggestions for a Romanian word, best first, at most `max`, in the word's case.
pub fn suggest(word: &str, max: usize) -> Vec<String> {
    suggest_in(&super::dictionary(), word, max)
}

/// [`suggest`] against a given dictionary.
pub fn suggest_in(d: &AffixDict, word: &str, max: usize) -> Vec<String> {
    let word = word.trim();
    if max == 0 || d.is_empty() || word.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    if let Some(fix) = super::cedilla_fix_with(d, word) {
        out.push(fix);
    }
    let lower = word.to_lowercase();
    if let Some((_, now)) = SINT.iter().find(|(old, _)| *old == lower) {
        let now = in_case_of(word, now);
        if d.can_suggest(&now) {
            out.push(now);
        }
    }
    for s in crate::cs::suggest::suggest_in(d, word, max) {
        if out.len() >= max {
            break;
        }
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out.truncate(max);
    out
}

/// `cand` (lower case) in the letter case of `word`.
fn in_case_of(word: &str, cand: &str) -> String {
    match casing(word) {
        Casing::Initial => capitalize(cand),
        Casing::Upper => cand.to_uppercase(),
        Casing::Lower | Casing::Mixed => cand.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ro::test_dict;

    fn dict() -> AffixDict {
        AffixDict::from_hunspell(test_dict::AFF, test_dict::DIC, &[]).unwrap()
    }

    #[test]
    fn romanian_suggestions_use_runtime_tables_and_case() {
        let d = dict();
        for (word, want) in [
            ("pana", "până"),
            ("cind", "când"),
            ("cînd", "când"),
            ("mîine", "mâine"),
            ("casa", "casă"),
            ("scoala", "școală"),
            ("Scoala", "Școală"),
            ("romania", "România"),
            ("fooarte", "foarte"),
            ("copilull", "copilul"),
            ("nefrumso", "nefrumos"),
            ("multbine", "mult bine"),
        ] {
            assert!(suggest_in(&d, word, 8).contains(&want.into()), "{word}: {:?}", suggest_in(&d, word, 8));
        }
    }

    #[test]
    fn the_comma_below_form_and_sunt_come_first() {
        let d = dict();
        for (word, first) in [
            ("şcoală", "școală"),
            ("Şcoală", "Școală"),
            ("ŞCOALĂ", "ȘCOALĂ"),
            ("ţară", "țară"),
            ("şi", "și"),
            ("sînt", "sunt"),
            ("Sînt", "Sunt"),
            ("SÎNTEM", "SUNTEM"),
            ("sînteți", "sunteți"),
            ("sînteţi", "sunteți"),
        ] {
            assert_eq!(suggest_in(&d, word, 4).first().map(String::as_str), Some(first), "{word}: {:?}", suggest_in(&d, word, 4));
        }
        // Each is offered once.
        let s = suggest_in(&d, "şcoală", 8);
        assert_eq!(s.iter().filter(|x| *x == "școală").count(), 1, "{s:?}");
        // A dictionary without a MAP line for the cedilla letters (nor ș/ț in its TRY) still
        // gets the comma-below form, first.
        let bare = AffixDict::from_hunspell("SET UTF-8\nTRY aeiou\n", "4\nșcoală\nțară\nșcoli\nșcolar\n", &[]).unwrap();
        for (word, first) in [("şcoală", "școală"), ("Ţară", "Țară"), ("şcolar", "școlar")] {
            assert_eq!(suggest_in(&bare, word, 4).first().map(String::as_str), Some(first), "{word}: {:?}", suggest_in(&bare, word, 4));
        }
    }

    #[test]
    fn suggestions_stay_bounded_and_skip_forbidden_words() {
        let d = dict();
        for w in ["", "două cuvinte", &"a".repeat(41)] {
            assert!(suggest_in(&d, w, 4).is_empty(), "{w}");
        }
        assert!(suggest_in(&d, "pana", 0).is_empty());
        assert!(suggest_in(&d, "şcoală", 0).is_empty());
        assert!(suggest_in(&AffixDict::default(), "şcoală", 4).is_empty());
        assert!(suggest_in(&d, "sînt", 1).len() == 1);
        assert!(suggest_in(&d, "casă", 4).len() <= 4);
        assert!(!suggest_in(&d, "casă", 4).contains(&"casă".into()));
        assert!(!suggest_in(&d, "zqinterzs", 8).contains(&"zqinterzis".into()));
        // The sunt forms are offered only when the dictionary has them.
        let without = AffixDict::from_hunspell("SET UTF-8\n", "1\nsânt\n", &[]).unwrap();
        assert!(!suggest_in(&without, "sînt", 8).contains(&"sunt".into()));
    }

    #[test]
    fn nosuggest_stems_and_forms_are_never_offered() {
        crate::affix::tests::assert_nosuggest_is_honoured(suggest_in);
    }
}
