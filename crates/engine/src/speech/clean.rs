//! The text rules: what Read Aloud leaves out of a sentence, or changes, before a voice speaks it.
//! Invisible characters always go; the rest follows [`TextRules`]. The page positions of the
//! sentence don't change, so the highlight stays on the words as written.

use super::settings::{TextRules, UrlMode};

/// Longest cleaned sentence, in bytes (replacements can make a sentence longer).
const MAX_CLEAN: usize = 20_000;

/// Characters that show nothing and say nothing: zero-width spaces and joiners, direction
/// marks, the soft hyphen, the byte order mark, variation selectors.
fn invisible(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}' | '\u{FE00}'..='\u{FE0F}')
}

/// Emoji, their skin tones, flags, keycaps and tag characters.
fn emoji(c: char) -> bool {
    matches!(c,
        '\u{1F000}'..='\u{1FAFF}' | '\u{2600}'..='\u{27BF}' | '\u{2B50}' | '\u{2B55}' | '\u{231A}' | '\u{231B}' | '\u{23E9}'..='\u{23FA}'
        | '\u{20E3}' | '\u{E0020}'..='\u{E007F}' | '\u{3030}' | '\u{303D}' | '\u{3297}' | '\u{3299}')
}

/// Decorative symbols a voice reads by name ("black right-pointing triangle"): arrows, technical
/// symbols, box drawing, blocks and shapes, dingbats, bullets, private-use (icon font) characters.
fn symbol(c: char) -> bool {
    matches!(c,
        '\u{2190}'..='\u{21FF}' | '\u{2300}'..='\u{23FF}' | '\u{2500}'..='\u{25FF}' | '\u{2600}'..='\u{27BF}' | '\u{2900}'..='\u{297F}'
        | '\u{2B00}'..='\u{2BFF}' | '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{10FFFF}' | '•' | '‣' | '⁃' | '∙' | '·')
}

/// Plain-text markup characters (Markdown, code, tables drawn with text).
fn markup(c: char) -> bool {
    matches!(c, '*' | '#' | '_' | '~' | '|' | '^' | '`' | '<' | '>' | '=' | '\\')
}

/// A web address: a word that starts with a scheme or `www.`.
fn is_url(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    ["http://", "https://", "ftp://", "www."].iter().any(|p| w.starts_with(p) && w.len() > p.len())
}

/// The site name of a web address: no scheme, no `www.`, no path, query or port.
fn domain(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.strip_prefix("www.").or_else(|| rest.strip_prefix("WWW.")).unwrap_or(rest);
    rest.split(['/', '?', '#', ':']).next().unwrap_or(rest)
}

/// Web addresses read, shortened to their site, or left out. Punctuation after an address stays.
fn urls(s: &str, mode: UrlMode) -> String {
    if mode == UrlMode::Read {
        return s.to_string();
    }
    let mut out = Vec::new();
    for word in s.split_whitespace() {
        // The address without the punctuation that ends the sentence or clause after it.
        let core = word.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'', '”', '’']);
        let tail = word.get(core.len()..).unwrap_or("");
        let core_l = core.trim_start_matches(['(', '[', '"', '\'', '“', '‘']);
        let head = core.get(..core.len() - core_l.len()).unwrap_or("");
        if !is_url(core_l) {
            out.push(word.to_string());
            continue;
        }
        match mode {
            UrlMode::Skip => out.push(format!("{head}{tail}")),
            _ => out.push(format!("{head}{}{tail}", domain(core_l))),
        }
    }
    out.join(" ")
}

/// `s` without the text between `open` and its `close` (nesting counted). An `open` without a
/// `close` keeps the rest.
fn without_enclosed(s: &str, open: char, close: char) -> String {
    let mut out = String::with_capacity(s.len());
    let mut held = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        if c == open {
            depth += 1;
            held.push(c);
        } else if c == close && depth > 0 {
            depth -= 1;
            if depth == 0 {
                held.clear();
                out.push(' ');
            } else {
                held.push(c);
            }
        } else if depth > 0 {
            held.push(c);
        } else {
            out.push(c);
        }
    }
    out.push_str(&held);
    out
}

/// Runs of four or more of the same punctuation or symbol (`*****`, `-----`, `!!!!`) left out.
fn collapse_repeats(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        let mut j = i + 1;
        while chars.get(j) == Some(&c) {
            j += 1;
        }
        if j - i >= 4 && !c.is_alphanumeric() && !c.is_whitespace() {
            out.push(' ');
        } else {
            out.extend(chars.get(i..j).unwrap_or(&[]));
        }
        i = j;
    }
    out
}

/// Cut `s` to at most `max` bytes, at a character boundary.
fn cap(s: &mut String, max: usize) {
    if s.len() > max {
        let mut at = max;
        while !s.is_char_boundary(at) {
            at -= 1;
        }
        s.truncate(at);
    }
}

