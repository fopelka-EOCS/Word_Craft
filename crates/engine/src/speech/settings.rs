//! Read Aloud settings: how much is read at one time and the text rules.
//!
//! `readAloud.settings` and `ui.json` both go through [`SpeechSettings::apply`] and
//! [`SpeechSettings::to_json`]. The values come from agents and from files, so they are hostile:
//! every string is capped, every number clamped, and a bad value is an error that changes nothing.

use serde_json::{Map, Value, json};

use crate::CmdError;

/// Most replacements, and the longest side of one, in characters.
pub const MAX_REPLACEMENTS: usize = 200;
const MAX_REPLACEMENT_CHARS: usize = 100;
/// Most skip patterns, and the longest one, in characters.
pub const MAX_PATTERNS: usize = 50;
const MAX_PATTERN_CHARS: usize = 200;
/// Most characters in the "characters to skip" list.
const MAX_SKIP_CHARS: usize = 200;
/// Compiled size limit of one skip pattern (the `regex` crate refuses larger ones).
const PATTERN_SIZE_LIMIT: usize = 1 << 20;

/// How much is read at one time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Chunk {
    #[default]
    Paragraph,
    Page,
    Document,
}

/// What happens to web addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlMode {
    /// Read them as written.
    Read,
    /// Read the site name only (`https://www.example.com/a?b` → `example.com`).
    Domain,
    /// Leave them out.
    Skip,
}

macro_rules! names {
    ($t:ty { $($v:ident = $n:literal),* $(,)? }) => {
        impl $t {
            pub fn name(self) -> &'static str {
                match self { $(<$t>::$v => $n),* }
            }
            pub fn parse(s: &str) -> Option<Self> {
                match s.trim().to_ascii_lowercase().as_str() { $(x if x == $n.to_ascii_lowercase() => Some(<$t>::$v),)* _ => None }
            }
            const NAMES: &'static [&'static str] = &[$($n),*];
        }
    };
}
names!(Chunk { Paragraph = "paragraph", Page = "page", Document = "document" });
names!(UrlMode { Read = "read", Domain = "domain", Skip = "skip" });

/// What is left out of, or changed in, the spoken text. See [`super::clean::clean`].
#[derive(Clone, Debug)]
pub struct TextRules {
    pub urls: UrlMode,
    pub skip_emoji: bool,
    pub skip_symbols: bool,
    pub skip_markup: bool,
    pub skip_brackets: bool,
    pub skip_parentheses: bool,
    pub collapse_repeats: bool,
    pub skip_chars: String,
    /// (from, to), applied in order, literally.
    pub replacements: Vec<(String, String)>,
    pub skip_patterns: Vec<String>,
    /// `skip_patterns`, compiled.
    pub(crate) compiled: Vec<regex::Regex>,
}

impl Default for TextRules {
    fn default() -> Self {
        TextRules {
            urls: UrlMode::Domain,
            skip_emoji: false,
            skip_symbols: true,
            skip_markup: false,
            skip_brackets: false,
            skip_parentheses: false,
            collapse_repeats: true,
            skip_chars: String::new(),
            replacements: Vec::new(),
            skip_patterns: Vec::new(),
            compiled: Vec::new(),
        }
    }
}

impl PartialEq for TextRules {
    fn eq(&self, o: &Self) -> bool {
        (self.urls, self.skip_emoji, self.skip_symbols, self.skip_markup, self.skip_brackets, self.skip_parentheses, self.collapse_repeats)
            == (o.urls, o.skip_emoji, o.skip_symbols, o.skip_markup, o.skip_brackets, o.skip_parentheses, o.collapse_repeats)
            && self.skip_chars == o.skip_chars
            && self.replacements == o.replacements
            && self.skip_patterns == o.skip_patterns
    }
}

/// Every Read Aloud setting.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SpeechSettings {
    pub chunk: Chunk,
    pub text: TextRules,
}

fn bad(msg: impl Into<String>) -> CmdError {
    CmdError::Params(msg.into())
}

