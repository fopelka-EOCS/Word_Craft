//! Read Aloud settings: the voice engine, how much is read and rendered at one time, the text
//! rules and the voice roles.
//!
//! `readAloud.settings` and `ui.json` both go through [`SpeechSettings::apply`] and
//! [`SpeechSettings::to_json`]. The values come from agents and from files, so they are hostile:
//! every string is capped, every number clamped, and a bad value is an error that changes nothing.
//! The cloud API key is never kept here: only the name of the environment variable that holds it.

use serde_json::{Map, Value, json};

use crate::CmdError;

/// Most voice roles after the Narrator (Voice A … Voice Z).
pub const MAX_ROLES: usize = 26;
/// Longest role label (`A`, `Anna`, `Villain_2`), in characters.
pub const MAX_LABEL: usize = 24;
/// Longest voice name, in characters.
const MAX_VOICE: usize = 64;
/// Longest program, path, endpoint or argument, in characters.
const MAX_FIELD: usize = 2048;
/// Most arguments of the local voice program.
const MAX_ARGS: usize = 32;
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
/// Speed of one voice role relative to the reading speed.
pub const MIN_ROLE_RATE: f32 = 0.5;
pub const MAX_ROLE_RATE: f32 = 2.0;

/// The program that speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Engine {
    /// The system's speech program (`say`, `espeak-ng`, Windows speech).
    #[default]
    System,
    /// A local neural voice program, Piper by default; it writes a WAV file.
    Local,
    /// An OpenAI-compatible `/v1/audio/speech` service.
    Cloud,
}

/// How much is read, and rendered ahead, at one time.
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

/// How long a voice marker stays in effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// To the end of the paragraph; the next paragraph starts with the Narrator.
    Paragraph,
    /// To the next marker, across paragraphs.
    UntilNext,
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
names!(Engine { System = "system", Local = "local", Cloud = "cloud" });
names!(Chunk { Paragraph = "paragraph", Page = "page", Document = "document" });
names!(UrlMode { Read = "read", Domain = "domain", Skip = "skip" });
names!(Scope { Paragraph = "paragraph", UntilNext = "untilNext" });

/// The local voice program.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalVoice {
    pub program: String,
    /// Arguments, with `{voice}`, `{output}`, `{rate}` and `{lengthScale}` filled in for each sentence.
    pub args: Vec<String>,
    /// The default voice (for Piper, the path of a `.onnx` voice model).
    pub voice: String,
}

impl Default for LocalVoice {
    fn default() -> Self {
        LocalVoice {
            program: "piper".into(),
            args: ["--model", "{voice}", "--output_file", "{output}", "--length_scale", "{lengthScale}"].map(String::from).to_vec(),
            voice: String::new(),
        }
    }
}

/// The cloud voice service.
#[derive(Clone, Debug, PartialEq)]
pub struct CloudVoice {
    pub endpoint: String,
    pub model: String,
    /// The default voice.
    pub voice: String,
    /// The environment variable that holds the API key.
    pub api_key_env: String,
}

impl Default for CloudVoice {
    fn default() -> Self {
        CloudVoice {
            endpoint: "https://api.openai.com/v1/audio/speech".into(),
            model: "tts-1".into(),
            voice: "alloy".into(),
            api_key_env: "OPENAI_API_KEY".into(),
        }
    }
}

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

/// One voice role: the Narrator (role 0) or Voice A, B, C…
#[derive(Clone, Debug, PartialEq)]
pub struct Role {
    pub label: String,
    /// The voice for each engine; empty keeps the engine's default voice.
    pub system: String,
    pub local: String,
    pub cloud: String,
    /// Speed relative to the reading speed.
    pub rate: f32,
}

impl Role {
    fn new(label: &str) -> Role {
        Role { label: label.into(), system: String::new(), local: String::new(), cloud: String::new(), rate: 1.0 }
    }
}

/// The label of role 0.
pub const NARRATOR: &str = "Narrator";

/// Voice roles and the marker that switches between them.
#[derive(Clone, Debug, PartialEq)]
pub struct Voices {
    pub enabled: bool,
    pub marker: char,
    pub scope: Scope,
    /// Role 0 is the Narrator; the others are switched to with the marker and their label.
    pub roles: Vec<Role>,
}

impl Default for Voices {
    fn default() -> Self {
        Voices { enabled: true, marker: '§', scope: Scope::Paragraph, roles: [NARRATOR, "A", "B", "C"].map(Role::new).to_vec() }
    }
}

