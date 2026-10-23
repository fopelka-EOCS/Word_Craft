//! Spelling suggestions for Dutch (`ProofLang::Nl`), driven by the installed `nl_NL` / `nl_BE`
//! [`AffixDict`] (OpenTaal and friends).
//!
//! Ranked by how Dutch typos happen in practice:
//!
//! 1. **Proper-noun capital**: `amsterdam` → `Amsterdam`, `nederland` → `Nederland`.
//! 2. **Diacritics (`MAP`)**: `cafe` → `café`, `tweeen` → `tweeën`, `belgie` → `België`,
//!    `geinteresseerd` → `geïnteresseerd`, `coordinatie` → `coördinatie` — up to two `MAP`
//!    substitutions, so missing tremas (diaereses) and accents come first.
//! 3. **Orthographic confusions (`REP`)**: `dt`/`d`/`t` (`worddt` → `wordt`, `gebeurdt` →
//!    `gebeurt`), `ei`/`ij`, `au`/`ou`, apostrophe plurals and diminutives (`autos` → `auto's`,
//!    `babytje` → `baby'tje`), and `_` multi-word replacements (`nietwaar` → `niet waar`).
//! 4. **Missing space**: `inhet` → `in het`, `opde` → `op de`, when both parts are words and one
//!    is a common short Dutch function word.
//! 5. **One-letter edit** (deletion, adjacent transposition, substitution, insertion) in `TRY`
//!    order, `MAP` relatives preferred.
//! 6. **Stem scan**: remaining distance-2 words by Damerau-Levenshtein distance.

use std::collections::HashSet;

use crate::affix::AffixDict;

/// Common short Dutch function words that often lose their space next to another word.
const COMMON_WORDS: &[&str] = &[
    "de", "het", "een", "en", "of", "maar", "want", "dat", "wat", "wie", "waar", "hoe", "ik", "jij", "je", "u", "hij", "zij", "ze", "wij", "we",
    "in", "op", "aan", "uit", "van", "voor", "naar", "met", "bij", "tot", "over", "onder", "door", "tegen", "zonder", "niet", "geen", "wel", "ook",
    "nog", "al", "te", "er", "zo", "nu", "dan", "dit", "die", "mijn", "zijn", "haar", "ons", "onze",
];

/// Up to `max` suggestions for `word` from the installed Dutch dictionary.
pub fn suggest(word: &str, max: usize) -> Vec<String> {
    let Some(d) = super::dictionary() else { return Vec::new() };
    suggest_with(&d, word, max)
}