/// A string setting: `None` when absent or null; at most `max` characters, control characters
/// removed, trimmed.
fn string(v: &Value, key: &str, max: usize) -> Result<Option<String>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            if s.chars().count() > max {
                return Err(bad(format!("`{key}` is longer than {max} characters")));
            }
            Ok(Some(s.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string()))
        }
        Some(_) => Err(bad(format!("`{key}` must be a string"))),
    }
}

fn boolean(v: &Value, key: &str) -> Result<Option<bool>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(bad(format!("`{key}` must be true or false"))),
    }
}

fn choice<T>(v: &Value, key: &str, parse: fn(&str) -> Option<T>, names: &[&str]) -> Result<Option<T>, CmdError> {
    match string(v, key, 32)? {
        None => Ok(None),
        Some(s) => parse(&s).map(Some).ok_or_else(|| bad(format!("`{key}` must be one of {}", names.join(", ")))),
    }
}

fn object<'a>(v: &'a Value, key: &str) -> Result<Option<&'a Map<String, Value>>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(m)) => Ok(Some(m)),
        Some(_) => Err(bad(format!("`{key}` must be an object"))),
    }
}

fn array<'a>(v: &'a Value, key: &str, max: usize) -> Result<Option<&'a Vec<Value>>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.len() <= max => Ok(Some(a)),
        Some(Value::Array(_)) => Err(bad(format!("`{key}` has more than {max} entries"))),
        Some(_) => Err(bad(format!("`{key}` must be a list"))),
    }
}

impl SpeechSettings {
    /// Change the settings `v` names; anything else stays. A bad value is an error, and then
    /// nothing changes.
    pub fn apply(&mut self, v: &Value) -> Result<(), CmdError> {
        if !v.is_object() {
            return Err(bad("the settings must be an object"));
        }
        let mut s = self.clone();
        if let Some(c) = choice(v, "chunk", Chunk::parse, Chunk::NAMES)? {
            s.chunk = c;
        }
        if let Some(t) = object(v, "text")? {
            s.text = text_rules(&Value::Object(t.clone()), s.text)?;
        }
        *self = s;
        Ok(())
    }

    /// Settings saved by [`SpeechSettings::to_json`]: each section that reads well is kept, a bad
    /// one keeps its defaults (a damaged `ui.json` never stops the app).
    pub fn load(v: &Value) -> SpeechSettings {
        let mut s = SpeechSettings::default();
        if let Some(m) = v.as_object() {
            for (k, x) in m {
                let mut one = Map::new();
                one.insert(k.clone(), x.clone());
                if s.apply(&Value::Object(one)).is_err() {
                    log::warn!("Read Aloud setting `{k}` ignored");
                }
            }
        }
        s
    }

    pub fn to_json(&self) -> Value {
        let t = &self.text;
        json!({
            "chunk": self.chunk.name(),
            "text": {
                "urls": t.urls.name(),
                "skipEmoji": t.skip_emoji,
                "skipSymbols": t.skip_symbols,
                "skipMarkup": t.skip_markup,
                "skipBrackets": t.skip_brackets,
                "skipParentheses": t.skip_parentheses,
                "collapseRepeats": t.collapse_repeats,
                "skipChars": t.skip_chars,
                "replacements": t.replacements.iter().map(|(f, to)| json!({"from": f, "to": to})).collect::<Vec<_>>(),
                "skipPatterns": t.skip_patterns,
            },
        })
    }
}

