//! Dutch proofing (`ProofLang::Nl`): spelling against the installed Hunspell `nl_NL` / `nl_BE`
//! dictionary (OpenTaal and friends), suggestions tuned for Dutch ([`suggest`]), and conservative
//! Dutch grammar rules ([`grammar`]).
//!
//! No Dutch dictionary is bundled: WordCraft reads the user's installed `nl_NL.aff`/`nl_NL.dic`
//! (or `nl_BE`) from the system Hunspell locations ([`crate::hunspell`]) on first use. When none
//! is installed, [`has_dictionary`] is `false` and [`is_correct`] accepts every word so Dutch text
//! is never underlined with false positives.
//!
//! OpenTaal's `nl_NL.aff` uses `FLAG long` (two-character ASCII flags) and `ICONV ij ĳ` /
//! `OCONV ĳ ij` (`U+0133` / `U+0132`). [`parse_hunspell`] normalises those into single-character
//! `FLAG UTF-8` flags and standard `ij`/`IJ` digraphs before building [`AffixDict`].

pub mod grammar;
pub mod suggest;
#[cfg(test)]
pub mod test_dict;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use crate::affix::AffixDict;
use crate::hunspell;

pub use suggest::{suggest, suggest_with};

/// Dictionary names searched for in the system Hunspell directories (`nl_NL` first, then `nl_BE`).
const NAMES: &[&str] = &["nl_NL", "nl_BE", "nl-NL", "nl-BE", "nl"];
/// Largest `.aff` or `.dic` file read (matches [`crate::hunspell`]).
const MAX_FILE: u64 = 64 << 20;

fn slot() -> &'static RwLock<Option<Arc<AffixDict>>> {
    static D: OnceLock<RwLock<Option<Arc<AffixDict>>>> = OnceLock::new();
    D.get_or_init(|| RwLock::new(Some(Arc::new(load_installed()))))
}

fn read_utf8_or_latin1(p: &Path) -> Result<String, String> {
    let len = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?.len();
    if len > MAX_FILE {
        return Err(format!("{}: too large ({len} bytes)", p.display()));
    }
    let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => Ok(e.into_bytes().into_iter().map(char::from).collect()),
    }
}

fn load_installed() -> AffixDict {
    let Some((aff, dic)) = hunspell::find(NAMES) else {
        hunspell::clear_load_error("nl_NL");
        return AffixDict::default();
    };
    let result = read_utf8_or_latin1(&aff).and_then(|aff_text| {
        let dic_text = read_utf8_or_latin1(&dic)?;
        parse_hunspell(&aff_text, &dic_text)
    });
    match hunspell::note_load("nl_NL", &dic, result) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("Dutch dictionary at {} failed to load: {e}", dic.display());
            AffixDict::default()
        }
    }
}

/// Parse a Dutch Hunspell `.aff`/`.dic` pair, supporting both standard single-character flag
/// dictionaries and OpenTaal's `FLAG long` + `ĳ`/`Ĳ` (`U+0133`/`U+0132`) dictionaries.
pub(crate) fn parse_hunspell(aff: &str, dic: &str) -> Result<AffixDict, String> {
    let aff_norm = if aff.contains('ĳ') || aff.contains('Ĳ') { aff.replace('ĳ', "ij").replace('Ĳ', "IJ") } else { aff.to_string() };
    let dic_norm = if dic.contains('ĳ') || dic.contains('Ĳ') { dic.replace('ĳ', "ij").replace('Ĳ', "IJ") } else { dic.to_string() };
    let is_flag_long = aff_norm.lines().any(|line| {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        parts.next() == Some("FLAG") && parts.next() == Some("long")
    });
    if is_flag_long {
        let (conv_aff, conv_dic) = convert_flag_long(&aff_norm, &dic_norm)?;
        AffixDict::from_hunspell(&conv_aff, &conv_dic, &[])
    } else {
        AffixDict::from_hunspell(&aff_norm, &dic_norm, &[])
    }
}

