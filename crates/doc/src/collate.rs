//! Language-aware sorting of text: a small, pure-Rust collation.
//!
//! Every letter gets a *primary* weight (its place in the alphabet) and a *secondary* one (its
//! diacritic). Strings compare by primary weights first, then by secondary ones, so `é` sorts
//! with `e` (`resume` < `résumé` < `rex`) unless the language makes it a letter of its own:
//! Polish puts `ą` right after `a` and `ż` after `ź` after `z`, Swedish puts `å ä ö` after `z`,
//! Czech puts `ch` after `h`, Spanish `ñ` after `n`, Hungarian `cs`, `gy`, `sz`… after their
//! first letter. The per-language tables follow the published alphabets of those languages.
//! Text of plain ASCII letters sorts exactly as a case-insensitive code-point compare; letters such
//! as `ø`, `ł`, `æ` and `ß` sort with their base letter.
//!
//! How a key is built ([`sort_key`]):
//! 1. The text is lowercased (`to_lowercase`, no full case folding) and put in canonical form
//!    (`collate_ucd::canonical`: decomposed, marks in canonical order). So a Latin letter typed
//!    precomposed (`ậ`) and typed as base + marks (`a` U+0302 U+0323 or `a` U+0323 U+0302;
//!    marks of different combining classes match in either order) get one key. This covers the
//!    Latin letters U+00C0..=U+024F and U+1E00..=U+1EFF with marks U+0300..=U+036F; other
//!    scripts (Cyrillic й, Greek ά) are not unified. (Lowercasing comes first so a character whose lowercase is a precomposed letter, such
//!    as the Angstrom sign, is decomposed too; the result is the same canonical lowercase text.)
//! 2. Each cluster is a base character and the combining marks (U+0300..=U+036F) after it. The
//!    base is composed with the cluster's **first** mark when `COMPOSE` has the pair **and**
//!    the composed letter means something to the key: it is in `DECOMP`, or a tailoring of
//!    the language starts with it. Otherwise the base stands alone. The other marks follow.
//! 3. Collation elements are read left to right: the longest tailoring sequence there, else
//!    a `DECOMP` letter (its base letters), else the character itself. Each combining mark
//!    after an element **appends** the code `32 + (cp - 0x300)` to the secondary list of the
//!    element's last weight; a mark with no element before it is dropped from the weights.
//! 4. There is one secondary list per primary weight: `[DECOMP mark or tailoring sec]` when that
//!    is nonzero (on the element's first weight only), else empty; then the mark codes, in
//!    canonical order. Distinct sets of marks therefore never share a secondary key (`ấ` is
//!    `[3, 33]`, `ä` is `[4]`), and an accented letter sorts by its base letter (`Ánh` next to
//!    `Anh`, not after `z`).
//! 5. A key holds at most 16 384 primary weights; the rest of a longer text only takes
//!    part through the `text` tie-break, which always holds the whole canonical lowercase text.
//!
//! Vietnamese (`vi`, `vi-VN`…; rule V, the CLDR `vi` standard order) replaces rules 2–4:
//! - V1. In each cluster a quality mark merges with the base into a letter of its own (`ă â ê ô
//!   ơ ư`; `đ` has no mark) wherever it stands in the cluster, unless it is blocked: an earlier
//!   mark that did not merge has a combining class >= its own. Canonical order puts the dot below
//!   before the circumflex (`ậ` is `a` U+0323 U+0302), so composing the first mark (rule 2) would
//!   miss these letters.
//! - V2. The letters sort `a ă â b c d đ e ê … o ô ơ … u ư …` (primary weights after their base
//!   letter, `VI`).
//! - V3. Each element's secondary list starts with its tone: 0 none, 1 huyền, 2 hỏi, 3 ngã,
//!   4 sắc, 5 nặng (the first tone mark of the cluster); then the `DECOMP` mark of a letter
//!   such as `ø` when nonzero; then the codes of the other marks (rule 3), leaving out every
//!   repeat of the tone mark. So tones are a secondary difference (`ma < mà < mả < mã < má < mạ`)
//!   and `Hoà` (tone on `a`) sorts before `Hòa` (tone on `o`).
//!
//! The key ignores case. [`sort_order`] adds the case level ([`case_key`]: lowercase first) to
//! break ties when asked to.

use std::borrow::Cow;
use std::cmp::Ordering;

use crate::collate_ucd;

/// Most primary weights a key holds (rule 5): a megabyte paste does not make a megabyte key.
const MAX_PRIMARY: usize = 16_384;

/// The diacritic of a precomposed lowercase Latin letter: `(base letters, mark)`. Marks: 1 acute,
/// 2 grave, 3 circumflex, 4 diaeresis, 5 tilde, 6 ring, 7 caron, 8 cedilla, 9 ogonek, 10 dot,
/// 11 stroke, 12 breve, 13 macron, 14 double acute, 15 comma below, 16 ligature, 17 dotless,
/// 18 eth.
const DECOMP: &[(char, &str, u16)] = &[
    ('à', "a", 2),
    ('á', "a", 1),
    ('â', "a", 3),
    ('ã', "a", 5),
    ('ä', "a", 4),
    ('å', "a", 6),
    ('ā', "a", 13),
    ('ă', "a", 12),
    ('ą', "a", 9),
    ('æ', "ae", 16),
    ('ç', "c", 8),
    ('ć', "c", 1),
    ('ĉ', "c", 3),
    ('ċ', "c", 10),
    ('č', "c", 7),
    ('ď', "d", 7),
    ('đ', "d", 11),
    ('ð', "d", 18),
    ('è', "e", 2),
    ('é', "e", 1),
    ('ê', "e", 3),
    ('ë', "e", 4),
    ('ē', "e", 13),
    ('ĕ', "e", 12),
    ('ė', "e", 10),
    ('ę', "e", 9),
    ('ě', "e", 7),
    ('ĝ', "g", 3),
    ('ğ', "g", 12),
    ('ġ', "g", 10),
    ('ģ', "g", 8),
    ('ĥ', "h", 3),
    ('ħ', "h", 11),
    ('ì', "i", 2),
    ('í', "i", 1),
    ('î', "i", 3),
    ('ï', "i", 4),
    ('ĩ', "i", 5),
    ('ī', "i", 13),
    ('ĭ', "i", 12),
    ('į', "i", 9),
    ('ı', "i", 17),
    ('ĳ', "ij", 16),
    ('ĵ', "j", 3),
    ('ķ', "k", 8),
    ('ĺ', "l", 1),
    ('ļ', "l", 8),
    ('ľ', "l", 7),
    ('ŀ', "l", 10),
    ('ł', "l", 11),
    ('ñ', "n", 5),
    ('ń', "n", 1),
    ('ņ', "n", 8),
    ('ň', "n", 7),
    ('ò', "o", 2),
    ('ó', "o", 1),
    ('ô', "o", 3),
    ('õ', "o", 5),
    ('ö', "o", 4),
    ('ø', "o", 11),
    ('ō', "o", 13),
    ('ŏ', "o", 12),
    ('ő', "o", 14),
    ('œ', "oe", 16),
    ('ŕ', "r", 1),
    ('ŗ', "r", 8),
    ('ř', "r", 7),
    ('ś', "s", 1),
    ('ŝ', "s", 3),
    ('ş', "s", 8),
    ('š', "s", 7),
    ('ș', "s", 15),
    ('ß', "ss", 16),
    ('ţ', "t", 8),
    ('ť', "t", 7),
    ('ŧ', "t", 11),
    ('ț', "t", 15),
    ('ù', "u", 2),
    ('ú', "u", 1),
    ('û', "u", 3),
    ('ü', "u", 4),
    ('ũ', "u", 5),
    ('ū', "u", 13),
    ('ŭ', "u", 12),
    ('ů', "u", 6),
    ('ű', "u", 14),
    ('ų', "u", 9),
    ('ŵ', "w", 3),
    ('ý', "y", 1),
    ('ÿ', "y", 4),
    ('ŷ', "y", 3),
    ('ź', "z", 1),
    ('ż', "z", 10),
    ('ž', "z", 7),
];

