//! Dutch grammar rules (`ProofLang::Nl`).
//!
//! Conservative checks with Dutch messages:
//!
//! - **Herhaald woord**: `de de`, `hij hij` (allows intentional repeats like `ja ja`, `nee nee`,
//!   `nou nou`, `zo zo`, `hè hè`, `ha ha`).
//! - **Spatie voor leesteken** and **Dubbele spatie**.
//! - **Hoofdletter aan het begin van de zin**, aware of Dutch abbreviations (`bijv.`, `o.a.`,
//!   `d.w.z.`, `enz.`, `dr.`, `prof.`, `mr.`, `ir.`, `nr.`, `blz.`…), Dutch sentence-initial
//!   apostrophe words (`'s Morgens`, `'t Gooi`, `'s-Gravenhage`) and the `IJ` digraph (`ijs` →
//!   `IJs`).
//! - **Dubbel leesteken** (`..` that isn't `...`, `,,`, `;;`, `!!`, `??`).

use crate::{Issue, IssueKind, ProofLang, words};

/// Abbreviations ending with a full stop that do not normally end a sentence when followed by a
/// lowercase word or number (`bijv. dit`, `blz. 12`, `dr. Jansen`).
const ABBREVIATIONS: &[&str] = &[
    "bijv", "bv", "o.a", "oa", "d.w.z", "dwz", "m.a.w", "maw", "enz", "etc", "dr", "prof", "mr", "ir", "ing", "drs", "ds", "mgr", "st", "nr", "nrs",
    "blz", "p", "pp", "fig", "tab", "art", "par", "jl", "a.s", "as", "ca", "resp", "vgl", "z.o.z", "zoz", "t.a.v", "tav", "m.b.t", "mbt", "n.a.v",
    "nav", "i.p.v", "ipv", "i.v.m", "ivm", "dhr", "mevr", "mw", "mej", "geb", "overl", "tel", "max", "min", "sec",
];

/// Words that can legitimately be doubled in Dutch without being a typo (`ja ja`, `nee nee`,
/// `nou nou`, `zo zo`).
const INTENTIONAL_REPEATS: &[&str] = &["ja", "nee", "nou", "zo", "hè", "ha", "ho", "tut", "kom", "wel", "bla", "poeh", "tja"];

