//! Read Aloud settings: how much is read at one time.
//!
//! `readAloud.settings` and `ui.json` both go through [`SpeechSettings::apply`] and
//! [`SpeechSettings::to_json`]. The values come from agents and from files, so they are hostile:
//! every string is capped, every number clamped, and a bad value is an error that changes nothing.

use serde_json::{Map, Value, json};

use crate::CmdError;

/// How much is read at one time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Chunk {
    #[default]
    Paragraph,
    Page,
    Document,
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

/// Every Read Aloud setting.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SpeechSettings {
    pub chunk: Chunk,
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

fn choice<T>(v: &Value, key: &str, parse: fn(&str) -> Option<T>, names: &[&str]) -> Result<Option<T>, CmdError> {
    match string(v, key, 32)? {
        None => Ok(None),
        Some(s) => parse(&s).map(Some).ok_or_else(|| bad(format!("`{key}` must be one of {}", names.join(", ")))),
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
        json!({
            "chunk": self.chunk.name(),
        })
    }
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
        let back = SpeechSettings::load(&s.to_json());
        assert_eq!(back, s);
    }

    #[test]
    fn a_bad_value_changes_nothing() {
        let mut s = SpeechSettings::default();
        for bad in [json!(5), json!({"chunk": "chapter"}), json!({"chunk": 3})] {
            assert!(s.apply(&bad).is_err(), "{bad}");
            assert_eq!(s, SpeechSettings::default(), "{bad}");
        }
    }

    #[test]
    fn a_damaged_file_keeps_the_good_sections() {
        let s = SpeechSettings::load(&json!({"chunk": {"x": 1}, "other": 5}));
        assert_eq!(s.chunk, Chunk::Paragraph);
        let s = SpeechSettings::load(&json!({"chunk": "page", "other": 5}));
        assert_eq!(s.chunk, Chunk::Page);
        assert_eq!(SpeechSettings::load(&json!(null)), SpeechSettings::default());
    }
}