/// A letter (or letter group) a language sorts as a letter of its own: right after `after`
/// (`rank` places after it), with secondary weight `sec` among letters of the same place.
struct Tailor {
    seq: &'static str,
    after: char,
    rank: u32,
    sec: u16,
}

const fn t(seq: &'static str, after: char, rank: u32) -> Tailor {
    Tailor { seq, after, rank, sec: 0 }
}

const fn ts(seq: &'static str, after: char, rank: u32, sec: u16) -> Tailor {
    Tailor { seq, after, rank, sec }
}

const PL: &[Tailor] =
    &[t("ą", 'a', 1), t("ć", 'c', 1), t("ę", 'e', 1), t("ł", 'l', 1), t("ń", 'n', 1), t("ó", 'o', 1), t("ś", 's', 1), t("ź", 'z', 1), t("ż", 'z', 2)];
const CS: &[Tailor] = &[t("č", 'c', 1), t("ch", 'h', 1), t("ř", 'r', 1), t("š", 's', 1), t("ž", 'z', 1)];
const SK: &[Tailor] = &[t("ä", 'a', 1), t("č", 'c', 1), t("ch", 'h', 1), t("ô", 'o', 1), t("š", 's', 1), t("ž", 'z', 1)];
const HU: &[Tailor] = &[
    t("cs", 'c', 1),
    t("dz", 'd', 1),
    t("dzs", 'd', 2),
    t("gy", 'g', 1),
    t("ly", 'l', 1),
    t("ny", 'n', 1),
    t("ö", 'o', 1),
    ts("ő", 'o', 1, 14),
    t("sz", 's', 1),
    t("ty", 't', 1),
    t("ü", 'u', 1),
    ts("ű", 'u', 1, 14),
    t("zs", 'z', 1),
];
const SV: &[Tailor] = &[t("å", 'z', 1), t("ä", 'z', 2), ts("æ", 'z', 2, 16), t("ö", 'z', 3), ts("ø", 'z', 3, 11)];
const DA: &[Tailor] = &[t("æ", 'z', 1), ts("ä", 'z', 1, 4), t("ø", 'z', 2), ts("ö", 'z', 2, 4), t("å", 'z', 3)];
const ES: &[Tailor] = &[t("ñ", 'n', 1)];
// Russian ё shares е's primary weight; the diaeresis breaks ties (CLDR Russian standard).
const RU: &[Tailor] = &[ts("ё", 'е', 0, 4)];
const TR: &[Tailor] = &[t("ç", 'c', 1), t("ğ", 'g', 1), t("ı", 'h', 1), t("ö", 'o', 1), t("ş", 's', 1), t("ü", 'u', 1)];
const HR: &[Tailor] =
    &[t("č", 'c', 1), t("ć", 'c', 2), t("dž", 'd', 1), t("đ", 'd', 2), t("lj", 'l', 1), t("nj", 'n', 1), t("š", 's', 1), t("ž", 'z', 1)];
const RO: &[Tailor] = &[t("ă", 'a', 1), t("â", 'a', 2), t("î", 'i', 1), t("ș", 's', 1), ts("ş", 's', 1, 8), t("ț", 't', 1), ts("ţ", 't', 1, 8)];
const ET: &[Tailor] = &[t("š", 's', 1), t("z", 's', 2), t("ž", 's', 3), t("õ", 'w', 1), t("ä", 'w', 2), t("ö", 'w', 3), t("ü", 'w', 4)];
const LT: &[Tailor] = &[t("č", 'c', 1), t("š", 's', 1), t("ž", 'z', 1)];
const LV: &[Tailor] = &[t("č", 'c', 1), t("ģ", 'g', 1), t("ķ", 'k', 1), t("ļ", 'l', 1), t("ņ", 'n', 1), t("š", 's', 1), t("ž", 'z', 1)];
const IS: &[Tailor] = &[
    t("á", 'a', 1),
    t("ð", 'd', 1),
    t("é", 'e', 1),
    t("í", 'i', 1),
    t("ó", 'o', 1),
    t("ú", 'u', 1),
    t("ý", 'y', 1),
    t("þ", 'z', 1),
    t("æ", 'z', 2),
    t("ö", 'z', 3),
];

/// Vietnamese (CLDR `vi` standard): the letters with a "quality" mark are letters of their own,
/// `a ă â b c d đ e ê … o ô ơ … u ư …`. Tones are not: they are a secondary difference
/// ([`QUALITY`], [`TONES`], rule V).
const VI: &[Tailor] = &[t("ă", 'a', 1), t("â", 'a', 2), t("đ", 'd', 1), t("ê", 'e', 1), t("ô", 'o', 1), t("ơ", 'o', 2), t("ư", 'u', 1)];