/// Capitalize a Dutch word at the start of a sentence (converting initial `ij` to `IJ`).
fn capitalize_nl(w: &str) -> String {
    if let Some(rest) = w.strip_prefix("ij") {
        return format!("IJ{rest}");
    }
    let mut chars = w.chars();
    chars.next().map(|f| f.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Grammar issues in a paragraph of Dutch text.
pub fn check(text: &str) -> Vec<Issue> {
    check_with(text, true)
}

/// [`check`] where `capital_first` says whether the paragraph's first word must start with a
/// capital letter (false for a list item, which may be a fragment).
pub fn check_with(text: &str, capital_first: bool) -> Vec<Issue> {
    let mut v = Vec::new();
    let ws = words(text);

    // 1. Repeated words ("Hij hij", "de de").
    for pair in ws.windows(2) {
        let [(a0, b0), (a1, b1)] = pair else { continue };
        let (Some(w0), Some(w1), Some(gap)) = (text.get(*a0..*b0), text.get(*a1..*b1), text.get(*b0..*a1)) else {
            continue;
        };
        let l0 = w0.to_lowercase();
        if gap.trim().is_empty() && l0 == w1.to_lowercase() && w0.chars().all(char::is_alphabetic) && !INTENTIONAL_REPEATS.contains(&l0.as_str()) {
            v.push(Issue {
                start: *a0,
                end: *b1,
                kind: IssueKind::Grammar,
                message: format!("Herhaald woord: \"{w1}\""),
                suggestions: vec![w0.to_string()],
                lang: ProofLang::Nl,
            });
        }
    }

    // 2. Space before punctuation; doubled spaces.
    for p in [" ,", " .", " ;", " !", " ?", " :"] {
        for (i, _) in text.match_indices(p) {
            if text.get(i + 2..).is_some_and(|r| r.starts_with('.') || r.starts_with(',')) {
                continue; // "..." or ".5"
            }
            if p == " :" && text.get(i + 2..).is_some_and(|r| r.starts_with(')') || r.starts_with('(') || r.starts_with('D')) {
                continue;
            }
            v.push(Issue {
                start: i,
                end: i + 2,
                kind: IssueKind::Grammar,
                message: "Spatie voor leesteken".into(),
                suggestions: vec![text.get(i + 1..i + 2).unwrap_or("").to_string()],
                lang: ProofLang::Nl,
            });
        }
    }
    for (i, _) in text.match_indices("  ") {
        if i > 0 && text.get(..i).is_some_and(|t| t.ends_with(' ')) {
            continue;
        }
        v.push(Issue {
            start: i,
            end: i + 2,
            kind: IssueKind::Grammar,
            message: "Dubbele spatie".into(),
            suggestions: vec![" ".into()],
            lang: ProofLang::Nl,
        });
    }

    // 3. Sentence start capital (aware of `'s Morgens`, `'t Gooi`, `'s-Gravenhage` and `IJ`).
    let mut after_end = capital_first;
    for (a, b) in &ws {
        let Some(w) = text.get(*a..*b) else { continue };
        let preceded_by_apos = text.get(..*a).is_some_and(|before| before.ends_with('\'') || before.ends_with('’'));
        if after_end && preceded_by_apos && matches!(w, "s" | "t" | "n") {
            // In Dutch, a sentence starting with `'s`, `'t` or `'n` keeps the clitic lowercase
            // and capitalizes the word that follows (`'s Morgens`, `'t Gooi`).
            continue;
        }
        if after_end
            && preceded_by_apos
            && let Some(rest) = w.strip_prefix("s-").or_else(|| w.strip_prefix("t-"))
        {
            if rest.chars().next().is_some_and(char::is_lowercase) {
                let prefix = w.get(..2).unwrap_or("s-");
                let fixed = format!("{prefix}{}", capitalize_nl(rest));
                v.push(Issue {
                    start: *a,
                    end: *b,
                    kind: IssueKind::Grammar,
                    message: "Hoofdletter aan het begin van de zin".into(),
                    suggestions: vec![fixed],
                    lang: ProofLang::Nl,
                });
            }
            after_end = false;
            continue;
        }
        if after_end && w.chars().next().is_some_and(char::is_lowercase) && w.chars().all(char::is_alphabetic) {
            let fixed = capitalize_nl(w);
            v.push(Issue {
                start: *a,
                end: *b,
                kind: IssueKind::Grammar,
                message: "Hoofdletter aan het begin van de zin".into(),
                suggestions: vec![fixed],
                lang: ProofLang::Nl,
            });
        }
        let tail = text.get(*b..).unwrap_or("");
        let punctuation = tail.trim_start_matches(['"', '”', '»', ')', '\'', '’']);
        let w_lower = w.to_lowercase();
        let is_abbr = punctuation.starts_with('.')
            && (ABBREVIATIONS.contains(&w_lower.as_str()) || (w.chars().count() == 1 && w.chars().all(char::is_alphabetic)));
        after_end = !is_abbr
            && matches!(punctuation.chars().next(), Some('.' | '!' | '?'))
            && !punctuation.starts_with("..")
            && punctuation.trim_start_matches(['.', '!', '?', '"', '”', '»', ')', '\'', '’']).starts_with(' ');
    }

    // 4. Doubled punctuation (`..` not `...`, `,,`, `;;`, `!!`, `??`).
    for (pat, single) in [("..", "."), (",,", ","), (";;", ";"), ("!!", "!"), ("??", "?")] {
        for (i, _) in text.match_indices(pat) {
            let ch = single.chars().next().unwrap_or('.');
            let before = text.get(..i).is_some_and(|t| t.ends_with(ch));
            let after = text.get(i + 2..).is_some_and(|t| t.starts_with(ch));
            if before || after {
                continue;
            }
            // Skip Dutch opening quotes `,,woord''` when `,,` is immediately followed by a letter.
            if pat == ",," && text.get(i + 2..).and_then(|t| t.chars().next()).is_some_and(char::is_alphabetic) {
                continue;
            }
            v.push(Issue {
                start: i,
                end: i + 2,
                kind: IssueKind::Grammar,
                message: "Dubbel leesteken".into(),
                suggestions: vec![single.into()],
                lang: ProofLang::Nl,
            });
        }
    }

    v.sort_by_key(|i| i.start);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_words() {
        let issues = check("Hij hij ging naar huis.");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].message, "Herhaald woord: \"hij\"");
        assert_eq!(issues[0].suggestions, vec!["Hij"]);

        assert!(check("Ja ja, dat klopt.").is_empty());
        assert!(check("Nee nee, we blijven hier.").is_empty());
        assert!(check("Nou nou, dat is mooi.").is_empty());
    }

    #[test]
    fn sentence_capital_and_abbreviations() {
        let issues = check("dit is een test. en nog een zin.");
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].suggestions, vec!["Dit"]);
        assert_eq!(issues[1].suggestions, vec!["En"]);

        // IJ digraph at sentence start: both I and J are capitalized.
        let ij_issues = check("ijs is koud.");
        assert_eq!(ij_issues.len(), 1);
        assert_eq!(ij_issues[0].suggestions, vec!["IJs"]);

        // Dutch abbreviations do not trigger a false sentence-start capital error.
        assert!(check("We kopen bijv. appels en peren.").is_empty());
        assert!(check("Zie blz. 12 en vgl. hoofdstuk drie.").is_empty());
        assert!(check("Welkom bij dr. de Vries.").is_empty());

        // Dutch sentence-initial clitics ('s, 't, 'n) capitalize the next word, not the clitic.
        assert!(check("'s Morgens is het koud.").is_empty());
        assert!(check("'t Gooi ligt in Noord-Holland.").is_empty());
        assert!(check("'s-Gravenhage is een mooie stad.").is_empty());

        let clitic_issues = check("'s morgens is het koud.");
        assert_eq!(clitic_issues.len(), 1);
        assert_eq!(clitic_issues[0].suggestions, vec!["Morgens"]);

        let place_issues = check("'s-gravenhage is een mooie stad.");
        assert_eq!(place_issues.len(), 1);
        assert_eq!(place_issues[0].suggestions, vec!["s-Gravenhage"]);
    }

    #[test]
    fn punctuation_and_spacing() {
        let issues = check("Hallo , wereld  en tot ziens..");
        assert_eq!(issues.len(), 3, "{issues:?}");
        assert_eq!(issues[0].message, "Spatie voor leesteken");
        assert_eq!(issues[1].message, "Dubbele spatie");
        assert_eq!(issues[2].message, "Dubbel leesteken");
    }
}