/// Convert an OpenTaal-style `FLAG long` (two-character ASCII flags) `.aff`/`.dic` pair into a
/// single-character `FLAG UTF-8` pair for [`AffixDict::from_hunspell`].
fn convert_flag_long(aff: &str, dic: &str) -> Result<(String, String), String> {
    let mut only_in_compound: Option<[u8; 2]> = None;
    let mut forbidden_flag: Option<[u8; 2]> = None;
    let mut nosuggest_flag: Option<[u8; 2]> = None;
    let mut has_directives = false;

    for raw in aff.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(kw) = parts.next() else { continue };
        match kw {
            "SET" | "TRY" | "KEY" | "MAP" | "REP" | "PFX" | "SFX" => has_directives = true,
            "ONLYINCOMPOUND" => {
                if let Some(f) = parts.next().and_then(parse_flag2) {
                    only_in_compound = Some(f);
                }
            }
            "FORBIDDENWORD" => {
                has_directives = true;
                if let Some(f) = parts.next().and_then(parse_flag2) {
                    forbidden_flag = Some(f);
                }
            }
            "NOSUGGEST" => {
                has_directives = true;
                if let Some(f) = parts.next().and_then(parse_flag2) {
                    nosuggest_flag = Some(f);
                }
            }
            _ => {}
        }
    }
    if !has_directives {
        return Err("unsupported FLAG: long".into());
    }

    // First pass over PFX/SFX rules: keep non-compound-glue rules and collect active 2-char flags.
    struct RawRule {
        strip: String,
        add: String,
        cont: Vec<[u8; 2]>,
        cond: String,
    }
    struct RawGroup {
        kind: &'static str,
        flag: [u8; 2],
        cross: String,
        rules: Vec<RawRule>,
    }

    let mut groups: Vec<RawGroup> = Vec::new();
    let mut flag_map: HashMap<[u8; 2], char> = HashMap::new();
    let flag_pool: Vec<char> = ('A'..='Z').chain('a'..='z').chain('0'..='9').chain('\u{0100}'..='\u{0140}').collect();

    let mut assign_flag = |f: [u8; 2]| -> Result<char, String> {
        if let Some(&c) = flag_map.get(&f) {
            return Ok(c);
        }
        let idx = flag_map.len();
        let Some(&c) = flag_pool.get(idx) else {
            return Err("too many affix flags in FLAG long dictionary".into());
        };
        flag_map.insert(f, c);
        Ok(c)
    };

    if let Some(f) = forbidden_flag {
        assign_flag(f)?;
    }
    if let Some(f) = nosuggest_flag {
        assign_flag(f)?;
    }

    for raw in aff.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(kw) = parts.next() else { continue };
        let kind = match kw {
            "PFX" => "PFX",
            "SFX" => "SFX",
            _ => continue,
        };
        let Some(flag_str) = parts.next() else {
            return Err(format!("missing flag in {kind}"));
        };
        let Some(flag) = parse_flag2(flag_str) else {
            return Err(format!("invalid 2-char flag: {flag_str}"));
        };
        let (Some(a), Some(b)) = (parts.next(), parts.next()) else {
            return Err(format!("incomplete {kind} line"));
        };
        if (a == "Y" || a == "N") && b.parse::<usize>().is_ok() && parts.next().is_none() {
            groups.push(RawGroup { kind, flag, cross: a.to_string(), rules: Vec::new() });
        } else {
            let cond = parts.next().unwrap_or(".").to_string();
            let (add_part, cont_part) = match b.split_once('/') {
                Some((l, r)) => (l, Some(r)),
                None => (b, None),
            };
            // Skip pure compound-glue rules (zero additions `0`, trailing/leading hyphens `-`,
            // or rules marked with `ONLYINCOMPOUND`).
            if add_part == "0" || add_part.starts_with('-') || add_part.ends_with('-') {
                continue;
            }
            let mut cont = Vec::new();
            if let Some(c_str) = cont_part {
                for &f2 in c_str.as_bytes().as_chunks::<2>().0 {
                    cont.push(f2);
                }
            }
            if let Some(oic) = only_in_compound
                && cont.contains(&oic)
            {
                continue;
            }
            if let Some(g) = groups.iter_mut().rev().find(|g| g.kind == kind && g.flag == flag) {
                g.rules.push(RawRule { strip: a.to_string(), add: add_part.to_string(), cont, cond });
            }
        }
    }

    groups.retain(|g| !g.rules.is_empty());
    for g in &groups {
        assign_flag(g.flag)?;
    }

    // Build the converted `.aff`.
    let mut out_aff = String::with_capacity(aff.len());
    out_aff.push_str("SET UTF-8\nFLAG UTF-8\n");
    if let Some(f) = forbidden_flag
        && let Some(&c) = flag_map.get(&f)
    {
        out_aff.push_str(&format!("FORBIDDENWORD {c}\n"));
    }
    if let Some(f) = nosuggest_flag
        && let Some(&c) = flag_map.get(&f)
    {
        out_aff.push_str(&format!("NOSUGGEST {c}\n"));
    }
    for raw in aff.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let Some(kw) = line.split_whitespace().next() else {
            continue;
        };
        if matches!(kw, "TRY" | "KEY" | "MAP" | "REP") {
            out_aff.push_str(line);
            out_aff.push('\n');
        }
    }
    for g in &groups {
        let Some(&c) = flag_map.get(&g.flag) else {
            continue;
        };
        out_aff.push_str(&format!("{} {} {} {}\n", g.kind, c, g.cross, g.rules.len()));
        for r in &g.rules {
            let cont_mapped: String = r.cont.iter().filter_map(|cf| flag_map.get(cf).copied()).collect();
            let add_full = if cont_mapped.is_empty() { r.add.clone() } else { format!("{}/{cont_mapped}", r.add) };
            out_aff.push_str(&format!("{} {} {} {} {}\n", g.kind, c, r.strip, add_full, r.cond));
        }
    }

    // Build the converted `.dic`.
    let mut out_dic = String::with_capacity(dic.len());
    let mut lines = dic.lines();
    if let Some(first) = lines.next() {
        out_dic.push_str(first.trim());
        out_dic.push('\n');
    }
    for raw in lines {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('/') {
            continue;
        }
        let entry = line.split(['\t', ' ']).next().unwrap_or(line);
        let (stem, flags_str) = match entry.split_once('/') {
            Some((s, f)) => (s, f),
            None => (entry, ""),
        };
        if stem.is_empty() || stem.starts_with('-') || stem.ends_with('-') {
            continue;
        }
        let mut skip_stem = false;
        let mut mapped = String::new();
        for &f2 in flags_str.as_bytes().as_chunks::<2>().0 {
            if Some(f2) == only_in_compound {
                skip_stem = true;
                break;
            }
            if let Some(&mc) = flag_map.get(&f2) {
                mapped.push(mc);
            }
        }
        if skip_stem {
            continue;
        }
        out_dic.push_str(stem);
        if !mapped.is_empty() {
            out_dic.push('/');
            out_dic.push_str(&mapped);
        }
        out_dic.push('\n');
    }

    Ok((out_aff, out_dic))
}

