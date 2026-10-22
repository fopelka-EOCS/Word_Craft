//! Conservative Romanian grammar and punctuation checks: repeated words, a space before
//! punctuation, double spaces, a capital at the start of a sentence and doubled punctuation, with
//! Romanian messages. No agreement, comma or hyphenation rules are guessed at: a false alarm in
//! every paragraph costs more than the occasional missed mistake.

use crate::{Issue, IssueKind, ProofLang, words};

/// Words Romanian repeats on purpose: answers, interjections, onomatopoeia and emphatic doubling
/// (`încet încet`, `da da`, `tic tac`).
const ALLOWED_REPEATS: &[&str] = &[
    "da", "nu", "ha", "hai", "hei", "of", "vai", "aha", "ia", "uite", "tic", "tac", "bum", "poc", "cioc", "pac", "ham", "miau", "încet", "repede",
    "bine", "așa", "gata", "mereu", "aproape",
];

/// Abbreviations written with a period, in lower case, after which a lower-case word doesn't
/// start a sentence (`str. Mihai`, `nr. 5`, `art. 3 alin. 2`).
const ABBREVIATIONS: &[&str] = &[
    "etc", "dl", "dna", "dra", "dlui", "dnei", "dvs", "dv", "prof", "conf", "lect", "asist", "univ", "dr", "ing", "ec", "av", "pr", "sf", "gen",
    "col", "lt", "mr", "cpt", "jud", "mun", "com", "loc", "str", "bd", "bdul", "sos", "șos", "nr", "ap", "bl", "sc", "et", "tel", "pag", "pp", "vol",
    "ed", "cap", "art", "alin", "lit", "pct", "par", "fig", "tab", "cca", "aprox", "ex", "obs", "cf", "vs", "ibid", "op", "cit", "id", "ian", "feb",
    "febr", "mar", "apr", "iun", "iul", "aug", "sep", "sept", "oct", "nov", "dec", "sec", "min", "orig", "trad", "coord", "red", "an", "gr", "ha",
];

fn issue(start: usize, end: usize, message: String, suggestions: Vec<String>) -> Issue {
    Issue { start, end, kind: IssueKind::Grammar, message, suggestions, lang: ProofLang::Ro }
}

/// Is `prev`, before a period, an abbreviation, an initial, a number or a Roman numeral (after
/// which a lower-case word doesn't start a sentence)?
fn abbreviation_or_number(prev: &str) -> bool {
    prev.chars().count() <= 1
        || prev.chars().any(|c| c.is_ascii_digit())
        || prev.chars().all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
        || ABBREVIATIONS.contains(&prev.to_lowercase().as_str())
}

const CLOSING: [char; 7] = ['"', '”', '’', '\'', ')', '»', ']'];
const OPENING: [char; 9] = ['"', '„', '“', '«', '(', '[', '\'', '—', '–'];

/// Does the gap between two words end a sentence: a `.`, `!` or `?` (not an ellipsis, not after
/// an abbreviation), then space, with nothing but quotes, brackets and dashes around?
fn ends_sentence(gap: &str, prev: &str) -> bool {
    let t = gap.trim_start_matches(CLOSING);
    let Some(end) = t.chars().next() else { return false };
    if !matches!(end, '.' | '!' | '?') || t.starts_with("..") {
        return false;
    }
    if end == '.' && abbreviation_or_number(prev) {
        return false;
    }
    let rest = t.trim_start_matches(['.', '!', '?']);
    // A quotation ending in its own `?` or `!` goes on in lower case („Vii?” a întrebat).
    if rest.starts_with(CLOSING) {
        return false;
    }
    rest.starts_with(char::is_whitespace) && rest.trim_start().chars().all(|c| OPENING.contains(&c) || c.is_whitespace())
}

/// Grammar issues in a paragraph of Romanian text, in order.
pub fn check(text: &str) -> Vec<Issue> {
    check_with(text, true)
}