/// Up to `max` suggestions for `word` from `dict`.
pub fn suggest_with(dict: &AffixDict, word: &str, max: usize) -> Vec<String> {
    if max == 0 || dict.is_empty() || word.contains(char::is_whitespace) {
        return Vec::new();
    }
    let w = word.trim_matches(|c: char| !c.is_alphabetic() && c != '\'' && c != '’' && c != '-');
    let w = w.replace('’', "'");
    let lower = w.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let n = chars.len();
    if n == 0 || n > 40 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // 1. Capitalized in the dictionary (`amsterdam` → `Amsterdam`).
    let cap = capitalize(&lower);
    if accept(dict, &cap) && dict.can_suggest(&cap) && cap != w {
        out.push(cap.clone());
        if out.len() >= max {
            return out;
        }
    }
    seen.insert(lower.clone());

    let mut push = |cand: &str, preserve_own_case: bool| -> bool {
        if !cand.split(' ').all(|part| dict.can_suggest(part)) {
            return out.len() >= max;
        }
        if !seen.insert(cand.to_lowercase()) {
            return out.len() >= max;
        }
        let formatted = if preserve_own_case { match_case_preserving_proper(&w, cand) } else { match_case(&w, cand) };
        out.push(formatted);
        out.len() >= max
    };

    // 2. Diacritics (`MAP`): 1 or 2 replacements inside the same `MAP` group (`cafe` → `café`,
    //    `tweeen` → `tweeën`, `belgie` → `België`).
    let mut map_cands = Vec::new();
    map_edits(dict, &chars, 0, 2, &mut Vec::new(), &mut map_cands);
    for c in map_cands {
        if let Some((accepted, is_proper)) = accept_or_capitalized(dict, &c)
            && push(&accepted, is_proper)
        {
            return out;
        }
    }

    // 3. `REP` table (includes multi-word replacements with `_` -> ` `).
    for c in rep_edits(dict, &lower) {
        if c.contains(' ') {
            if c.split(' ').all(|part| !part.is_empty() && accept_or_capitalized(dict, part).is_some()) && push(&c, false) {
                return out;
            }
        } else if let Some((accepted, is_proper)) = accept_or_capitalized(dict, &c)
            && push(&accepted, is_proper)
        {
            return out;
        }
    }
    // Two chained `REP` substitutions on short words.
    if n <= 16 {
        let first = rep_edits(dict, &lower);
        for mid in &first {
            if mid.contains(' ') {
                continue;
            }
            for c in rep_edits(dict, mid) {
                if !c.contains(' ')
                    && let Some((accepted, is_proper)) = accept_or_capitalized(dict, &c)
                    && push(&accepted, is_proper)
                {
                    return out;
                }
            }
        }
    }

    // 4. Missing space (`inhet` → `in het`, `opde` → `op de`).
    for i in 1..n {
        let a: String = chars[..i].iter().collect();
        let b: String = chars[i..].iter().collect();
        let common = COMMON_WORDS.contains(&a.as_str()) || COMMON_WORDS.contains(&b.as_str());
        let both_long = i >= 3 && n - i >= 3;
        if (common || both_long) && accept(dict, &a) && accept(dict, &b) {
            let pair = format!("{a} {b}");
            if push(&pair, false) {
                return out;
            }
        }
    }

    // 5. Distance-1 edits (delete, transpose, substitute, insert).
    let mut d1_key = Vec::new();
    let mut d1_other = Vec::new();
    edits_d1(dict, &chars, &mut d1_key, &mut d1_other);
    for c in d1_key.into_iter().chain(d1_other) {
        if push(&c, false) {
            return out;
        }
    }

    // 6. Scan stems sharing the first letter (or a MAP relative) for remaining distance-1/2 matches.
    if let Some(&first) = chars.first() {
        let mut firsts: Vec<char> = vec![first];
        for g in dict.map() {
            if g.contains(&first) {
                firsts.extend(g.iter().copied());
            }
        }
        let upper: Vec<char> = firsts.iter().flat_map(|c| c.to_uppercase()).collect();
        firsts.extend(upper);
        firsts.sort_unstable();
        firsts.dedup();
        let mut scored: Vec<(usize, u8, usize, String)> = Vec::new();
        for f in firsts {
            let prefix = f.to_string();
            for stem in dict.stems_with_prefix(&prefix) {
                let sc: Vec<char> = stem.to_lowercase().chars().collect();
                if sc.len().abs_diff(n) > 2 {
                    continue;
                }
                let d = damerau(&chars, &sc);
                if d <= 2 && accept(dict, stem) {
                    let same_first = u8::from(sc.first() != chars.first());
                    scored.push((d, same_first, sc.len().abs_diff(n), stem.to_string()));
                }
            }
        }
        scored.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)).then(a.3.cmp(&b.3)));
        for (_, _, _, s) in scored {
            let is_proper = s.chars().next().is_some_and(char::is_uppercase);
            if push(&s, is_proper) {
                break;
            }
        }
    }
    out
}

fn accept(dict: &AffixDict, w: &str) -> bool {
    dict.contains_suggestible(w)
}

/// Checks if `w` or its capitalized form (`België`) is in `dict`.
fn accept_or_capitalized(dict: &AffixDict, w: &str) -> Option<(String, bool)> {
    if accept(dict, w) {
        return Some((w.to_string(), false));
    }
    let cap = capitalize(w);
    if cap != w && accept(dict, &cap) {
        return Some((cap, true));
    }
    None
}

/// All strings reachable by 1..=`left` substitutions within [`AffixDict::map`] groups.
fn map_edits(dict: &AffixDict, chars: &[char], pos: usize, left: usize, cur: &mut Vec<char>, out: &mut Vec<String>) {
    if out.len() >= 64 {
        return;
    }
    if pos == chars.len() {
        if cur.as_slice() != chars {
            out.push(cur.iter().collect());
        }
        return;
    }
    let c = chars[pos];
    cur.push(c);
    map_edits(dict, chars, pos + 1, left, cur, out);
    cur.pop();
    if left > 0 {
        for group in dict.map() {
            if group.contains(&c) {
                for &r in group.iter().filter(|&&r| r != c) {
                    cur.push(r);
                    map_edits(dict, chars, pos + 1, left - 1, cur, out);
                    cur.pop();
                }
            }
        }
    }
}

