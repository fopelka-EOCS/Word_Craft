//! Interface languages (#8). Strings in the code stay English and are the lookup keys; a catalog
//! per language (`*.tsv`, format in `zh-hans.tsv`) maps them to display text when the UI is drawn.
//! Command ids, document text, file names, the control channel, the CLI and MCP never see
//! translated text, so scripts and agents are unaffected. A string without a translation shows in
//! English, so coverage can grow one entry at a time.
//!
//! The `language` setting is `auto` by default: the UI follows the operating system's language
//! (Windows display language, macOS preferred languages, `LANG`/`LC_*` elsewhere) when there is a
//! catalog for it, and English otherwise. File ▸ Options ▸ Interface language picks one by hand.
//!
//! This is the system PdfCraft and PhotoCraft use (`pdfcraft/crates/ui-egui/src/i18n`), so the
//! catalog format and habits carry over between the Crafting Apps.
//!
//! # Adding a language
//! 1. Add `xx.tsv` next to `zh-hans.tsv` (copy its header; translate from the *meaning* of the
//!    English text, clean-room: never from Microsoft Word's or another product's localisation).
//! 2. Add one row to [`LANGUAGES`]. Set its `editing_lang` only when `wordcraft-proof` can proof
//!    the language (Polish: `pl-PL`): new documents are then written in it. Hebrew (`he`) is the
//!    mirrored interface: [`Lang::is_rtl`] is true and [`Lang::catalog_tsv`] is `he.tsv`.
//!
//! The Options dropdown, the system-language match and the catalog tests pick it up from there.
//!
//! # Looking strings up
//! - [`tl!`](crate::tl) / [`t`]: a string in the current language. [`tr`]: in a given one.
//! - [`tc`] / [`trc`]: worded for a context (`preview`), for text that must fit a tight spot.
//! - [`location`]: a ribbon location such as `Home › Font`, segment by segment.
//! - [`fmt`]: fill `{name}` placeholders after a lookup; translators may reorder them.

mod catalog;

use std::borrow::Cow;
use std::cell::Cell;
use std::sync::OnceLock;

use catalog::Catalog;

/// The `language` setting that follows the system.
pub const AUTO: &str = "auto";

/// One supported interface language.
pub struct LangInfo {
    /// BCP 47 code, lowercase (`ja`, `zh-hans`). Also the `language` setting's value.
    pub code: &'static str,
    /// The language's name in itself, shown in the Options dropdown.
    pub name: &'static str,
    /// Catalog file contents (empty for the built-in English).
    pub source: &'static str,
    /// Which CJK interface face comes first (Chinese text wants the Chinese face).
    pub prefer_hans: bool,
    /// The language (BCP 47) new blank documents are written in with this interface, for
    /// languages WordCraft can proof; `None` keeps WordCraft's default, US English.
    pub editing_lang: Option<&'static str>,
    catalog: OnceLock<Catalog>,
}