/// [`check`] where `capital_first` says whether the text's first word must start a sentence with
/// a capital (false for a list item, which may be a fragment).
pub fn check_with(text: &str, capital_first: bool) -> Vec<Issue> {
    let ws = words(text);
    let mut v = Vec::new();
    repeated_words(text, &ws, &mut v);
    spaces(text, &mut v);
    sentence_capitals(text, &ws, capital_first, &mut v);
    doubled_punctuation(text, &mut v);
    v.sort_by_key(|i| (i.start, i.end));
    v
}

fn repeated_words(text: &str, ws: &[(usize, usize)], v: &mut Vec<Issue>) {
    for pair in ws.windows(2) {
        let [(a0, b0), (a1, b1)] = pair else { continue };
        let (Some(w0), Some(w1), Some(gap)) = (text.get(*a0..*b0), text.get(*a1..*b1), text.get(*b0..*a1)) else { continue };
        if !gap.is_empty()
            && gap.chars().all(char::is_whitespace)
            && w0.chars().all(char::is_alphabetic)
            && w0.to_lowercase() == w1.to_lowercase()
            && !ALLOWED_REPEATS.contains(&w0.to_lowercase().as_str())
        {
            v.push(issue(*a0, *b1, format!("Cuvânt repetat: „{w1}”"), vec![w0.to_string()]));
        }
    }
}

fn spaces(text: &str, v: &mut Vec<Issue>) {
    for (i, c) in text.char_indices() {
        if c != ' ' {
            continue;
        }
        let rest = text.get(i + 1..).unwrap_or("");
        let mut it = rest.chars();
        let Some(p) = it.next() else { continue };
        let after = it.next();
        // A ratio or score keeps the spaces in digit-space-colon-space-digit.
        let ratio = p == ':'
            && text.get(..i).and_then(|before| before.chars().next_back()).is_some_and(|c| c.is_ascii_digit())
            && after == Some(' ')
            && it.next().is_some_and(|c| c.is_ascii_digit());
        let flag = !ratio
            && match p {
                ',' => true,
                // `;)` and `;-)` are emoticons.
                ';' => !matches!(after, Some(')' | '(' | '-' | 'P' | 'D')),
                // ` ...` is an ellipsis, ` .5` a number.
                '.' => !matches!(after, Some('.')) && !after.is_some_and(|a| a.is_ascii_digit()),
                // ` :)`, ` :-)`, ` :D` and similar are emoticons; ` ?!` is still a stray space.
                ':' | '!' | '?' => after.is_none_or(|a| a.is_whitespace() || matches!(a, '"' | '”' | '’' | '!' | '?')),
                _ => false,
            };
        if flag {
            v.push(issue(i, i + 1 + p.len_utf8(), "Spațiu înaintea semnului de punctuație".into(), vec![p.to_string()]));
        }
    }
    for (i, _) in text.match_indices("  ") {
        if i > 0 && text.get(..i).is_some_and(|t| t.ends_with(' ')) {
            continue;
        }
        v.push(issue(i, i + 2, "Spațiu dublu".into(), vec![" ".into()]));
    }
}

fn sentence_capitals(text: &str, ws: &[(usize, usize)], capital_first: bool, v: &mut Vec<Issue>) {
    // An enumeration (`– mere,`) may start in lower case.
    let list_item = text.trim_end().ends_with([',', ';']);
    for (k, &(a, b)) in ws.iter().enumerate() {
        let Some(w) = text.get(a..b) else { continue };
        let starts = if k == 0 {
            let before = text.get(..a).unwrap_or("");
            capital_first && !list_item && before.chars().all(|c| c.is_whitespace() || OPENING.contains(&c))
        } else {
            let Some(&(pa, pb)) = k.checked_sub(1).and_then(|p| ws.get(p)) else { continue };
            ends_sentence(text.get(pb..a).unwrap_or(""), text.get(pa..pb).unwrap_or(""))
        };
        if starts && w.chars().next().is_some_and(char::is_lowercase) && w.chars().all(|c| c.is_alphabetic() || c == '-') {
            v.push(issue(a, b, "Propoziția trebuie să înceapă cu literă mare".into(), vec![crate::affix::capitalize(w)]));
        }
    }
}