/// A base letter and the mark that make a Vietnamese letter of their own (rule V1).
const QUALITY: &[(char, char, char)] =
    &[('a', '\u{306}', 'ă'), ('a', '\u{302}', 'â'), ('e', '\u{302}', 'ê'), ('o', '\u{302}', 'ô'), ('o', '\u{31b}', 'ơ'), ('u', '\u{31b}', 'ư')];

/// The Vietnamese tone marks and their secondary weights, in `vi` order: (no tone 0), huyền,
/// hỏi, ngã, sắc, nặng.
const TONES: &[(char, u16)] = &[('\u{300}', 1), ('\u{309}', 2), ('\u{303}', 3), ('\u{301}', 4), ('\u{323}', 5)];

/// The primary language subtag of a BCP 47 tag (`pl-PL` → `pl`), lowercased.
pub fn language(lang: Option<&str>) -> String {
    lang.and_then(|l| l.split(['-', '_']).next()).unwrap_or("").trim().to_ascii_lowercase()
}

/// Whether `lang` selects the Vietnamese order (`vi`, `vi-VN`, `VI_vn`…).
fn is_vi(lang: Option<&str>) -> bool {
    language(lang) == "vi"
}

fn tailoring(lang: Option<&str>) -> &'static [Tailor] {
    match language(lang).as_str() {
        "pl" => PL,
        "cs" => CS,
        "sk" => SK,
        "hu" => HU,
        "sv" | "fi" => SV,
        "da" | "nb" | "nn" | "no" => DA,
        "es" | "gl" => ES,
        "ru" => RU,
        "tr" | "az" => TR,
        "hr" | "bs" | "sr" => HR,
        "ro" => RO,
        "et" => ET,
        "lt" => LT,
        "lv" => LV,
        "is" => IS,
        "sl" => LT,
        _ => &[],
    }
}

/// A lowercase Latin or Russian letter followed by a combining mark, and the single letter Unicode
/// composes them to (NFC; generated from the Unicode Character Database). Text pasted from
/// decomposed sources (macOS file names: `z` + U+0307) collates like the composed letter.
const COMPOSE: &[(char, char, char)] = &[
    ('a', '\u{300}', 'à'),
    ('a', '\u{301}', 'á'),
    ('a', '\u{302}', 'â'),
    ('a', '\u{303}', 'ã'),
    ('a', '\u{304}', 'ā'),
    ('a', '\u{306}', 'ă'),
    ('a', '\u{307}', 'ȧ'),
    ('a', '\u{308}', 'ä'),
    ('a', '\u{30a}', 'å'),
    ('a', '\u{30c}', 'ǎ'),
    ('a', '\u{328}', 'ą'),
    ('b', '\u{307}', 'ḃ'),
    ('c', '\u{301}', 'ć'),
    ('c', '\u{302}', 'ĉ'),
    ('c', '\u{307}', 'ċ'),
    ('c', '\u{30c}', 'č'),
    ('c', '\u{327}', 'ç'),
    ('d', '\u{307}', 'ḋ'),
    ('d', '\u{30c}', 'ď'),
    ('d', '\u{327}', 'ḑ'),
    ('e', '\u{300}', 'è'),
    ('e', '\u{301}', 'é'),
    ('e', '\u{302}', 'ê'),
    ('e', '\u{303}', 'ẽ'),
    ('e', '\u{304}', 'ē'),
    ('e', '\u{306}', 'ĕ'),
    ('e', '\u{307}', 'ė'),
    ('e', '\u{308}', 'ë'),
    ('e', '\u{30c}', 'ě'),
    ('e', '\u{327}', 'ȩ'),
    ('e', '\u{328}', 'ę'),
    ('f', '\u{307}', 'ḟ'),
    ('g', '\u{301}', 'ǵ'),
    ('g', '\u{302}', 'ĝ'),
    ('g', '\u{304}', 'ḡ'),
    ('g', '\u{306}', 'ğ'),
    ('g', '\u{307}', 'ġ'),
    ('g', '\u{30c}', 'ǧ'),
    ('g', '\u{327}', 'ģ'),
    ('h', '\u{302}', 'ĥ'),
    ('h', '\u{307}', 'ḣ'),
    ('h', '\u{308}', 'ḧ'),
    ('h', '\u{30c}', 'ȟ'),
    ('h', '\u{327}', 'ḩ'),
    ('i', '\u{300}', 'ì'),
    ('i', '\u{301}', 'í'),
    ('i', '\u{302}', 'î'),
    ('i', '\u{303}', 'ĩ'),
    ('i', '\u{304}', 'ī'),
    ('i', '\u{306}', 'ĭ'),
    ('i', '\u{308}', 'ï'),
    ('i', '\u{30c}', 'ǐ'),
    ('i', '\u{328}', 'į'),
    ('j', '\u{302}', 'ĵ'),
    ('j', '\u{30c}', 'ǰ'),
    ('k', '\u{301}', 'ḱ'),
    ('k', '\u{30c}', 'ǩ'),
    ('k', '\u{327}', 'ķ'),
    ('l', '\u{301}', 'ĺ'),
    ('l', '\u{30c}', 'ľ'),
    ('l', '\u{327}', 'ļ'),
    ('m', '\u{301}', 'ḿ'),
    ('m', '\u{307}', 'ṁ'),
    ('n', '\u{300}', 'ǹ'),
    ('n', '\u{301}', 'ń'),
    ('n', '\u{303}', 'ñ'),
    ('n', '\u{307}', 'ṅ'),
    ('n', '\u{30c}', 'ň'),
    ('n', '\u{327}', 'ņ'),
    ('o', '\u{300}', 'ò'),
    ('o', '\u{301}', 'ó'),
    ('o', '\u{302}', 'ô'),
    ('o', '\u{303}', 'õ'),
    ('o', '\u{304}', 'ō'),
    ('o', '\u{306}', 'ŏ'),
    ('o', '\u{307}', 'ȯ'),
    ('o', '\u{308}', 'ö'),
    ('o', '\u{30b}', 'ő'),
    ('o', '\u{30c}', 'ǒ'),
    ('o', '\u{328}', 'ǫ'),
    ('p', '\u{301}', 'ṕ'),
    ('p', '\u{307}', 'ṗ'),
    ('r', '\u{301}', 'ŕ'),
    ('r', '\u{307}', 'ṙ'),
    ('r', '\u{30c}', 'ř'),
    ('r', '\u{327}', 'ŗ'),
    ('s', '\u{301}', 'ś'),
    ('s', '\u{302}', 'ŝ'),
    ('s', '\u{307}', 'ṡ'),
    ('s', '\u{30c}', 'š'),
    ('s', '\u{326}', 'ș'),
    ('s', '\u{327}', 'ş'),
    ('t', '\u{307}', 'ṫ'),
    ('t', '\u{308}', 'ẗ'),
    ('t', '\u{30c}', 'ť'),
    ('t', '\u{326}', 'ț'),
    ('t', '\u{327}', 'ţ'),
    ('u', '\u{300}', 'ù'),
    ('u', '\u{301}', 'ú'),
    ('u', '\u{302}', 'û'),
    ('u', '\u{303}', 'ũ'),
    ('u', '\u{304}', 'ū'),
    ('u', '\u{306}', 'ŭ'),
    ('u', '\u{308}', 'ü'),
    ('u', '\u{30a}', 'ů'),
    ('u', '\u{30b}', 'ű'),
    ('u', '\u{30c}', 'ǔ'),
    ('u', '\u{328}', 'ų'),
    ('v', '\u{303}', 'ṽ'),
    ('w', '\u{300}', 'ẁ'),
    ('w', '\u{301}', 'ẃ'),
    ('w', '\u{302}', 'ŵ'),
    ('w', '\u{307}', 'ẇ'),
    ('w', '\u{308}', 'ẅ'),
    ('w', '\u{30a}', 'ẘ'),
    ('x', '\u{307}', 'ẋ'),
    ('x', '\u{308}', 'ẍ'),
    ('y', '\u{300}', 'ỳ'),
    ('y', '\u{301}', 'ý'),
    ('y', '\u{302}', 'ŷ'),
    ('y', '\u{303}', 'ỹ'),
    ('y', '\u{304}', 'ȳ'),
    ('y', '\u{307}', 'ẏ'),
    ('y', '\u{308}', 'ÿ'),
    ('y', '\u{30a}', 'ẙ'),
    ('z', '\u{301}', 'ź'),
    ('z', '\u{302}', 'ẑ'),
    ('z', '\u{307}', 'ż'),
    ('z', '\u{30c}', 'ž'),
    ('е', '\u{308}', 'ё'),
    ('и', '\u{306}', 'й'),
];