/// Strings produced by applying one [`AffixDict::rep`] rule to `lower`.
fn rep_edits(dict: &AffixDict, lower: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (pat, rep) in dict.rep() {
        if pat.is_empty() {
            continue;
        }
        let rep_clean = rep.replace('_', " ");
        for (i, _) in lower.match_indices(pat.as_str()) {
            let (Some(head), Some(tail)) = (lower.get(..i), lower.get(i + pat.len()..)) else {
                continue;
            };
            out.push(format!("{head}{rep_clean}{tail}"));
        }
    }
    out
}

/// Distance-1 edits of `chars` that are in `dict`: transpositions and `MAP` relatives go into
/// `preferred`, the rest into `other`.
fn edits_d1(dict: &AffixDict, chars: &[char], preferred: &mut Vec<String>, other: &mut Vec<String>) {
    let n = chars.len();
    let alphabet = try_alphabet(dict);
    let are_mapped = |a: char, b: char| dict.map().iter().any(|g| g.contains(&a) && g.contains(&b));
    // Adjacent transposition.
    let mut buf = chars.to_vec();
    for i in 0..n.saturating_sub(1) {
        buf.swap(i, i + 1);
        let s: String = buf.iter().collect();
        if accept(dict, &s) {
            preferred.push(s);
        }
        buf.swap(i, i + 1);
    }
    // Deletion.
    for i in 0..n {
        let s: String = chars[..i].iter().chain(&chars[i + 1..]).collect();
        if accept(dict, &s) {
            other.push(s);
        }
    }
    // Substitution.
    for i in 0..n {
        let orig = buf[i];
        for &c in &alphabet {
            if c == orig {
                continue;
            }
            buf[i] = c;
            let s: String = buf.iter().collect();
            if accept(dict, &s) {
                if are_mapped(orig, c) {
                    preferred.push(s);
                } else {
                    other.push(s);
                }
            }
        }
        buf[i] = orig;
    }
    // Insertion.
    for i in 0..=n {
        for &c in &alphabet {
            let s: String = chars[..i].iter().chain(std::iter::once(&c)).chain(&chars[i..]).collect();
            if accept(dict, &s) {
                other.push(s);
            }
        }
    }
}

fn try_alphabet(dict: &AffixDict) -> Vec<char> {
    if !dict.try_chars().is_empty() {
        dict.try_chars().chars().filter(|c| c.is_lowercase() || *c == '\'').collect()
    } else {
        "esiaonrtldkumgcbpvwjfhzyxéèëêïöü'".chars().collect()
    }
}

fn capitalize(w: &str) -> String {
    if let Some(rest) = w.strip_prefix("ij") {
        return format!("IJ{rest}");
    }
    let mut c = w.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn match_case(orig: &str, cand: &str) -> String {
    let cap = orig.chars().next().is_some_and(char::is_uppercase);
    let upper = orig.chars().all(|c| !c.is_lowercase()) && orig.chars().count() > 1;
    if upper {
        cand.to_uppercase()
    } else if cap {
        capitalize(cand)
    } else {
        cand.to_string()
    }
}

fn match_case_preserving_proper(orig: &str, cand: &str) -> String {
    let upper = orig.chars().all(|c| !c.is_lowercase()) && orig.chars().count() > 1;
    if upper { cand.to_uppercase() } else { cand.to_string() }
}

fn damerau(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > 2 {
        return 3;
    }
    let mut prev2 = vec![0usize; m + 1];
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        if let Some(c) = cur.get_mut(0) {
            *c = i;
        }
        let mut row_min = i;
        for j in 1..=m {
            let cost = usize::from(a.get(i - 1) != b.get(j - 1));
            let mut v = (prev.get(j).copied().unwrap_or(9) + 1)
                .min(cur.get(j - 1).copied().unwrap_or(9) + 1)
                .min(prev.get(j - 1).copied().unwrap_or(9) + cost);
            if i > 1 && j > 1 && a.get(i - 1) == b.get(j - 2) && a.get(i - 2) == b.get(j - 1) {
                v = v.min(prev2.get(j - 2).copied().unwrap_or(9) + 1);
            }
            if let Some(c) = cur.get_mut(j) {
                *c = v;
            }
            row_min = row_min.min(v);
        }
        if row_min > 2 {
            return 3;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev.get(m).copied().unwrap_or(9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nosuggest_stems_and_forms_are_never_offered() {
        crate::affix::tests::assert_nosuggest_is_honoured(suggest_with);
    }
}