impl Voices {
    /// The role a marker label names: `Narrator` or `N` for role 0, else a role's label, any case.
    pub fn role_of(&self, label: &str) -> Option<usize> {
        if label.eq_ignore_ascii_case(NARRATOR) || label.eq_ignore_ascii_case("n") {
            return Some(0);
        }
        self.roles.iter().enumerate().skip(1).find(|(_, r)| r.label.to_lowercase() == label.to_lowercase()).map(|(i, _)| i)
    }

    /// The label to show for a role.
    pub fn label(&self, role: usize) -> &str {
        self.roles.get(role).map_or(NARRATOR, |r| r.label.as_str())
    }
}

/// Every Read Aloud setting.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SpeechSettings {
    pub engine: Engine,
    pub chunk: Chunk,
    pub local: LocalVoice,
    pub cloud: CloudVoice,
    pub text: TextRules,
    pub voices: Voices,
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

/// A voice name or path handed to a program as one argument: it may not look like an option.
fn voice_name(v: &Value, key: &str, max: usize) -> Result<Option<String>, CmdError> {
    let s = string(v, key, max)?;
    if s.as_deref().is_some_and(|s| s.starts_with('-')) {
        return Err(bad(format!("`{key}` may not start with `-`")));
    }
    Ok(s)
}

/// Is `url` an endpoint text may be sent to: `https://`, or `http://` to this computer only.
pub fn endpoint_ok(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|p| lower.strip_prefix(p).is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/'])));
    (lower.starts_with("https://") && lower.len() > 8 || local) && !url.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"')
}

/// Is `name` a plain environment variable name (`OPENAI_API_KEY`)?
pub fn env_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Is `label` a role label: letters, digits and `_`, 1 to [`MAX_LABEL`] characters.
pub fn label_ok(label: &str) -> bool {
    let n = label.chars().count();
    (1..=MAX_LABEL).contains(&n) && label.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn rate(v: &Value, key: &str) -> Result<Option<f32>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => match x.as_f64().filter(|f| f.is_finite()) {
            Some(f) => Ok(Some((f as f32).clamp(MIN_ROLE_RATE, MAX_ROLE_RATE))),
            None => Err(bad(format!("`{key}` must be a number"))),
        },
    }
}

/// Is `path` a voice a command may set for the local voice program: letters, digits and
/// `. _ / \ : -` only, so it can't mean anything else to a shell in the argument template.
pub fn command_path_ok(path: &str) -> bool {
    path.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '/' | '\\' | ':' | '-'))
}

/// Who changes the settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The user, in File › Options, or the settings saved in `ui.json`.
    User,
    /// A command (`readAloud.settings` from the control channel, MCP, the CLI or a script). A
    /// command may not choose the program WordCraft runs (`local.program`, `local.args`), where
    /// text and the API key go (`cloud.endpoint`, `cloud.apiKeyEnv`), or switch the cloud voice
    /// on: any client that can send commands could otherwise run any program, or send the
    /// document and the key to its own server.
    Command,
}

impl SpeechSettings {
    /// Change the settings `v` names, as a command ([`Origin::Command`]); anything else stays. A
    /// bad value is an error, and then nothing changes.
    pub fn apply(&mut self, v: &Value) -> Result<(), CmdError> {
        self.apply_from(v, Origin::Command)
    }