fn text_rules(v: &Value, mut t: TextRules) -> Result<TextRules, CmdError> {
    if let Some(u) = choice(v, "urls", UrlMode::parse, UrlMode::NAMES)? {
        t.urls = u;
    }
    for (key, field) in [
        ("skipEmoji", &mut t.skip_emoji),
        ("skipSymbols", &mut t.skip_symbols),
        ("skipMarkup", &mut t.skip_markup),
        ("skipBrackets", &mut t.skip_brackets),
        ("skipParentheses", &mut t.skip_parentheses),
        ("collapseRepeats", &mut t.collapse_repeats),
    ] {
        if let Some(b) = boolean(v, key)? {
            *field = b;
        }
    }
    // Not trimmed: a space is a character too.
    match v.get("skipChars") {
        None | Some(Value::Null) => {}
        Some(Value::String(s)) if s.chars().count() <= MAX_SKIP_CHARS => {
            let mut seen = String::new();
            for c in s.chars().filter(|c| !c.is_control()) {
                if !seen.contains(c) {
                    seen.push(c);
                }
            }
            t.skip_chars = seen;
        }
        Some(_) => return Err(bad(format!("`skipChars` must be a string of at most {MAX_SKIP_CHARS} characters"))),
    }
    if let Some(a) = array(v, "replacements", MAX_REPLACEMENTS)? {
        let mut out = Vec::new();
        for x in a {
            let side = |k: &str| match x.get(k) {
                Some(Value::String(s)) if s.chars().count() <= MAX_REPLACEMENT_CHARS && !s.chars().any(char::is_control) => Some(s.clone()),
                None | Some(Value::Null) if k == "to" => Some(String::new()),
                _ => None,
            };
            match (side("from"), side("to")) {
                (Some(f), Some(to)) if !f.is_empty() => out.push((f, to)),
                _ => return Err(bad(format!("each replacement is {{\"from\": text, \"to\": text}}, at most {MAX_REPLACEMENT_CHARS} characters"))),
            }
        }
        t.replacements = out;
    }
    if let Some(a) = array(v, "skipPatterns", MAX_PATTERNS)? {
        let mut src = Vec::new();
        let mut compiled = Vec::new();
        for x in a {
            let Some(p) = x.as_str().filter(|p| !p.is_empty() && p.chars().count() <= MAX_PATTERN_CHARS) else {
                return Err(bad(format!("each skip pattern is a regular expression of at most {MAX_PATTERN_CHARS} characters")));
            };
            let re = regex::RegexBuilder::new(p)
                .size_limit(PATTERN_SIZE_LIMIT)
                .dfa_size_limit(PATTERN_SIZE_LIMIT)
                .build()
                .map_err(|e| bad(format!("skip pattern `{p}`: {e}")))?;
            src.push(p.to_string());
            compiled.push(re);
        }
        t.skip_patterns = src;
        t.compiled = compiled;
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn defaults_round_trip() {
        let s = SpeechSettings::default();
        assert_eq!(s.chunk, Chunk::Paragraph);
        assert_eq!(SpeechSettings::load(&s.to_json()), s);
    }

    #[test]
    fn apply_changes_only_what_is_named() {
        let mut s = SpeechSettings::default();
        s.apply(&json!({"chunk": "page"})).unwrap();
        assert_eq!(s.chunk, Chunk::Page);
        s.apply(&json!({"text": {"skipEmoji": true}})).unwrap();
        assert!(s.text.skip_emoji && s.text.skip_symbols);
        let back = SpeechSettings::load(&s.to_json());
        assert_eq!(back, s);
    }

    #[test]
    fn a_bad_value_changes_nothing() {
        let mut s = SpeechSettings::default();
        for bad in [
            json!(5),
            json!({"chunk": "chapter"}),
            json!({"chunk": 3}),
            json!({"text": {"skipPatterns": ["("]}}),
            json!({"text": {"skipPatterns": ["a{1000}{1000}{1000}"]}}),
            json!({"text": {"replacements": [{"from": "", "to": "x"}]}}),
            json!({"text": {"replacements": [{"from": "x".repeat(101), "to": "x"}]}}),
            json!({"text": {"skipEmoji": "yes"}}),
        ] {
            assert!(s.apply(&bad).is_err(), "{bad}");
            assert_eq!(s, SpeechSettings::default(), "{bad}");
        }
    }

    #[test]
    fn a_damaged_file_keeps_the_good_sections() {
        let s = SpeechSettings::load(&json!({"chunk": {"x": 1}, "voices": "junk", "text": {"skipEmoji": true}}));
        assert_eq!(s.chunk, Chunk::Paragraph);
        assert!(s.text.skip_emoji);
        assert_eq!(SpeechSettings::load(&json!(null)), SpeechSettings::default());
    }
}