/// The sentence as the voice should get it, under `rules`.
pub fn clean(s: &str, rules: &TextRules) -> String {
    let mut t: String = s.chars().map(|c| if invisible(c) && c.is_whitespace() { ' ' } else { c }).filter(|c| !invisible(*c)).collect();
    for (from, to) in &rules.replacements {
        if t.contains(from.as_str()) {
            t = t.replace(from.as_str(), to);
            cap(&mut t, MAX_CLEAN);
        }
    }
    for re in &rules.compiled {
        t = re.replace_all(&t, " ").into_owned();
    }
    t = urls(&t, rules.urls);
    if rules.skip_brackets {
        t = without_enclosed(&t, '[', ']');
    }
    if rules.skip_parentheses {
        t = without_enclosed(&t, '(', ')');
    }
    let drop = |c: char| {
        (rules.skip_emoji && (emoji(c) || matches!(c, '\u{200D}' | '\u{1F3FB}'..='\u{1F3FF}')))
            || (rules.skip_symbols && symbol(c))
            || rules.skip_chars.contains(c)
    };
    t = t.chars().map(|c| if rules.skip_markup && markup(c) { ' ' } else { c }).filter(|c| !drop(*c)).collect();
    if rules.collapse_repeats {
        t = collapse_repeats(&t);
    }
    super::tidy(&t)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::speech::SpeechSettings;

    fn rules(v: serde_json::Value) -> TextRules {
        let mut s = SpeechSettings::default();
        s.apply(&json!({ "text": v })).unwrap();
        s.text
    }

    #[test]
    fn invisible_characters_always_go() {
        let r = rules(json!({"skipSymbols": false, "collapseRepeats": false}));
        assert_eq!(clean("co\u{00AD}op\u{200B}er\u{FEFF}ate\u{200E}", &r), "cooperate");
    }

    #[test]
    fn web_addresses() {
        let t = "See https://www.example.com/a/b?c=1, or (www.wordcraft.org). Mail me.";
        assert_eq!(clean(t, &rules(json!({}))), "See example.com, or (wordcraft.org). Mail me.");
        assert_eq!(clean(t, &rules(json!({"urls": "skip"}))), "See, or. Mail me.");
        assert_eq!(clean(t, &rules(json!({"urls": "read"}))), t);
        // Not an address: no scheme, or a scheme alone.
        assert_eq!(clean("example.com and https:// and www.", &rules(json!({}))), "example.com and https:// and www.");
    }

    #[test]
    fn brackets_and_parentheses() {
        let r = rules(json!({"skipBrackets": true, "skipParentheses": true}));
        assert_eq!(clean("Yes [editor: check this] and (as [nested] said) done.", &r), "Yes and done.");
        // Unmatched: the rest is kept.
        assert_eq!(clean("Open [ never closed", &r), "Open [ never closed");
        assert_eq!(clean("Close ] alone", &r), "Close ] alone");
    }

    #[test]
    fn emoji_symbols_and_markup() {
        let r = rules(json!({"skipEmoji": true}));
        assert_eq!(clean("Great 👍🏽 job 👨‍👩‍👧 team 🇺🇸.", &r), "Great job team.");
        // Symbols are skipped by default: arrows, boxes, bullets, icon fonts.
        let d = rules(json!({}));
        assert_eq!(clean("Next → step ■ done • ok \u{F0B7}", &d), "Next step done ok");
        assert_eq!(clean("Next → step", &rules(json!({"skipSymbols": false}))), "Next → step");
        let m = rules(json!({"skipMarkup": true}));
        assert_eq!(clean("**Bold** and `code` | cell", &m), "Bold and code cell");
    }

    #[test]
    fn repeats_chars_replacements_and_patterns() {
        let d = rules(json!({}));
        assert_eq!(clean("Part one ***** Part two ----- end!!!!", &d), "Part one Part two end");
        // Three in a row stay (an ellipsis, `---`).
        assert_eq!(clean("Wait... ok", &d), "Wait... ok");
        let r = rules(json!({
            "skipChars": "~%",
            "replacements": [{"from": "&", "to": "and"}, {"from": "Dr.", "to": "Doctor"}],
            "skipPatterns": ["\\[\\d+\\]", "(?i)todo:"],
        }));
        assert_eq!(clean("Dr. Lee & ~me~ [12] TODO: 50% done", &r), "Doctor Lee and me 50 done");
    }

    #[test]
    fn hostile_text_is_bounded() {
        let r = rules(json!({"replacements": [{"from": "a", "to": "x".repeat(100)}]}));
        let out = clean(&"a".repeat(5_000), &r);
        assert!(out.len() <= MAX_CLEAN);
        let mut s = "é".repeat(MAX_CLEAN);
        cap(&mut s, MAX_CLEAN - 1);
        assert!(s.len() < MAX_CLEAN);
        assert_eq!(clean("", &rules(json!({}))), "");
        assert_eq!(clean("((((((((", &rules(json!({"skipParentheses": true}))), "");
    }
}