/// Whether `c` is a combining mark the key attaches to the element before it.
fn is_mark(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c)
}

/// `s` lowercased, in canonical form (rule 1): the `text` of its key. ASCII skips the tables.
fn lower_canonical(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    collate_ucd::canonical(&s.to_lowercase())
}

/// The letter `base` + `mark` compose to, when the key reads that letter as one (rule 2): it is
/// in `DECOMP` or a tailoring of the language starts with it.
fn compose(base: char, mark: char, rules: &[Tailor]) -> Option<char> {
    let (_, _, composed) = COMPOSE.iter().find(|(b, m, _)| *b == base && *m == mark)?;
    let known = DECOMP.iter().any(|(x, _, _)| x == composed) || rules.iter().any(|r| r.seq.starts_with(*composed));
    known.then_some(*composed)
}

/// `text` (lowercase, canonical) with each cluster's base composed with its first mark where
/// rule 2 allows; the string collation elements are read from.
fn compose_first_marks<'a>(text: &'a str, rules: &[Tailor]) -> Cow<'a, str> {
    if text.is_ascii() {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    // The base of the current cluster while no mark has followed it yet.
    let mut base = None;
    for c in text.chars() {
        if !is_mark(c) {
            out.push(c);
            base = Some(c);
        } else if let Some(composed) = base.take().and_then(|b| compose(b, c, rules)) {
            out.pop();
            out.push(composed);
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// The secondary code of combining mark `c` (rule 3): `32 + (cp - 0x300)`.
fn mark_code(c: char) -> u16 {
    u16::try_from((c as u32).saturating_sub(0x300).saturating_add(32)).unwrap_or(u16::MAX)
}

fn weight(c: char, rank: u32) -> u32 {
    ((c as u32) << 4).saturating_add(rank)
}

/// One collation element: primary weight, secondary weight, the text it covers.
struct Elem<'a> {
    primary: Vec<u32>,
    sec: u16,
    text: &'a str,
    /// Set when the text is a letter of its own in the language (shown as is in headings).
    own: bool,
}

/// The next collation element of `rest` (lowercase), and its length in bytes.
fn next_elem<'a>(rest: &'a str, rules: &[Tailor]) -> Option<(Elem<'a>, usize)> {
    if let Some(r) = rules.iter().filter(|r| rest.starts_with(r.seq)).max_by_key(|r| r.seq.len()) {
        let text = rest.get(..r.seq.len())?;
        return Some((Elem { primary: vec![weight(r.after, r.rank)], sec: r.sec, text, own: true }, r.seq.len()));
    }
    let c = rest.chars().next()?;
    let n = c.len_utf8();
    let text = rest.get(..n)?;
    if let Some((_, base, mark)) = DECOMP.iter().find(|(x, _, _)| *x == c) {
        return Some((Elem { primary: base.chars().map(|b| weight(b, 0)).collect(), sec: *mark, text, own: false }, n));
    }
    // A combining mark adds a secondary difference to the letter before it (rule 3).
    if is_mark(c) {
        return Some((Elem { primary: Vec::new(), sec: mark_code(c), text, own: false }, n));
    }
    Some((Elem { primary: vec![weight(c, 0)], sec: 0, text, own: false }, n))
}

/// One Vietnamese cluster (rule V1): its base with the quality marks merged in, its tone and the
/// codes of its other marks.
struct ViCluster {
    base: char,
    tone: u16,
    others: Vec<u16>,
}

/// The cluster of `base` (lowercase) and the marks after it (canonical order) in `vi` (rule V1).
fn vi_cluster(base: char, marks: &[char]) -> ViCluster {
    let mut base = base;
    let mut rest: Vec<char> = Vec::new();
    // The highest combining class among the marks kept so far: a quality mark merges only when
    // no kept mark before it has a class >= its own (it is not blocked).
    let mut top = 0u8;
    for &m in marks {
        let class = collate_ucd::ccc(m);
        match QUALITY.iter().find(|(b, q, _)| *b == base && *q == m) {
            Some((_, _, merged)) if top < class => base = *merged,
            _ => {
                rest.push(m);
                top = top.max(class);
            }
        }
    }
    let tone_of = |m: char| TONES.iter().find(|(t, _)| *t == m).map_or(0, |(_, w)| *w);
    let tone_mark = rest.iter().copied().find(|m| tone_of(*m) != 0);
    let tone = tone_mark.map_or(0, tone_of);
    let others = rest.iter().filter(|m| Some(**m) != tone_mark).map(|m| mark_code(*m)).collect();
    ViCluster { base, tone, others }
}

/// The weights of `text` (lowercase, canonical) in `vi` (rule V), appended to `key`.
fn vi_weights(text: &str, key: &mut SortKey) {
    let mut chars = text.chars().peekable();
    let mut marks = Vec::new();
    while key.primary.len() < MAX_PRIMARY {
        let Some(c) = chars.next() else { break };
        if is_mark(c) {
            continue; // a mark with no letter before it leaves the weights alone (rule 3)
        }
        marks.clear();
        while let Some(m) = chars.next_if(|m| is_mark(*m)) {
            marks.push(m);
        }
        let cluster = vi_cluster(c, &marks);
        let mut buf = [0u8; 4];
        let Some((e, _)) = next_elem(cluster.base.encode_utf8(&mut buf), VI) else { continue };
        let mut sec = Vec::with_capacity(cluster.others.len().saturating_add(2));
        sec.push(cluster.tone);
        if e.sec != 0 {
            sec.push(e.sec);
        }
        sec.extend(cluster.others);
        let mut first = Some(sec);
        for w in e.primary {
            if key.primary.len() >= MAX_PRIMARY {
                break;
            }
            key.primary.push(w);
            key.secondary.push(first.take().unwrap_or_else(|| vec![0]));
        }
    }
}

/// A sort key: compare keys to sort strings in a language's order. Equal keys mean the strings
/// differ at most in letter case and in how they are encoded (precomposed or decomposed Latin
/// letters, marks of different combining classes in either order; see the module docs for the
/// covered range).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SortKey {
    primary: Vec<u32>,
    /// One list per primary weight: the letter's diacritic code, then its combining marks.
    secondary: Vec<Vec<u16>>,
    /// The canonical lowercase text, whole even past the primary cap.
    text: String,
}