/// The registry. English first: it is the fallback and the source language.
pub static LANGUAGES: [LangInfo; 28] = [
    LangInfo { code: "en", name: "English", source: "", prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Simplified Chinese; `zh`, `zh-CN`, `zh-SG` and `zh-Hans-*` resolve here (see `candidates`).
    LangInfo {
        code: "zh-hans",
        name: "简体中文",
        source: include_str!("zh-hans.tsv"),
        prefer_hans: true,
        editing_lang: None,
        catalog: OnceLock::new(),
    },
    // Traditional Chinese (Taiwan vocabulary); `zh-TW`, `zh-HK`, `zh-MO` and `zh-Hant-*` resolve here.
    LangInfo {
        code: "zh-hant",
        name: "繁體中文",
        source: include_str!("zh-hant.tsv"),
        prefer_hans: true,
        editing_lang: None,
        catalog: OnceLock::new(),
    },
    LangInfo { code: "ja", name: "日本語", source: include_str!("ja.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    LangInfo {
        code: "ko",
        name: "한국어",
        source: include_str!("ko.tsv"),
        prefer_hans: false, // 일본어처럼 CJK지만 한자 우선순위는 필요 없으므로 false
        editing_lang: None, // 현재 한국어 맞춤법(proofing) 지원이 없으므로 None
        catalog: OnceLock::new(),
    },
    LangInfo {
        code: "uk", name: "Українська", source: include_str!("uk.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new()
    },
    // Spanish, neutral across Spain and Latin America; `es-ES`, `es-MX`, `es-419` … resolve here.
    LangInfo { code: "es", name: "Español", source: include_str!("es.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Brazilian Portuguese; `pt-BR` and `pt-BR-*` resolve here. Plain `pt` and `pt-PT` have no
    // catalog yet (the European vocabulary differs), so they stay in English.
    LangInfo {
        code: "pt-br",
        name: "Português (Brasil)",
        source: include_str!("pt-br.tsv"),
        prefer_hans: false,
        editing_lang: Some("pt-BR"),
        catalog: OnceLock::new(),
    },
    // Serbian, Cyrillic script (the default per BCP 47 when no script is given); `sr`, `sr-RS`,
    // `sr-Cyrl-*` resolve here.
    LangInfo { code: "sr", name: "Српски", source: include_str!("sr.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Serbian, Latin script; `sr-Latn-*` resolves here (see `candidates`'s generic prefix
    // matching — no special-casing needed, unlike Chinese's script-by-region fallback).
    LangInfo {
        code: "sr-latn",
        name: "Srpski (latinica)",
        source: include_str!("sr-latn.tsv"),
        prefer_hans: false,
        editing_lang: None,
        catalog: OnceLock::new(),
    },
    // Polish; `pl`, `pl-PL` and `pl_PL.UTF-8` resolve here. WordCraft proofs Polish (spelling,
    // grammar, hyphenation), so new documents are written in Polish with this interface.
    LangInfo {
        code: "pl",
        name: "Polski",
        source: include_str!("pl.tsv"),
        prefer_hans: false,
        editing_lang: Some("pl-PL"),
        catalog: OnceLock::new(),
    },
    LangInfo {
        code: "cs",
        name: "Čeština",
        source: include_str!("cs.tsv"),
        prefer_hans: false,
        editing_lang: Some("cs-CZ"),
        catalog: OnceLock::new(),
    },
    LangInfo { code: "et", name: "Eesti", source: include_str!("et.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // German, neutral across Germany, Austria and Switzerland; `de`, `de-DE`, `de-AT`, `de-CH` …
    // resolve here.
    LangInfo { code: "de", name: "Deutsch", source: include_str!("de.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Russian; `ru`, `ru-RU`, `ru_RU.UTF-8` and other regions resolve here.
    LangInfo { code: "ru", name: "Русский", source: include_str!("ru.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    LangInfo {
        code: "tr",
        name: "Türkçe",
        source: include_str!("tr.tsv"),
        prefer_hans: false,
        editing_lang: Some("tr-TR"),
        catalog: OnceLock::new(),
    },
    // Italian; `it-IT`, `it-CH` and `it-*` resolve here. New documents are written in
    // Italian: `wordcraft-proof` checks it (`ProofLang::It`).
    LangInfo {
        code: "it",
        name: "Italiano",
        source: include_str!("it.tsv"),
        prefer_hans: false,
        editing_lang: Some("it-IT"),
        catalog: OnceLock::new(),
    },
    // Hungarian; `hu`, `hu-HU` and `hu_HU.UTF-8` resolve here.
    LangInfo { code: "hu", name: "Magyar", source: include_str!("hu.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // French; `fr`, `fr-FR`, `fr_CA` and other regions resolve here.
    LangInfo { code: "fr", name: "Français", source: include_str!("fr.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Bulgarian; `bg`, `bg-BG` and `bg_BG.UTF-8` resolve here.
    LangInfo {
        code: "bg", name: "Български", source: include_str!("bg.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new()
    },
    // Vietnamese; `vi`, `vi-VN` and `vi_VN.UTF-8` resolve here.
    LangInfo { code: "vi", name: "Tiếng Việt", source: include_str!("vi.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Swedish; sv, sv-SE, sv-FI and sv_SE.UTF-8 resolve here.
    LangInfo { code: "sv", name: "Svenska", source: include_str!("sv.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Croatian; `hr`, `hr-HR` and `hr_HR.UTF-8` resolve here.
    LangInfo { code: "hr", name: "Hrvatski", source: include_str!("hr.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Dutch; `nl`, `nl-NL`, `nl-BE` and `nl_NL.UTF-8` resolve here.
    LangInfo { code: "nl", name: "Nederlands", source: include_str!("nl.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Danish; `da`, `da-DK` and `da_DK.UTF-8` resolve here.
    LangInfo { code: "da", name: "Dansk", source: include_str!("da.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Romanian; `ro`, `ro-RO`, `ro_RO.UTF-8` and `ro-MD` resolve here.
    LangInfo { code: "ro", name: "Română", source: include_str!("ro.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Slovenian; `sl`, `sl-SI` and `sl_SI.UTF-8` resolve here.
    LangInfo { code: "sl", name: "Slovenščina", source: include_str!("sl.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new() },
    // Hebrew; `he`, `he-IL` and `he_IL.UTF-8` resolve here. The interface is mirrored.
    LangInfo {
        code: "he", name: "עברית (Hebrew)", source: include_str!("he.tsv"), prefer_hans: false, editing_lang: None, catalog: OnceLock::new()
    },
];

impl LangInfo {
    fn catalog(&self) -> &Catalog {
        self.catalog.get_or_init(|| {
            let (catalog, errors) = Catalog::parse(self.source);
            for error in errors {
                log::warn!("{} interface catalog: {error}; that entry shows in English", self.code);
            }
            catalog
        })
    }
}

/// A language the UI can be shown in (a handle into [`LANGUAGES`]).
#[derive(Clone, Copy)]
pub struct Lang(&'static LangInfo);

impl std::fmt::Debug for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lang({})", self.0.code)
    }
}

impl PartialEq for Lang {
    fn eq(&self, other: &Self) -> bool {
        self.0.code == other.0.code
    }
}

impl Eq for Lang {}

impl Lang {
    pub const EN: Lang = Lang(&LANGUAGES[0]);

    pub fn code(self) -> &'static str {
        self.0.code
    }

    pub fn name(self) -> &'static str {
        self.0.name
    }

    pub fn prefers_hans(self) -> bool {
        self.0.prefer_hans
    }

    pub fn prefers_korean(self) -> bool {
        self.0.code == "ko"
    }

    /// The language new blank documents are written in with this interface ([`LangInfo::editing_lang`]).
    pub fn editing_lang(self) -> Option<&'static str> {
        self.0.editing_lang
    }

    /// A mirrored interface: Hebrew. Rows pack from the right, and Hebrew text is reordered for
    /// egui, which places glyphs left to right.
    pub fn is_rtl(self) -> bool {
        self.0.code == "he"
    }

    /// The catalog file for this language (empty for English).
    pub fn catalog_tsv(self) -> &'static str {
        self.0.source
    }

    /// Is the interface written in CJK script (its own name is), so it needs a CJK face?
    pub fn uses_cjk(self) -> bool {
        self.0.name.chars().any(|c| ('\u{2E80}'..='\u{9FFF}').contains(&c) || ('\u{AC00}'..='\u{D7AF}').contains(&c))
    }

    /// The CJK interface text the fonts must cover with the Chinese (`hans`) or other CJK fallback
    /// order: the catalogs of the CJK languages using that order, and every language's own name (the
    /// Options list). The interface fonts check the embedded faces against it (#486).
    pub fn cjk_font_text(hans: bool) -> (Vec<&'static str>, String) {
        let catalogs = Lang::all().filter(|l| l.uses_cjk() && l.prefers_hans() == hans).map(|l| l.0.source).collect();
        (catalogs, Lang::all().map(Lang::name).collect::<Vec<_>>().join(" "))
    }

    /// The CJK interface catalogs using the Korean or Chinese writing-system preference.
    pub fn cjk_font_text_for(hans: bool, korean: bool) -> (Vec<&'static str>, String) {
        let catalogs = Lang::all().filter(|l| l.uses_cjk() && l.prefers_hans() == hans && l.prefers_korean() == korean).map(|l| l.0.source).collect();
        (catalogs, Lang::all().map(Lang::name).collect::<Vec<_>>().join(" "))
    }

    /// A language by its exact code (any case).
    pub fn from_code(code: &str) -> Option<Lang> {
        LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(code)).map(Lang)
    }

    /// Resolve the `language` setting: a language code, or [`AUTO`] (and anything unknown, e.g. a
    /// code from a newer version) to follow the system.
    pub fn from_pref(pref: &str) -> Lang {
        Lang::from_code(pref).unwrap_or_else(system_lang)
    }

    /// Every registered language.
    pub fn all() -> impl Iterator<Item = Lang> {
        LANGUAGES.iter().map(Lang)
    }

    /// Every registered language in the order a picker lists them: English first (the source
    /// language and the fallback), then the rest by their own name with accents folded, so
    /// Čeština sits under C rather than after Z. Names in other scripts follow the Latin ones in
    /// code point order: Greek, Cyrillic, Arabic, then the CJK languages.
    pub fn for_picker() -> Vec<Lang> {
        let mut rest: Vec<Lang> = Lang::all().filter(|l| l.code() != Lang::EN.code()).collect();
        rest.sort_by_cached_key(|l| name_sort_key(l.name()));
        std::iter::once(Lang::EN).chain(rest).collect()
    }

    fn catalog(self) -> &'static Catalog {
        self.0.catalog()
    }
}

/// The canonical `language` setting for user input: [`AUTO`] or a registered code (any case).
/// `None` for anything else, so callers keep the current setting.
pub fn normalize_pref(pref: &str) -> Option<&'static str> {
    if pref.eq_ignore_ascii_case(AUTO) {
        return Some(AUTO);
    }
    Lang::from_code(pref).map(Lang::code)
}

/// Candidate codes for a locale tag, most specific first: `zh_TW.UTF-8` → `zh-tw`, `zh-hant`, `zh`.
fn candidates(tag: &str) -> Vec<String> {
    let base = tag.split(['.', '@']).next().unwrap_or("").replace('_', "-").to_ascii_lowercase();
    let parts: Vec<&str> = base.split('-').filter(|p| !p.is_empty()).collect();
    let Some(&primary) = parts.first() else { return Vec::new() };
    let mut out = Vec::new();
    for n in (1..=parts.len()).rev() {
        out.push(parts.get(..n).unwrap_or_default().join("-"));
    }
    if primary == "zh" && !parts.iter().any(|p| matches!(*p, "hans" | "hant")) {
        // Chinese by region when no script is given.
        let script = if parts.iter().any(|p| matches!(*p, "tw" | "hk" | "mo")) { "zh-hant" } else { "zh-hans" };
        out.insert(out.len().saturating_sub(1), script.to_string());
    }
    out
}

/// The registered language for a locale tag such as `ja_JP.UTF-8` or `zh-Hant-TW`; `None` when
/// there is no catalog for it. `C`/`POSIX` mean English.
pub fn lang_from_tag(tag: &str) -> Option<Lang> {
    let cands = candidates(tag);
    if matches!(cands.first().map(String::as_str), Some("c" | "posix")) {
        return Some(Lang::EN);
    }
    cands.iter().find_map(|c| Lang::from_code(c))
}

/// The first of the system's preferred languages WordCraft has, in the user's order: someone
/// who prefers French, then Japanese, gets Japanese rather than English.
pub fn first_supported<'a>(tags: impl IntoIterator<Item = &'a str>) -> Option<Lang> {
    tags.into_iter().find_map(lang_from_tag)
}

/// The system language (looked up once). English when it can't be determined or isn't supported.
pub fn system_lang() -> Lang {
    // Tests drive the UI by its English labels whatever the machine's language is.
    if cfg!(test) {
        return Lang::EN;
    }
    static SYSTEM: OnceLock<Lang> = OnceLock::new();
    *SYSTEM.get_or_init(detect_system_lang)
}

fn detect_system_lang() -> Lang {
    // An explicit override, then the platform's preferred UI languages (Windows display language,
    // macOS preferred languages, `LC_ALL`/`LC_MESSAGES`/`LANG` on Unix, the browser on the web).
    if let Some(l) = std::env::var("WORDCRAFT_LANGUAGE").ok().and_then(|v| lang_from_tag(&v)) {
        return l;
    }
    let tags: Vec<String> = sys_locale::get_locales().take(16).collect();
    first_supported(tags.iter().map(String::as_str)).unwrap_or(Lang::EN)
}

/// The system region (#838): it picks the paper File › New starts on, the default measurement
/// unit and the decimal separator, independently of the interface language (an English
/// interface in Germany still gets A4 and centimetres). `en-US` in tests.
pub fn system_region() -> wordcraft_engine::Region {
    if cfg!(test) {
        return wordcraft_engine::Region::default();
    }
    // The environment's locale variables (Unix), else the platform's first preferred locale (on
    // macOS it carries the region, as in `en-PL`; on the web it is the browser's language).
    wordcraft_engine::Region::from_locale_vars(|name| std::env::var(name).ok(), sys_locale::get_locale)
}

thread_local! {
    // Per thread: parallel test harnesses must not change each other's language.
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::EN) };
}

/// Set the language the UI is drawn in (the app does this every frame from its setting).
pub fn set_current(lang: Lang) {
    CURRENT.set(lang);
}

/// The language the UI is drawn in.
pub fn current() -> Lang {
    CURRENT.get()
}

/// Does `lang` have a catalog entry for this string? (English never does: it is the source.)
pub fn has(lang: Lang, s: &str) -> bool {
    lang.catalog().plain(s).is_some()
}

/// `s` in the current language ([`tr`] with [`current`]).
pub fn t(s: &str) -> &str {
    tr(current(), s)
}

/// `s` in the current language as worded for `context` (one of `catalog::CONTEXTS`), falling
/// back to the plain translation and then to English.
pub fn tc<'a>(context: &str, s: &'a str) -> &'a str {
    trc(current(), context, s)
}

/// [`tc`] in a given language.
pub fn trc<'a>(lang: Lang, context: &str, s: &'a str) -> &'a str {
    #[cfg(test)]
    if pseudo::on() {
        return pseudo::mark(s);
    }
    lang.catalog().in_context(context, s).unwrap_or(s)
}

/// Lowercase translated interface labels and UI search queries in the current language.
/// Turkish has two distinct I pairs: `İ`/`i` and `I`/`ı`. Unicode's default casing turns `İ`
/// into `i` plus a combining dot, which would prevent `italik` from finding `İtalik`.
/// Command IDs and document text keep their own existing casing semantics.
pub fn lowercase(s: &str) -> String {
    if current().code() != "tr" {
        return s.to_lowercase();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'İ' => out.push('i'),
            'I' => out.push('ı'),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// `s` in `lang`; strings without a translation come back unchanged.
pub fn tr(lang: Lang, s: &str) -> &str {
    #[cfg(test)]
    if pseudo::on() {
        return pseudo::mark(s);
    }
    lang.catalog().plain(s).unwrap_or(s)
}

/// Does `s` contain a strong right-to-left letter?
fn contains_rtl(s: &str) -> bool {
    s.chars().any(|c| matches!(unicode_bidi::bidi_class(c), unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL))
}

/// Bracket `c` as drawn in a right-to-left run. egui paints the character it is given.
fn mirror_bracket(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        // `{` `}` stay: catalog placeholders are `{name}`, and reversing a brace would hide it from `fmt`.
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        _ => c,
    }
}

/// Keep `{name}` together as left-to-right text. A brace is direction-neutral, so an right-to-left
/// paragraph would otherwise peel it off the name and `fmt` could no longer see `{page}`.
fn protect_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after_open)) = rest.split_once('{') {
        out.push_str(before);
        if let Some((name, after)) = after_open.split_once('}')
            && !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            out.push('\u{202A}');
            out.push('{');
            out.push_str(name);
            out.push('}');
            out.push('\u{202C}');
            rest = after;
            continue;
        }
        out.push('{');
        rest = after_open;
    }
    out.push_str(rest);
    out
}

/// `text` in the order egui should paint when the interface is right to left. egui lays glyphs
/// out left to right and does not run the bidi algorithm, so each right-to-left run is reversed
/// (and its brackets mirrored) while left-to-right runs, including `{placeholders}`, stay put.
pub fn visual_for_ui(text: &str) -> String {
    let protected = protect_placeholders(text);
    let mut out = String::with_capacity(protected.len());
    for para in protected.split_inclusive('\n') {
        let (body, nl) = match para.strip_suffix('\n') {
            Some(body) => (body, true),
            None => (para, false),
        };
        if !body.is_empty() {
            let info = unicode_bidi::BidiInfo::new(body, Some(unicode_bidi::Level::rtl()));
            if let Some(p) = info.paragraphs.first() {
                let (levels, runs) = info.visual_runs(p, p.range.clone());
                for run in runs {
                    let rtl = levels.get(run.start).is_some_and(|level| level.is_rtl());
                    let Some(piece) = body.get(run) else { continue };
                    if rtl {
                        out.extend(piece.chars().rev().map(mirror_bracket));
                    } else {
                        out.push_str(piece);
                    }
                }
            } else {
                out.push_str(body);
            }
        }
        if nl {
            out.push('\n');
        }
    }
    out.retain(|c| c != '\u{202A}' && c != '\u{202C}');
    out
}

/// `s` as egui should paint it. Left-to-right interfaces borrow `s`. A right-to-left interface
/// returns an owned reordering; the caller drops it, and the lookup cache does too when it is full.
pub fn present(s: &str) -> Cow<'_, str> {
    #[cfg(test)]
    if pseudo::on() {
        return Cow::Borrowed(s);
    }
    if !current().is_rtl() || !contains_rtl(s) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(cached_visual(s))
}

/// Reordered strings for labels drawn every frame. Entries are owned and dropped when the cache
/// is full, so a document with many distinct right-to-left names cannot grow this without bound.
const VISUAL_CACHE_MAX_ENTRIES: usize = 8192;
const VISUAL_CACHE_MAX_BYTES: usize = 512 * 1024;

struct VisualCache {
    map: std::collections::HashMap<String, String>,
    bytes: usize,
}

static VISUAL_CACHE: std::sync::LazyLock<std::sync::Mutex<VisualCache>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(VisualCache { map: std::collections::HashMap::new(), bytes: 0 }));

fn cached_visual(logical: &str) -> String {
    let guard = VISUAL_CACHE.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(found) = guard.map.get(logical) {
        return found.clone();
    }
    drop(guard);
    let visual = visual_for_ui(logical);
    let cost = logical.len().saturating_add(visual.len());
    // One enormous label is painted from the caller's copy and is not kept.
    if cost > VISUAL_CACHE_MAX_BYTES {
        return visual;
    }
    let mut guard = VISUAL_CACHE.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(found) = guard.map.get(logical) {
        return found.clone();
    }
    if guard.map.len() >= VISUAL_CACHE_MAX_ENTRIES || guard.bytes.saturating_add(cost) > VISUAL_CACHE_MAX_BYTES {
        guard.map.clear();
        guard.bytes = 0;
    }
    guard.bytes = guard.bytes.saturating_add(cost);
    guard.map.insert(logical.to_string(), visual.clone());
    visual
}

/// [`present`] as an owned string, for text built from more than one lookup.
pub fn present_owned(s: &str) -> String {
    present(s).into_owned()
}

#[cfg(test)]
pub(crate) fn visual_cache_entries() -> usize {
    VISUAL_CACHE.lock().unwrap_or_else(|err| err.into_inner()).map.len()
}

/// Pack a horizontal row toward the trailing edge: the right in a left-to-right interface, the
/// left when the interface is mirrored. English keeps today's `right_to_left` rows.
pub fn trailing_layout(valign: egui::Align) -> egui::Layout {
    if current().is_rtl() { egui::Layout::left_to_right(valign) } else { egui::Layout::right_to_left(valign) }
}

/// Lay `body` out from the right when the interface is Hebrew. egui 0.36 panels are created
/// `top_down(Align::Min)` and [`egui::Ui`] has no way to change that layout afterwards, so Hebrew
/// runs `body` in a child with `Align::Max`. That is what makes [`egui::Ui::horizontal`] pack from
/// the right. English calls `body` on `ui` directly, so an English frame is unchanged.
pub fn mirrored<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    if current().is_rtl() { ui.with_layout(egui::Layout::top_down(egui::Align::Max), body).inner } else { body(ui) }
}

/// A style's name as the interface shows it: built-in styles (`Normal`, `Heading 2`, `TOC 3`) in
/// the current language, styles the author made exactly as named. Display only: the document,
/// commands and saved files keep the English name.
pub fn style_name(style: &wordcraft_doc::Style) -> String {
    if style.builtin { builtin_style_name(&style.name) } else { present_owned(&style.name) }
}

/// A built-in style's English name in the current language; numbered families go through one
/// template each (`Heading {n}`), and names without a translation stay English.
pub fn builtin_style_name(name: &str) -> String {
    let direct = t(name);
    if direct != name {
        return present_owned(direct);
    }
    for (prefix, template) in [("Heading ", "Heading {n}"), ("TOC ", "TOC {n}"), ("Index ", "Index {n}")] {
        if let Some(n) = name.strip_prefix(prefix)
            && !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
        {
            return present_owned(&fmt(t(template), &[("n", n)]));
        }
    }
    present_owned(name)
}

/// A ribbon location (`Home › Font`) in the current language, segment by segment.
pub fn location(loc: &str) -> String {
    present_owned(&loc.split(" › ").map(t).collect::<Vec<_>>().join(" › "))
}

/// A label with an action after a fixed English prefix, e.g. `Undo Typing`: both parts translated
/// where the catalog has them.
pub fn prefixed(prefix: &str, rest: &str) -> String {
    if rest.is_empty() {
        return present_owned(t(prefix));
    }
    present_owned(&fmt(t(&format!("{prefix} {{action}}")), &[("action", t(rest))]))
}

/// Fill `{name}` placeholders in one pass. Unknown placeholders stay as written, and inserted
/// values are never read as templates again, so a file name containing `{n}` stays intact.
pub fn fmt(template: impl AsRef<str>, args: &[(&str, &str)]) -> String {
    let template = template.as_ref();
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some((before, after_open)) = rest.split_once('{') {
        out.push_str(before);
        let Some((name, after_close)) = after_open.split_once('}') else {
            out.push('{');
            out.push_str(after_open);
            return out;
        };
        match args.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = after_close;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod audit;
#[cfg(test)]
pub(crate) mod pseudo;
#[cfg(test)]
mod tests;

/// `name` lowercased with the Latin accents that occur in language names folded away, so a
/// byte-wise sort reads alphabetically. Other scripts are left as they are.
fn name_sort_key(name: &str) -> String {
    name.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'è'..='ë' | 'ē' | 'ě' | 'ę' | 'ế' | 'ệ' => 'e',
            'ì'..='ï' | 'ī' => 'i',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò'..='ö' | 'ø' | 'ő' => 'o',
            'ś' | 'š' => 's',
            'ù'..='ü' | 'ū' | 'ů' | 'ű' => 'u',
            'ý' | 'ÿ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod picker_order_tests {
    use super::*;

    #[test]
    fn picker_lists_english_first_then_names_alphabetically() {
        let order = Lang::for_picker();
        assert_eq!(order.first().map(|l| l.code()), Some(Lang::EN.code()));
        assert_eq!(order.len(), Lang::all().count());
        let keys: Vec<String> = order[1..].iter().map(|l| name_sort_key(l.name())).collect();
        assert!(keys.windows(2).all(|w| w[0] <= w[1]), "{keys:?}");
        assert_eq!(name_sort_key("Čeština"), "cestina");
        assert_eq!(name_sort_key("Türkçe"), "turkce");
    }
}