fn parse_flag2(s: &str) -> Option<[u8; 2]> {
    let b = s.as_bytes();
    (b.len() == 2).then(|| [b[0], b[1]])
}

pub(crate) fn dictionary() -> Option<Arc<AffixDict>> {
    let d = slot().read().unwrap_or_else(|e| e.into_inner()).clone()?;
    (!d.is_empty()).then_some(d)
}

/// Replace the active Dutch dictionary with one parsed from `aff` and `dic` (for tests and tools).
pub fn install_dictionary(aff: &str, dic: &str) -> Result<(), String> {
    let path = installed_path().unwrap_or_else(|| std::path::PathBuf::from("nl_NL.dic"));
    let d = hunspell::note_load("nl_NL", &path, parse_hunspell(aff, dic))?;
    set_installed(Arc::new(d));
    Ok(())
}

pub(crate) fn set_installed(d: Arc<AffixDict>) {
    *slot().write().unwrap_or_else(|e| e.into_inner()) = Some(d);
    crate::DICTIONARY_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Reload the Dutch dictionary from the current Hunspell search path.
pub fn reload_dictionary() {
    set_installed(Arc::new(load_installed()));
}

/// Path of the `.dic` file found in the Hunspell search locations (`None` if none is installed).
pub fn installed_path() -> Option<std::path::PathBuf> {
    hunspell::find(NAMES).map(|(_, dic)| dic)
}

/// Is a Dutch Hunspell dictionary installed and loaded?
pub fn has_dictionary() -> bool {
    dictionary().is_some()
}

/// Is `word` spelled correctly in Dutch?
///
/// Always `true` when no Dutch dictionary is installed. Single letters, numbers, ALL-CAPS
/// acronyms, words with digits and words in the user dictionary are accepted.
pub fn is_correct(word: &str) -> bool {
    let Some(d) = dictionary() else { return true };
    is_correct_with(&d, word)
}

/// Is `word` spelled correctly according to `dict` (an empty dictionary accepts everything)?
pub fn is_correct_with(dict: &AffixDict, word: &str) -> bool {
    if dict.is_empty() {
        return true;
    }
    let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’' && c != '-').trim_matches('-');
    let w_norm = w.replace('’', "'");
    let w_trimmed = w_norm.trim_matches('\'');
    if w_trimmed.chars().count() <= 1
        || !w_trimmed.chars().any(char::is_alphabetic)
        || w_trimmed.chars().any(|c| c.is_ascii_digit())
        || w_trimmed.chars().all(|c| !c.is_lowercase())
    {
        return true;
    }
    let lower = w_trimmed.to_lowercase();
    if crate::in_user_dictionary(&lower) {
        return true;
    }
    if word_known(dict, w_trimmed) || word_known(dict, &w_norm) {
        return true;
    }
    // Dutch possessive on words ending in s/x/z (`Alex'`, `Max'`, `Socrates'`): `words()` strips
    // the trailing `'`, or `is_correct_with` may be called with it included.
    if let Some(stem) = w_norm.strip_suffix('\'')
        && stem.ends_with(['s', 'x', 'z', 'S', 'X', 'Z'])
        && word_known(dict, stem)
    {
        return true;
    }
    // Dutch apostrophe inflections (`auto's`, `baby'tje`, `collega's`, `Jan's`, `Anna's`): if the
    // dictionary already generated the form it matched above; also accept `stem + 'tje(s)` when
    // the base noun is in the dictionary.
    for suf in ["'tje", "'tjes"] {
        if let Some(stem) = w_trimmed.strip_suffix(suf)
            && stem.chars().count() >= 2
            && word_known(dict, stem)
        {
            return true;
        }
    }
    // Dutch place names starting with `'s-` (`'s-Gravenhage`, `'s-Hertogenbosch`) when `words()`
    // trimmed the leading `'`: check with a leading `'` restored.
    if (w_trimmed.starts_with("s-") || w_trimmed.starts_with("t-")) && word_known(dict, &format!("'{w_trimmed}")) {
        return true;
    }
    // Hyphenated compounds (`woon-werkverkeer`, `e-mail`, `ex-collega`, `noord-zuid`): every part
    // must be a known word, closed compound, or single letter in `e-mail`.
    if w_trimmed.contains('-') {
        return w_trimmed.split('-').all(|p| {
            !p.is_empty()
                && (p.chars().count() == 1 && p.chars().all(char::is_alphabetic) || word_known(dict, p) || compound_known(dict, &p.to_lowercase()))
        });
    }
    compound_known(dict, &lower)
}

fn word_known(dict: &AffixDict, w: &str) -> bool {
    if dict.check(w) || crate::in_user_dictionary(&w.to_lowercase()) {
        return true;
    }
    // Dutch `IJ` digraph at the start of a sentence (`IJs`, `IJzer`, `IJskoud`): both `I` and `J`
    // are capitalized, so `AffixDict::casing` sees two capitals (`Mixed`), whereas in Dutch `IJ`
    // is a single initial capital.
    if let Some(rest) = w.strip_prefix("IJ")
        && !rest.is_empty()
        && rest.chars().all(|c| !c.is_uppercase())
    {
        return dict.contains(&format!("ij{rest}"));
    }
    false
}

/// Closed two-part Dutch compounds (`spellingcontrole`, `samenwerkingsverband`, `werkverkeer`):
/// both parts must be known lowercase words of sufficient length (left >= 4 chars, right >= 5
/// chars), with an optional linking `-s-`.
fn compound_known(dict: &AffixDict, lower: &str) -> bool {
    let chars: Vec<char> = lower.chars().collect();
    if chars.len() >= 9 {
        for split in 4..=chars.len().saturating_sub(5) {
            let left: String = chars[..split].iter().collect();
            let right: String = chars[split..].iter().collect();
            if dict.contains(&left) && dict.contains(&right) {
                return true;
            }
            if left.ends_with('s')
                && left.chars().count() >= 5
                && let Some(left_stem) = left.strip_suffix('s')
                && dict.contains(left_stem)
                && dict.contains(&right)
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dutch_spelling_and_inflections() {
        test_dict::install();
        for ok in [
            "de",
            "het",
            "een",
            "huis",
            "huisje",
            "huizen",
            "boek",
            "boeken",
            "boekje",
            "fiets",
            "fietsen",
            "fietst",
            "fietste",
            "auto",
            "auto's",
            "foto's",
            "baby",
            "baby's",
            "baby'tje",
            "tafel",
            "tafels",
            "tafeltje",
            "idee",
            "ideeën",
            "ideetje",
            "mooi",
            "mooie",
            "mooier",
            "mooist",
            "mooiste",
            "gezellig",
            "ongezellig",
            "belangrijk",
            "belangrijke",
            "onbelangrijk",
            "gebeuren",
            "gebeurt",
            "gebeurde",
            "wordt",
            "werd",
            "geworden",
            "Nederland",
            "Nederlands",
            "Nederlandse",
            "Amsterdam",
            "België",
            "Alex'",
            "Anna's",
            "'s-Gravenhage",
            "woon-werkverkeer",
            "e-mail",
            "ex-collega",
            "spellingcontrole",
            "samenwerkingsverband",
            "café",
            "cafés",
            "één",
            "tweeën",
            "geïnteresseerd",
            "coördinatie",
            "industriële",
        ] {
            assert!(is_correct(ok), "expected '{ok}' to be accepted");
        }

        for bad in ["gebeurdt", "worddt", "vinddt", "autos", "fotos", "babys", "fietssen", "belangrek", "Nederalnd", "amsterdam", "belgie", "tweeen"]
        {
            assert!(!is_correct(bad), "expected '{bad}' to be rejected");
        }
    }

    #[test]
    fn dutch_suggestions() {
        test_dict::install();
        assert_eq!(suggest("amsterdam", 3).first().map(String::as_str), Some("Amsterdam"));
        assert!(suggest("belgie", 5).contains(&"België".to_string()));
        assert!(suggest("cafe", 5).contains(&"café".to_string()));
        assert!(suggest("tweeen", 5).contains(&"tweeën".to_string()));
        assert!(suggest("gebeurdt", 5).contains(&"gebeurt".to_string()));
        assert!(suggest("worddt", 5).contains(&"wordt".to_string()));
        assert!(suggest("autos", 5).contains(&"auto's".to_string()));
        assert!(suggest("babytje", 5).contains(&"baby'tje".to_string()));
        assert!(suggest("nietwaar", 5).contains(&"niet waar".to_string()));
        assert!(suggest("inhet", 5).contains(&"in het".to_string()));
    }

    #[test]
    fn opentaal_flag_long_and_ij_ligature_are_supported() {
        let aff = "\
SET UTF-8
FLAG long
TRY esiaonrtld
FORBIDDENWORD Fw
NOSUGGEST NS
ONLYINCOMPOUND Cx
ICONV 2
ICONV ij ĳ
ICONV IJ Ĳ
OCONV 2
OCONV ĳ ij
OCONV Ĳ IJ
SFX Zb Y 2
SFX Zb 0 en/CaCbCp .
SFX Zb 0 en-/CaCbCp .
SFX Aa Y 1
SFX Aa 0 e .
";
        let dic = "\
5
woordenboek/Zb
belangrĳk/Aa
gebeurdt/Fw
scheldwoord/NS
alleencomp/CxZb
";
        let d = parse_hunspell(aff, dic).unwrap();
        assert!(is_correct_with(&d, "woordenboek"));
        assert!(is_correct_with(&d, "woordenboeken"));
        assert!(!d.contains("woordenboeken-"));
        assert!(is_correct_with(&d, "belangrijk"));
        assert!(is_correct_with(&d, "belangrijke"));
        assert!(!is_correct_with(&d, "gebeurdt"));
        assert!(!is_correct_with(&d, "alleencomp"));
        assert!(!d.contains_suggestible("scheldwoord"));
    }

    #[test]
    fn unreadable_dictionary_records_load_error() {
        assert!(install_dictionary("FLAG long\n", "1\nhuis\n").is_err());
        assert!(crate::dictionary_load_error(crate::ProofLang::Nl).is_some());
        test_dict::install();
        assert!(crate::dictionary_load_error(crate::ProofLang::Nl).is_none());
    }

    #[test]
    fn installed_opentaal_dictionary_loads_when_present() {
        let Some((aff, dic)) = hunspell::find(NAMES) else { return };
        // Skip when `hunspell::find` returned the synthetic temp folder from `dictionaries_folder_is_searched_first`.
        if aff.to_string_lossy().contains("wc-proof-dictionaries-") {
            return;
        }
        let aff_text = read_utf8_or_latin1(&aff).unwrap();
        let dic_text = read_utf8_or_latin1(&dic).unwrap();
        let d = parse_hunspell(&aff_text, &dic_text).unwrap();
        assert!(d.len() > 100_000, "expected real Dutch dictionary, got {} stems", d.len());
        for ok in ["Nederland", "Nederlandse", "woordenboek", "woordenboeken", "belangrijk", "belangrijke", "auto's", "gezellig"] {
            assert!(is_correct_with(&d, ok), "expected '{ok}' to be accepted by system nl_NL");
        }
        for bad in ["Nederalnd", "woordenboekk", "belangrek"] {
            assert!(!is_correct_with(&d, bad), "expected '{bad}' to be rejected by system nl_NL");
        }
    }
}