/// The sort key of `s` for language `lang` (a BCP 47 tag such as `pl-PL`; `None` sorts like
/// English).
pub fn sort_key(s: &str, lang: Option<&str>) -> SortKey {
    let text = lower_canonical(s);
    let mut key = SortKey::default();
    if is_vi(lang) {
        vi_weights(&text, &mut key);
        key.text = text;
        return key;
    }
    let rules = tailoring(lang);
    let composed = compose_first_marks(&text, rules);
    let mut i: usize = 0;
    while key.primary.len() < MAX_PRIMARY
        && let Some(rest) = composed.get(i..).filter(|r| !r.is_empty())
    {
        let Some((e, n)) = next_elem(rest, rules) else { break };
        i = i.saturating_add(n.max(1));
        if e.primary.is_empty() {
            if let Some(last) = key.secondary.last_mut() {
                last.push(e.sec);
            }
            continue;
        }
        for (k, w) in e.primary.iter().enumerate() {
            if key.primary.len() >= MAX_PRIMARY {
                break;
            }
            key.primary.push(*w);
            key.secondary.push(if k == 0 && e.sec != 0 { vec![e.sec] } else { Vec::new() });
        }
    }
    drop(composed); // it may borrow `text`
    key.text = text;
    key
}

/// `s` lowercased and in canonical form: the text a key compares last ([`SortKey`]'s `text`).
/// Search one `search_text` in another to find text whatever its case and however it is encoded
/// (`nguyễn` typed precomposed finds `NGUYỄN` typed as base letters and marks).
fn search_text(s: &str) -> String {
    lower_canonical(s)
}

/// Whether `haystack` contains `needle`: canonical-equivalent, case-insensitive substring match
/// at letter boundaries. Both sides compare as [`search_text`], and a match counts only when no
/// mark in U+0300–U+036F follows it, so `e` does not find the `e` of `ễ` and `ê` does not find `ễ`
/// (`ễ` itself does). A needle starting with a mark is matched as given. The empty needle is in
/// every text.
pub fn contains_text(haystack: &str, needle: &str) -> bool {
    let (hay, needle) = (search_text(haystack), search_text(needle));
    if needle.is_empty() {
        return true;
    }
    // Try every occurrence, overlapping ones too, so a rejected one never hides a later match.
    let mut start = 0;
    while let Some(rest) = hay.get(start..) {
        let Some(at) = rest.find(&needle) else { return false };
        let at = start.saturating_add(at);
        let end = at.saturating_add(needle.len());
        if !hay.get(end..).and_then(|t| t.chars().next()).is_some_and(is_mark) {
            return true;
        }
        // Resume one character after this occurrence's start.
        start = at.saturating_add(hay.get(at..).and_then(|t| t.chars().next()).map_or(1, char::len_utf8));
    }
    false
}

/// Compare two strings in the order of language `lang`, ignoring case.
pub fn collate(a: &str, b: &str, lang: Option<&str>) -> Ordering {
    sort_key(a, lang).cmp(&sort_key(b, lang))
}

/// The case level of a string: one flag per letter (each non-mark character of its canonical
/// form, case kept), `true` for uppercase. Lowercase sorts before uppercase (`an` < `An` < `AN`).
/// It only breaks ties between strings that [`collate`] finds equal.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CaseKey(Vec<bool>);

/// The case level of `s` ([`CaseKey`]): the same for a precomposed and a decomposed spelling.
pub fn case_key(s: &str) -> CaseKey {
    CaseKey(collate_ucd::canonical(s).chars().filter(|c| !is_mark(*c)).map(char::is_uppercase).collect())
}

/// Compare two strings in the order of language `lang`: [`collate`] (letters, then diacritics,
/// ignoring case), then, only when `case_sensitive`, their [`case_key`]s, lowercase first.
/// Strings it finds equal differ at most in encoding (and, ignoring case, in letter case); a
/// stable sort keeps them in input order.
pub fn sort_order(a: &str, b: &str, lang: Option<&str>, case_sensitive: bool) -> Ordering {
    collate(a, b, lang).then_with(|| if case_sensitive { case_key(a).cmp(&case_key(b)) } else { Ordering::Equal })
}

/// The index heading letter `s` files under in language `lang`: `Ł` for `Łódź` in Polish, `L`
/// in English; `Ch` for `chata` in Czech; `A` for `Äpfel` in German, `Ä` in Swedish.
pub fn heading_letter(s: &str, lang: Option<&str>) -> Option<String> {
    let low = lower_canonical(s.trim_start());
    if is_vi(lang) {
        return vi_heading_letter(&low);
    }
    let rules = tailoring(lang);
    let composed = compose_first_marks(&low, rules);
    let (e, _) = next_elem(&composed, rules)?;
    shown_letter(&e)
}