fn doubled_punctuation(text: &str, v: &mut Vec<Issue>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if !matches!(b, b',' | b'.' | b';') {
            i += 1;
            continue;
        }
        let run = bytes.get(i..).map_or(0, |r| r.iter().take_while(|&&x| x == b).count());
        // `..` is a slip for `.` or `…`; `...` is an ellipsis.
        let flag = if b == b'.' { run == 2 } else { run >= 2 };
        if flag {
            let suggestions: Vec<String> = if b == b'.' { vec![".".into(), "…".into()] } else { vec![char::from(b).to_string()] };
            v.push(issue(i, i + run, "Semn de punctuație repetat".into(), suggestions));
        }
        i += run.max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<(String, String)> {
        check(text).into_iter().map(|i| (text[i.start..i.end].to_string(), i.message)).collect()
    }

    #[test]
    fn romanian_messages_and_suggestions() {
        let text = "Am mers mers acasă ,  apoi am plecat.. Mâine";
        let issues = check(text);
        assert!(issues.iter().all(|i| i.lang == ProofLang::Ro && i.kind == IssueKind::Grammar));
        let f = found(text);
        assert!(f.contains(&("mers mers".into(), "Cuvânt repetat: „mers”".into())), "{f:?}");
        assert!(f.contains(&(" ,".into(), "Spațiu înaintea semnului de punctuație".into())), "{f:?}");
        assert!(f.contains(&("  ".into(), "Spațiu dublu".into())), "{f:?}");
        assert!(f.contains(&("..".into(), "Semn de punctuație repetat".into())), "{f:?}");
        let repeated = issues.iter().find(|i| i.message.starts_with("Cuvânt repetat")).unwrap();
        assert_eq!(repeated.suggestions, ["mers"]);
    }

    #[test]
    fn sentence_capitals_after_full_stops_but_not_abbreviations() {
        assert_eq!(found("Am venit. apoi am plecat."), [("apoi".into(), "Propoziția trebuie să înceapă cu literă mare".into())]);
        assert_eq!(found("„ce faci?” a întrebat."), [("ce".into(), "Propoziția trebuie să înceapă cu literă mare".into())]);
        assert_eq!(check("ștergem tot.")[0].suggestions, ["Ștergem"]);
        for text in [
            "Locuiește pe str. mihai Eminescu nr. 5.",
            "Vezi art. 3 alin. 2 lit. b din lege.",
            "Mere, pere etc. și alte fructe.",
            "Prof. dr. ing. Popescu a venit.",
            "Am citit cap. II. apoi am plecat.",
            "Ora 10. după aceea pauză.",
            "Așteaptă... apoi pleacă.",
        ] {
            assert!(check(text).iter().all(|i| !i.message.starts_with("Propoziția")), "{text}: {:?}", found(text));
        }
    }

    #[test]
    fn intended_repetitions_ratios_emoticons_and_ellipses_are_quiet() {
        for text in [
            "Da da, vin acum.",
            "Merge încet încet spre casă.",
            "Ceasul face tic tac.",
            "Scorul a fost 2 : 1.",
            "Bine :) Ne vedem ;)",
            "Și apoi... nimic.",
            "Ea, ea a spus-o.",
            "Valoarea este .5 din total.",
            "Într-o zi, într-un an.",
        ] {
            assert!(check(text).is_empty(), "{text}: {:?}", found(text));
        }
    }

    #[test]
    fn a_list_item_may_start_in_lower_case() {
        assert!(check_with("mere și pere", false).is_empty());
        assert_eq!(found("mere și pere").len(), 1);
        assert!(check("– mere,").is_empty());
    }

    proptest::proptest! {
        #[test]
        fn never_panics_and_ranges_are_valid(s in "\\PC{0,80}") {
            for issue in check(&s).into_iter().chain(check_with(&s, false)) {
                proptest::prop_assert!(issue.start <= issue.end && issue.end <= s.len());
                proptest::prop_assert!(s.is_char_boundary(issue.start) && s.is_char_boundary(issue.end));
            }
        }

        #[test]
        fn romanian_punctuation_never_panics(s in "[aăâîșțşţ „”«».,;:!?()\\-–— ]{0,60}") {
            for issue in check(&s) {
                proptest::prop_assert!(s.is_char_boundary(issue.start) && s.is_char_boundary(issue.end));
            }
        }
    }
}