    /// [`SpeechSettings::apply`] for `origin`.
    pub fn apply_from(&mut self, v: &Value, origin: Origin) -> Result<(), CmdError> {
        if !v.is_object() {
            return Err(bad("the settings must be an object"));
        }
        let mut s = self.clone();
        if let Some(e) = choice(v, "engine", Engine::parse, Engine::NAMES)? {
            s.engine = e;
        }
        if let Some(c) = choice(v, "chunk", Chunk::parse, Chunk::NAMES)? {
            s.chunk = c;
        }
        if let Some(l) = object(v, "local")? {
            let l = Value::Object(l.clone());
            if let Some(p) = voice_name(&l, "program", MAX_FIELD)? {
                if p.is_empty() {
                    return Err(bad("`local.program` is empty"));
                }
                s.local.program = p;
            }
            if let Some(a) = array(&l, "args", MAX_ARGS)? {
                let mut args = Vec::new();
                for x in a {
                    match x.as_str() {
                        Some(t) if t.chars().count() <= MAX_FIELD && !t.chars().any(char::is_control) => args.push(t.to_string()),
                        _ => return Err(bad("`local.args` must be a list of short strings")),
                    }
                }
                s.local.args = args;
            }
            if let Some(x) = voice_name(&l, "voice", MAX_FIELD)? {
                s.local.voice = x;
            }
        }
        if let Some(c) = object(v, "cloud")? {
            let c = Value::Object(c.clone());
            if let Some(e) = string(&c, "endpoint", MAX_FIELD)? {
                if !endpoint_ok(&e) {
                    return Err(bad("`cloud.endpoint` must start with https:// (or http://localhost)"));
                }
                s.cloud.endpoint = e;
            }
            if let Some(m) = voice_name(&c, "model", MAX_VOICE)? {
                s.cloud.model = m;
            }
            if let Some(x) = voice_name(&c, "voice", MAX_VOICE)? {
                s.cloud.voice = x;
            }
            if let Some(k) = string(&c, "apiKeyEnv", 64)? {
                if !env_name_ok(&k) {
                    return Err(bad("`cloud.apiKeyEnv` must be an environment variable name such as OPENAI_API_KEY"));
                }
                s.cloud.api_key_env = k;
            }
        }
        if let Some(t) = object(v, "text")? {
            s.text = text_rules(&Value::Object(t.clone()), s.text)?;
        }
        if let Some(x) = object(v, "voices")? {
            s.voices = voices(&Value::Object(x.clone()), s.voices)?;
        }
        if origin == Origin::Command {
            // The same values are fine: an agent may send back the settings it read.
            if s.local.program != self.local.program || s.local.args != self.local.args {
                return Err(bad("`local.program` and `local.args` choose the program WordCraft runs: change them in File › Options › Accessibility"));
            }
            // A voice goes into the program's arguments, which may be a shell's (`sh -c …`): a new
            // one from a command is a plain name or path only.
            let known: Vec<&str> = self.voices.roles.iter().map(|r| r.local.as_str()).chain([self.local.voice.as_str()]).collect();
            let new = s.voices.roles.iter().map(|r| r.local.as_str()).chain([s.local.voice.as_str()]);
            if let Some(v) = new.filter(|v| !known.contains(v)).find(|v| !command_path_ok(v)) {
                return Err(bad(format!(
                    "the voice `{v}` may only have letters, digits and . _ / \\ : - when set by a command: set it in File › Options › Accessibility"
                )));
            }
            if s.cloud.endpoint != self.cloud.endpoint || s.cloud.api_key_env != self.cloud.api_key_env {
                return Err(bad(
                    "`cloud.endpoint` and `cloud.apiKeyEnv` choose where the text and the API key go: change them in File › Options › Accessibility",
                ));
            }
            if s.engine == Engine::Cloud && self.engine != Engine::Cloud {
                return Err(bad("the cloud voice sends the document's text to a service: switch it on in File › Options › Accessibility"));
            }
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
                if s.apply_from(&Value::Object(one), Origin::User).is_err() {
                    log::warn!("Read Aloud setting `{k}` ignored");
                }
            }
        }
        s
    }

    pub fn to_json(&self) -> Value {
        let role = |r: &Role| json!({"label": r.label, "system": r.system, "local": r.local, "cloud": r.cloud, "rate": r.rate});
        let t = &self.text;
        json!({
            "engine": self.engine.name(),
            "chunk": self.chunk.name(),
            "local": {"program": self.local.program, "args": self.local.args, "voice": self.local.voice},
            "cloud": {"endpoint": self.cloud.endpoint, "model": self.cloud.model, "voice": self.cloud.voice, "apiKeyEnv": self.cloud.api_key_env},
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
            "voices": {
                "enabled": self.voices.enabled,
                "marker": self.voices.marker.to_string(),
                "scope": self.voices.scope.name(),
                "narrator": self.voices.roles.first().map(role),
                "roles": self.voices.roles.iter().skip(1).map(role).collect::<Vec<_>>(),
            },
        })
    }

    /// The voice and speed factor of `role` for the current engine (an empty voice: the
    /// engine's default).
    pub fn role_voice(&self, role: usize) -> (&str, f32) {
        let r = self.voices.roles.get(role);
        let voice = r.map_or("", |r| match self.engine {
            Engine::System => r.system.as_str(),
            Engine::Local => r.local.as_str(),
            Engine::Cloud => r.cloud.as_str(),
        });
        (voice, r.map_or(1.0, |r| r.rate))
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

fn role(v: &Value, mut r: Role) -> Result<Role, CmdError> {
    if let Some(x) = voice_name(v, "system", MAX_VOICE)? {
        r.system = x;
    }
    if let Some(x) = voice_name(v, "local", MAX_FIELD)? {
        r.local = x;
    }
    if let Some(x) = voice_name(v, "cloud", MAX_VOICE)? {
        r.cloud = x;
    }
    if let Some(x) = rate(v, "rate")? {
        r.rate = x;
    }
    Ok(r)
}

fn voices(v: &Value, mut x: Voices) -> Result<Voices, CmdError> {
    if let Some(b) = boolean(v, "enabled")? {
        x.enabled = b;
    }
    match v.get("marker") {
        None | Some(Value::Null) => {}
        Some(Value::String(m)) => {
            let mut cs = m.chars();
            match (cs.next(), cs.next()) {
                (Some(c), None) if !c.is_alphanumeric() && !c.is_whitespace() && !c.is_control() && c != '_' && c != ':' => x.marker = c,
                _ => return Err(bad("`voices.marker` must be one character that is not a letter, a digit, a space, `_` or `:`")),
            }
        }
        Some(_) => return Err(bad("`voices.marker` must be a string")),
    }
    if let Some(sc) = choice(v, "scope", Scope::parse, Scope::NAMES)? {
        x.scope = sc;
    }
    if let Some(n) = object(v, "narrator")? {
        let old = x.roles.first().cloned().unwrap_or_else(|| Role::new(NARRATOR));
        let narrator = role(&Value::Object(n.clone()), old)?;
        match x.roles.first_mut() {
            Some(first) => *first = narrator,
            None => x.roles.push(narrator),
        }
    }
    if let Some(a) = array(v, "roles", MAX_ROLES)? {
        let mut roles = vec![x.roles.first().cloned().unwrap_or_else(|| Role::new(NARRATOR))];
        for r in a {
            let label = string(r, "label", MAX_LABEL)?.unwrap_or_default();
            if !label_ok(&label) || x.role_of_label_in(&roles, &label) {
                return Err(bad(format!("role label `{label}` must be 1 to {MAX_LABEL} letters or digits, unique, and not Narrator or N")));
            }
            roles.push(role(r, Role::new(&label))?);
        }
        x.roles = roles;
    }
    Ok(x)
}

impl Voices {
    /// Is `label` taken in `roles`, or reserved for the Narrator?
    fn role_of_label_in(&self, roles: &[Role], label: &str) -> bool {
        label.eq_ignore_ascii_case(NARRATOR)
            || label.eq_ignore_ascii_case("n")
            || roles.iter().skip(1).any(|r| r.label.to_lowercase() == label.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn defaults_round_trip() {
        let s = SpeechSettings::default();
        assert_eq!(s.engine, Engine::System);
        assert_eq!(s.chunk, Chunk::Paragraph);
        assert_eq!(s.voices.marker, '§');
        assert_eq!(SpeechSettings::load(&s.to_json()), s);
    }

    #[test]
    fn apply_changes_only_what_is_named() {
        let mut s = SpeechSettings::default();
        s.apply_from(&json!({"engine": "cloud", "chunk": "page", "cloud": {"voice": "nova"}}), Origin::User).unwrap();
        assert_eq!((s.engine, s.chunk, s.cloud.voice.as_str()), (Engine::Cloud, Chunk::Page, "nova"));
        assert_eq!(s.cloud.model, "tts-1");
        s.apply(&json!({"text": {"skipEmoji": true}})).unwrap();
        assert!(s.text.skip_emoji && s.text.skip_symbols);
        s.apply(&json!({"voices": {"marker": "^", "scope": "untilNext", "roles": [{"label": "Anna", "cloud": "shimmer", "rate": 9}]}})).unwrap();
        assert_eq!(s.voices.marker, '^');
        assert_eq!(s.voices.roles.len(), 2);
        assert_eq!(s.voices.role_of("anna"), Some(1));
        assert_eq!(s.voices.role_of("N"), Some(0));
        assert_eq!(s.voices.roles[1].rate, MAX_ROLE_RATE);
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
            json!({"engine": "telepathy"}),
            json!({"engine": "local", "chunk": 3}),
            json!({"cloud": {"endpoint": "http://example.com/speech"}}),
            json!({"cloud": {"endpoint": "file:///etc/passwd"}}),
            json!({"cloud": {"apiKeyEnv": "A B"}}),
            json!({"local": {"program": "--evil"}}),
            json!({"local": {"args": [1]}}),
            json!({"voices": {"marker": "ab"}}),
            json!({"voices": {"marker": "a"}}),
            json!({"voices": {"marker": " "}}),
            json!({"voices": {"roles": [{"label": "A"}, {"label": "a"}]}}),
            json!({"voices": {"roles": [{"label": "Narrator"}]}}),
            json!({"voices": {"roles": [{"label": "x y"}]}}),
            json!({"voices": {"roles": [{"label": "A", "system": "-v"}]}}),
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
    fn commands_cannot_choose_the_program_or_where_text_goes() {
        let mut s = SpeechSettings::default();
        for bad in [
            json!({"local": {"program": "/bin/sh"}}),
            json!({"local": {"args": ["-c", "rm -rf ~"]}}),
            json!({"cloud": {"endpoint": "https://attacker.example/v1/audio/speech"}}),
            json!({"cloud": {"apiKeyEnv": "AWS_SECRET_ACCESS_KEY"}}),
            json!({"engine": "cloud"}),
        ] {
            assert!(s.apply(&bad).is_err(), "{bad}");
            assert_eq!(s, SpeechSettings::default(), "{bad}");
        }
        // A new voice from a command is a plain path: nothing a shell would read.
        for bad in [
            json!({"local": {"voice": "x; rm -rf ~"}}),
            json!({"local": {"voice": "$(id)"}}),
            json!({"voices": {"roles": [{"label": "A", "local": "a`b`.onnx"}]}}),
            json!({"local": {"voice": "/my voices/a.onnx"}}),
        ] {
            assert!(s.apply(&bad).is_err(), "{bad}");
            assert_eq!(s, SpeechSettings::default(), "{bad}");
        }
        s.apply(&json!({"local": {"voice": "C:\\Voices\\en_US-amy.onnx"}, "voices": {"roles": [{"label": "A", "local": "/v/x-1_2.onnx"}]}})).unwrap();
        // The user may pick any path; a command may send it back unchanged.
        s.apply_from(&json!({"local": {"voice": "/my voices/a.onnx"}}), Origin::User).unwrap();
        s.apply(&s.to_json()).unwrap();
        s = SpeechSettings::default();
        // The settings as read back, unchanged, are fine, and so is everything else.
        s.apply(&SpeechSettings::default().to_json()).unwrap();
        s.apply(&json!({"engine": "local", "local": {"voice": "/voices/a.onnx"}, "cloud": {"voice": "nova", "model": "tts-1-hd"}})).unwrap();
        // The user may change them (File › Options, `ui.json`).
        s.apply_from(
            &json!({"engine": "cloud", "local": {"program": "/opt/piper/piper"}, "cloud": {"endpoint": "http://localhost:8000/v1/audio/speech"}}),
            Origin::User,
        )
        .unwrap();
        assert_eq!((s.engine, s.local.program.as_str()), (Engine::Cloud, "/opt/piper/piper"));
        assert_eq!(SpeechSettings::load(&s.to_json()), s);
        // Once the user has switched the cloud voice on, a command may leave it on, or switch it off.
        s.apply(&json!({"engine": "cloud", "chunk": "page"})).unwrap();
        s.apply(&json!({"engine": "system"})).unwrap();
        assert!(s.apply(&json!({"engine": "cloud"})).is_err());
    }

    #[test]
    fn endpoints_must_be_https_or_this_computer() {
        assert!(endpoint_ok("https://api.openai.com/v1/audio/speech"));
        assert!(endpoint_ok("http://localhost:8000/v1/audio/speech"));
        assert!(endpoint_ok("http://127.0.0.1/v1/audio/speech"));
        assert!(!endpoint_ok("http://localhost.evil.com/x"));
        assert!(!endpoint_ok("https://"));
        assert!(!endpoint_ok("https://a b"));
        assert!(!endpoint_ok("ftp://x"));
    }

    #[test]
    fn a_damaged_file_keeps_the_good_sections() {
        let s = SpeechSettings::load(&json!({"engine": "local", "chunk": {"x": 1}, "voices": "junk", "text": {"skipEmoji": true}}));
        assert_eq!(s.engine, Engine::Local);
        assert_eq!(s.chunk, Chunk::Paragraph);
        assert!(s.text.skip_emoji);
        assert_eq!(s.voices, Voices::default());
        assert_eq!(SpeechSettings::load(&json!(null)), SpeechSettings::default());
    }

    #[test]
    fn role_voice_follows_the_engine() {
        let mut s = SpeechSettings::default();
        s.apply(&json!({"voices": {"roles": [{"label": "A", "system": "Daniel", "cloud": "echo", "rate": 1.2}]}})).unwrap();
        assert_eq!(s.role_voice(1), ("Daniel", 1.2));
        s.engine = Engine::Cloud;
        assert_eq!(s.role_voice(1).0, "echo");
        assert_eq!(s.role_voice(0).0, "");
        assert_eq!(s.role_voice(99), ("", 1.0));
    }
}