/// The heading letter of `low` (lowercase, canonical) in `vi`: the first letter with its quality
/// mark (`Â` for `Ấn`), tones dropped (`A` for `Ảnh`). Marks with no letter before them are
/// skipped, as the key skips them.
fn vi_heading_letter(low: &str) -> Option<String> {
    let mut chars = low.chars().skip_while(|c| is_mark(*c));
    let c = chars.next()?;
    let marks: Vec<char> = chars.take_while(|m| is_mark(*m)).collect();
    let base = vi_cluster(c, &marks).base;
    let mut buf = [0u8; 4];
    let (e, _) = next_elem(base.encode_utf8(&mut buf), VI)?;
    shown_letter(&e)
}

/// How element `e` shows as a heading letter: a letter of its own as is, else its base letter,
/// uppercased.
fn shown_letter(e: &Elem<'_>) -> Option<String> {
    let shown = if e.own || e.sec == 0 {
        e.text.to_string()
    } else {
        DECOMP.iter().find(|(x, _, _)| e.text.starts_with(*x)).map(|(_, b, _)| b.chars().take(1).collect()).unwrap_or_else(|| e.text.to_string())
    };
    let mut cs = shown.chars();
    let first = cs.next()?;
    Some(first.to_uppercase().chain(cs).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(words: &[&str], lang: &str) -> Vec<String> {
        let mut v: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        v.sort_by(|a, b| collate(a, b, Some(lang)));
        v
    }

    #[test]
    fn polish_letters_follow_their_base_letter() {
        assert_eq!(sorted(&["Zakopane", "Łódź", "Kraków"], "pl-PL"), ["Kraków", "Łódź", "Zakopane"]);
        assert_eq!(sorted(&["zebra", "ćma", "dom"], "pl-PL"), ["ćma", "dom", "zebra"]);
        assert_eq!(sorted(&["żaba", "źle", "zebra", "ząb", "lato", "łapa", "mama"], "pl"), ["lato", "łapa", "mama", "ząb", "zebra", "źle", "żaba"]);
        assert_eq!(sorted(&["b", "ą", "a"], "pl"), ["a", "ą", "b"]);
        // Every Polish letter is a letter of its own, after its base letter (review of #844).
        assert_eq!(sorted(&["Malbork", "Łódź", "Lublin"], "pl"), ["Lublin", "Łódź", "Malbork"]);
        assert_eq!(sorted(&["ósmy", "owca", "pies"], "pl"), ["owca", "ósmy", "pies"]);
        assert_eq!(sorted(&["ściana", "sum", "tak"], "pl"), ["sum", "ściana", "tak"]);
        assert_eq!(sorted(&["ęsi", "ezop", "fala"], "pl"), ["ezop", "ęsi", "fala"]);
        assert_eq!(sorted(&["ńa", "nz", "oko"], "pl"), ["nz", "ńa", "oko"]);
        assert_eq!(heading_letter("ósmy", Some("pl")).as_deref(), Some("Ó"));
        assert_eq!(heading_letter("ńa", Some("pl")).as_deref(), Some("Ń"));
        // Decomposed letters (`z` + combining dot above, as macOS file names write `ż`) sort like
        // the composed ones.
        assert_eq!(collate("z\u{307}aba", "żaba", Some("pl-PL")), Ordering::Equal);
        assert_eq!(collate("z\u{307}aba", "zebra", Some("pl-PL")), Ordering::Greater);
        assert_eq!(heading_letter("Z\u{307}aba", Some("pl")).as_deref(), Some("Ż"));
        assert_eq!(collate("a\u{328}", "ą", Some("pl")), Ordering::Equal);
    }

    #[test]
    fn swedish_puts_a_ring_and_umlauts_after_z() {
        assert_eq!(sorted(&["ödla", "zebra", "äpple", "åka", "apa"], "sv-SE"), ["apa", "zebra", "åka", "äpple", "ödla"]);
    }

    #[test]
    fn german_sorts_umlauts_with_their_base_letter() {
        assert_eq!(sorted(&["Zug", "Äpfel", "Apfel", "Bach"], "de-DE"), ["Apfel", "Äpfel", "Bach", "Zug"]);
        assert_eq!(sorted(&["Straße", "Strasse", "Strand"], "de"), ["Strand", "Strasse", "Straße"]);
    }

    #[test]
    fn russian_yo_sorts_with_e_instead_of_after_ya() {
        for lang in ["ru", "ru-RU", "RU_ru"] {
            assert_eq!(sorted(&["яблоко", "ёлка", "жук", "ель", "ёж", "еж"], lang), ["еж", "ёж", "ёлка", "ель", "жук", "яблоко"]);
            assert_eq!(collate("ЁЛКА", "ёлка", Some(lang)), Ordering::Equal);
            assert_eq!(heading_letter("Ёлка", Some(lang)).as_deref(), Some("Ё"));
        }
    }

    #[test]
    fn russian_decomposed_letters_match_their_composed_forms() {
        for (composed, decomposed) in [("ёлка", "е\u{308}лка"), ("Ёлка", "Е\u{308}лка"), ("йод", "и\u{306}од"), ("Йод", "И\u{306}од")]
        {
            assert_eq!(collate(composed, decomposed, Some("ru-RU")), Ordering::Equal);
            assert_eq!(heading_letter(composed, Some("ru")), heading_letter(decomposed, Some("ru")));
        }
        assert_eq!(sorted(&["к", "и\u{306}", "и"], "ru"), ["и", "и\u{306}", "к"]);
    }

    #[test]
    fn spanish_n_tilde_after_n() {
        assert_eq!(sorted(&["oso", "ñu", "nube"], "es-ES"), ["nube", "ñu", "oso"]);
    }

    #[test]
    fn czech_slovak_hungarian_alphabets() {
        assert_eq!(sorted(&["chata", "hrad", "ihla", "cesta", "čaj", "dům"], "cs-CZ"), ["cesta", "čaj", "dům", "hrad", "chata", "ihla"]);
        assert_eq!(sorted(&["ôsmy", "oko", "pes", "ä", "b"], "sk"), ["ä", "b", "oko", "ôsmy", "pes"]);
        assert_eq!(sorted(&["csak", "cukor", "dob", "szó", "sűrű", "tál"], "hu-HU"), ["cukor", "csak", "dob", "sűrű", "szó", "tál"]);
        // ö and ő are one letter after o, told apart by the accent only.
        assert_eq!(sorted(&["ötös", "őr", "pálya", "olló", "ör"], "hu"), ["olló", "ör", "őr", "ötös", "pálya"]);
    }

    #[test]
    fn english_keeps_case_insensitive_order_and_folds_accents() {
        assert_eq!(sorted(&["rex", "résumé", "resume", "Apple", "banana"], "en-US"), ["Apple", "banana", "resume", "résumé", "rex"]);
        assert_eq!(collate("Łódź", "Lodz", None), Ordering::Greater);
        assert_eq!(collate("Łódź", "Lodzia", None), Ordering::Less);
        assert_eq!(collate("ABC", "abc", None), Ordering::Equal);
    }

    #[test]
    fn heading_letters() {
        assert_eq!(heading_letter("Łódź", Some("pl-PL")).as_deref(), Some("Ł"));
        assert_eq!(heading_letter("Łódź", Some("en-US")).as_deref(), Some("L"));
        assert_eq!(heading_letter("chata", Some("cs")).as_deref(), Some("Ch"));
        assert_eq!(heading_letter("Äpfel", Some("de")).as_deref(), Some("A"));
        assert_eq!(heading_letter("Äpple", Some("sv")).as_deref(), Some("Ä"));
        assert_eq!(heading_letter("", Some("sv")), None);
    }

    #[test]
    fn layer1_vietnamese_letters_sort_next_to_their_base_letter() {
        // Expected order from the analysis vectors (ICU `en`; no test here runs ICU for a non-Vi order):
        // Ánh next to Anh, no longer after Z.
        // Primary first: "an" is a prefix of "anh", so Ấn sorts before Anh.
        assert_eq!(sorted(&["Zambia", "Ấn", "Ánh", "Anh", "Bình"], "en-US"), ["Ấn", "Anh", "Ánh", "Bình", "Zambia"]);
        assert_eq!(sorted(&["Huỳnh", "Hoàng", "Hồ"], "en"), ["Hồ", "Hoàng", "Huỳnh"]);
        assert_eq!(collate("ấ", "ä", None), Ordering::Less); // vectors.json collisions.en_less
        assert_eq!(sorted(&["ơn", "zebra", "ox", "ôm"], "en"), ["ôm", "ơn", "ox", "zebra"]);
    }

    #[test]
    fn layer1_canonical_equivalents_compare_equal() {
        // Every canonical_equal pair of vectors.json, with no language and with `en`.
        for (a, b) in [
            ("ậ", "a\u{302}\u{323}"),
            ("ặ", "a\u{306}\u{323}"),
            ("ệ", "e\u{302}\u{323}"),
            ("ộ", "o\u{302}\u{323}"),
            ("ợ", "o\u{323}\u{31b}"),
            ("ự", "u\u{323}\u{31b}"),
            ("Nguyễn", "Nguye\u{302}\u{303}n"),
            ("Ánh", "A\u{301}nh"),
        ] {
            for l in [None, Some("en")] {
                assert_eq!(collate(a, b, l), Ordering::Equal, "{a} {l:?}");
                assert_eq!(sort_key(a, l), sort_key(b, l), "{a} {l:?}");
            }
        }
        // canonical_not_equal: tilde and circumflex share a combining class, so their order counts.
        assert_ne!(collate("ễ", "e\u{303}\u{302}", None), Ordering::Equal);
    }

    #[test]
    fn layer1_distinct_mark_sets_never_collide() {
        // vectors.json collisions.all_distinct_vi: the primary and secondary weights alone (not
        // the `text` tie-break) tell all 23 strings apart.
        let words = ["a", "à", "ả", "ã", "á", "ạ", "ă", "ằ", "ẳ", "ẵ", "ắ", "ặ", "â", "ầ", "ẩ", "ẫ", "ấ", "ậ", "ä", "å", "ā", "ą", "ǎ"];
        assert_eq!(words.len(), 23);
        let weights: std::collections::HashSet<_> = words
            .iter()
            .map(|w| {
                let k = sort_key(w, None);
                (k.primary, k.secondary)
            })
            .collect();
        assert_eq!(weights.len(), words.len());
        // Typed decomposed, each gets the same weights as typed precomposed.
        for w in words {
            let (a, b) = (sort_key(w, None), sort_key(&crate::collate_ucd::canonical(w), None));
            assert_eq!((a.primary, a.secondary), (b.primary, b.secondary), "{w}");
        }
        // And each of them still sorts with `a`: between "a" and "b".
        for w in words {
            assert_ne!(collate(w, "a", None), Ordering::Less, "{w}");
            assert_eq!(collate(w, "b", None), Ordering::Less, "{w}");
        }
    }

    #[test]
    fn decomposed_letters_recompose_to_the_letters_the_tables_name() {
        // Rule 2 must rebuild every precomposed letter `DECOMP` and the tailorings name, or a
        // tailoring would silently stop matching once the text is in canonical (decomposed) form.
        for (x, _, _) in DECOMP {
            let s = x.to_string();
            assert_eq!(compose_first_marks(&lower_canonical(&s), &[]), s, "U+{:04X}", *x as u32);
        }
        for rules in [PL, CS, SK, HU, SV, DA, ES, TR, HR, RO, ET, LT, LV, IS] {
            for r in rules {
                assert_eq!(compose_first_marks(&lower_canonical(r.seq), rules), r.seq);
            }
        }
    }

    #[test]
    fn hostile_marks_and_huge_strings_never_panic() {
        let marks = "a".to_string() + &"\u{323}\u{302}".repeat(5_000);
        assert_eq!(marks.chars().count(), 10_001);
        let unit = "Nguye\u{302}\u{303}n Va\u{306}n Thi\u{323} A\u{301}nh ";
        let big = unit.repeat(1_000_000 / unit.len() + 1);
        assert!(big.len() >= 1_000_000);
        let cases = ["", "\u{301}", "a\u{301}\u{301}\u{301}", "dzs", "\u{10FFFF}", "ẞ", "İstanbul", "\u{301}a", &marks, &big];
        for s in cases {
            for l in [None, Some("vi"), Some("pl"), Some("hu"), Some(""), Some("-"), Some("tr-TR")] {
                let k = sort_key(s, l);
                assert!(k.primary.len() <= 16_384);
                assert_eq!(k.primary.len(), k.secondary.len());
                let _ = heading_letter(s, l);
            }
        }
        let k = sort_key(&big, None);
        assert_eq!(k.primary.len(), 16_384);
        assert!(k.text.len() >= 1_000_000, "text keeps the whole string");
        // A mark with nothing before it has nothing to attach to: it leaves the weights alone and
        // stays in the text.
        let k = sort_key("\u{301}a", None);
        assert_eq!((k.primary, k.text.as_str()), (sort_key("a", None).primary, "\u{301}a"));
    }

    #[test]
    fn vi_distinct_mark_sets_never_collide() {
        // vectors.json collisions.all_distinct_vi in `vi`: the weights alone, without the `text`
        // tie-break, tell them apart, and precomposed and decomposed spellings get one key.
        let words = ["a", "à", "ả", "ã", "á", "ạ", "ă", "ằ", "ẳ", "ẵ", "ắ", "ặ", "â", "ầ", "ẩ", "ẫ", "ấ", "ậ", "ä", "å", "ā", "ą", "ǎ"];
        let weights: std::collections::HashSet<_> = words
            .iter()
            .map(|w| {
                let k = sort_key(w, Some("vi"));
                (k.primary, k.secondary)
            })
            .collect();
        assert_eq!(weights.len(), words.len());
        for w in words {
            assert_eq!(sort_key(w, Some("vi")), sort_key(&crate::collate_ucd::canonical(w), Some("vi")), "{w}");
        }
        // Every secondary list starts with the tone.
        let k = sort_key("Nguyễn Thị Ánh", Some("vi"));
        assert_eq!(k.primary.len(), k.secondary.len());
        assert!(k.secondary.iter().all(|s| s.first().is_some_and(|t| *t <= 5)));
        // A quality mark blocked by an earlier mark of the same class does not merge: e + tilde +
        // circumflex stays e (with two marks), not ê.
        let (merged, blocked) = (sort_key("ễ", Some("vi")), sort_key("e\u{303}\u{302}", Some("vi")));
        assert_ne!(merged.primary, blocked.primary);
        assert_eq!(blocked.primary, sort_key("e", Some("vi")).primary);
    }

    #[test]
    fn tone_marks_typed_as_their_singletons_are_the_tones() {
        // U+0340 and U+0341 (the grave and acute tone marks) decompose to U+0300 and U+0301, and
        // U+0344 to U+0308 U+0301.
        let pairs = [("a\u{341}", "á"), ("a\u{340}", "à"), ("MA\u{341}", "MÁ"), ("Ă\u{341}", "Ắ"), ("Hoa\u{340}", "Hoà"), ("u\u{344}", "ǘ")];
        for lang in [None, Some("vi"), Some("pl"), Some("sv")] {
            for (single, usual) in pairs {
                assert_eq!(collate(single, usual, lang), Ordering::Equal, "{single:?} {lang:?}");
                assert_eq!(sort_key(single, lang), sort_key(usual, lang), "{single:?} {lang:?}");
                assert_eq!(sort_order(single, usual, lang, true), Ordering::Equal, "{single:?} {lang:?}");
            }
        }
        assert!(contains_text("Ma\u{341} Hoa\u{340}", "má hoà"));
        assert!(contains_text("Má Hoà", "ma\u{341}"));
        assert_eq!(heading_letter("A\u{341}nh", Some("vi")), heading_letter("Ánh", Some("vi")));
        // Under `vi` they are huyền and sắc: `ma` + U+0341 sorts exactly where `má` does.
        assert_eq!(sort_key("ma\u{340}", Some("vi")).secondary.get(1).and_then(|s| s.first()), Some(&1));
        assert_eq!(sort_key("ma\u{341}", Some("vi")).secondary.get(1).and_then(|s| s.first()), Some(&4));
        let row = ["ma", "ma\u{340}", "mả", "mã", "ma\u{341}", "mạ"];
        let mut v = row;
        v.reverse();
        v.sort_by(|a, b| collate(a, b, Some("vi")));
        assert_eq!(v, row);
    }

    #[test]
    fn search_text_matches_across_case_and_encoding() {
        assert_eq!(search_text("NGUYỄN"), search_text("nguye\u{302}\u{303}n"));
        assert_eq!(search_text("Ánh"), sort_key("A\u{301}nh", Some("vi")).text);
        assert!(search_text("Ông NGUYỄN Văn An").contains(&search_text("nguyễn")));
        assert_eq!(search_text("Mail@X.org"), "mail@x.org");
    }

    #[test]
    fn contains_text_matches_whole_letters() {
        // A base letter or a letter with fewer marks never matches inside a marked letter.
        assert!(!contains_text("Nguyễn", "e"));
        assert!(contains_text("Nguyễn", "ễ"));
        assert!(contains_text("Nguye\u{302}\u{303}n", "nguyễn"));
        assert!(contains_text("NGUYE\u{302}\u{303}N", "Nguyễn"));
        assert!(!contains_text("Ánh", "a"));
        assert!(contains_text("Ánh", "á"));
        assert!(!contains_text("ế", "é"));
        assert!(!contains_text("ễ", "ê"));
        assert!(contains_text("Hà Nội và Hà Giang", "giang"));
        assert!(contains_text("abc", ""));
        assert!(contains_text("", ""));
        // A rejected occurrence does not hide a later whole-letter one.
        assert!(contains_text("ễ e", "e"));
        assert!(contains_text("Ánh Anh", "anh"));
        // A needle that starts with a mark is taken as given.
        assert!(contains_text("ễ", "\u{303}"));
        assert!(!contains_text("abc", "abcd"));
        // An overlapping later occurrence is found after a rejected one.
        assert!(contains_text("e\u{302}e\u{302}e", "e\u{302}e"));
        for (h, n) in [("\u{301}", "\u{301}"), ("a\u{301}\u{301}", "\u{301}"), ("\u{10FFFF}", "a"), ("İstanbul", "i"), ("ẞ", "ss")] {
            let _ = contains_text(h, n);
        }
    }

    #[test]
    fn hostile_input_never_panics() {
        for s in ["", "\u{301}", "a\u{301}\u{301}\u{301}", "dzs", "\u{10FFFF}", "ẞ", "İstanbul"] {
            for l in [None, Some("hu"), Some(""), Some("-"), Some("tr-TR")] {
                let _ = sort_key(s, l);
                let _ = heading_letter(s, l);
            }
        }
    }
}
