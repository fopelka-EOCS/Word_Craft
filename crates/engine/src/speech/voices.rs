//! Voice markers: `§A Hello.` makes Voice A read "Hello.". The marker character (`§` by default)
//! and a role label switch the voice; the label may end with `:`, and the marker, the label, the
//! `:` and the spaces after them are not spoken. A label that names no role leaves the text as it
//! is, so `§5` in a legal text is read as written.

use super::settings::{MAX_LABEL, Voices};

/// The voice role of each character of a paragraph, and which characters are spoken.
#[derive(Debug, PartialEq)]
pub struct Marked {
    pub roles: Vec<usize>,
    pub spoken: Vec<bool>,
    /// The role after the last character.
    pub end: usize,
}

/// Find the voice markers in `chars`, which start in role `role`.
pub fn mark(chars: &[char], voices: &Voices, mut role: usize) -> Marked {
    let n = chars.len();
    let mut roles = vec![role; n];
    let mut spoken = vec![true; n];
    if !voices.enabled {
        return Marked { roles, spoken, end: role };
    }
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        if c != voices.marker {
            if let Some(r) = roles.get_mut(i) {
                *r = role;
            }
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j - i <= MAX_LABEL + 1 && chars.get(j).is_some_and(|c| c.is_alphanumeric() || *c == '_') {
            j += 1;
        }
        let label: String = chars.get(i + 1..j).unwrap_or(&[]).iter().collect();
        let switch = (j - i - 1 <= MAX_LABEL).then(|| voices.role_of(&label)).flatten();
        let Some(next) = switch else {
            // Not a marker: the character is read as written.
            if let Some(r) = roles.get_mut(i) {
                *r = role;
            }
            i += 1;
            continue;
        };
        if chars.get(j) == Some(&':') {
            j += 1;
        }
        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        for k in i..j {
            if let Some(s) = spoken.get_mut(k) {
                *s = false;
            }
            if let Some(r) = roles.get_mut(k) {
                *r = next;
            }
        }
        role = next;
        i = j;
    }
    Marked { roles, spoken, end: role }
}

/// The role a paragraph of `text` ends in, starting in `role` (for markers that last to the next one).
pub fn end_role(text: &str, voices: &Voices, role: usize) -> usize {
    if !voices.enabled || !text.contains(voices.marker) {
        return role;
    }
    let chars: Vec<char> = text.chars().collect();
    mark(&chars, voices, role).end
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::speech::SpeechSettings;

    /// The spoken text of each role, in order: (role, text).
    fn parts(text: &str, voices: &Voices) -> Vec<(usize, String)> {
        let chars: Vec<char> = text.chars().collect();
        let m = mark(&chars, voices, 0);
        let mut out: Vec<(usize, String)> = Vec::new();
        for ((c, r), s) in chars.iter().zip(&m.roles).zip(&m.spoken) {
            if !s {
                continue;
            }
            match out.last_mut() {
                Some((lr, t)) if lr == r => t.push(*c),
                _ => out.push((*r, c.to_string())),
            }
        }
        out
    }

    #[test]
    fn markers_switch_voices() {
        let v = Voices::default();
        assert_eq!(
            parts("Hello. §A Hi there! §b: Hey. §Narrator They left.", &v),
            [(0, "Hello. ".to_string()), (1, "Hi there! ".into()), (2, "Hey. ".into()), (0, "They left.".into())]
        );
        assert_eq!(parts("§N back §n again", &v), [(0, "back again".to_string())]);
    }

    #[test]
    fn unknown_labels_are_read_as_written() {
        let v = Voices::default();
        assert_eq!(parts("See §5 and §Zed and § alone and §", &v), [(0, "See §5 and §Zed and § alone and §".to_string())]);
        let long = format!("§{}", "A".repeat(MAX_LABEL + 5));
        assert_eq!(parts(&long, &v), [(0, long.clone())]);
    }

    #[test]
    fn a_custom_marker_and_roles() {
        let mut s = SpeechSettings::default();
        s.apply(&json!({"voices": {"marker": "→", "roles": [{"label": "Анна"}]}})).unwrap();
        assert_eq!(parts("→Анна Привет. →A not a role", &s.voices), [(1, "Привет. →A not a role".to_string())]);
        s.apply(&json!({"voices": {"enabled": false}})).unwrap();
        assert_eq!(parts("→Анна Привет.", &s.voices), [(0, "→Анна Привет.".to_string())]);
    }

    #[test]
    fn the_end_role_carries_on() {
        let v = Voices::default();
        assert_eq!(end_role("Plain text", &v, 2), 2);
        assert_eq!(end_role("§A starts §B ends", &v, 0), 2);
        assert_eq!(end_role("§N", &v, 3), 0);
    }
}
