//! Agreement of the hand-written Vietnamese collation with ICU4X (`icu_collator`, CLDR `vi`
//! standard, default tertiary strength), and order properties that hold for any input.
//!
//! ICU is only an oracle here (a dev-dependency, never shipped). Generated names are letters and
//! spaces only: punctuation is ordered by code point and letters outside the language's alphabet
//! are kept apart by code point, and both differ from ICU on purpose.

use std::cmp::Ordering;
use std::collections::HashSet;

use icu_collator::options::CollatorOptions;
use icu_collator::{CollatorBorrowed, CollatorPreferences};
use icu_locale_core::locale;
use icu_normalizer::DecomposingNormalizerBorrowed;
use icu_normalizer::properties::CanonicalCombiningClassMapBorrowed;
use proptest::prelude::*;
use wordcraft_doc::collate::{collate, sort_order};

/// The syllables the name generator draws from (the list of the M3 collation prototype).
const SYLL: &[&str] = &[
    "an", "anh", "ang", "ao", "au", "ăn", "ăm", "âu", "ân", "ấp", "ba", "bình", "bùi", "cao", "công", "cường", "chi", "chu", "dũng", "duy", "dương",
    "dung", "đào", "đặng", "đinh", "đỗ", "đức", "em", "ê", "giang", "hà", "hạ", "hải", "hoa", "hoà", "hòa", "hồ", "hồng", "huệ", "huy", "huỳnh",
    "khánh", "khoa", "lan", "lê", "linh", "loan", "long", "lý", "mai", "minh", "my", "nam", "nga", "ngô", "ngọc", "nguyễn", "nhung", "oanh", "ông",
    "ơn", "phan", "phạm", "phúc", "phương", "quang", "quân", "quỳnh", "sơn", "tâm", "thanh", "thảo", "thu", "thư", "tiến", "trang", "trần", "trinh",
    "trung", "tuấn", "tú", "uyên", "út", "ưng", "văn", "vân", "việt", "võ", "vũ", "xuân", "yến", "ậu", "ặc", "ệ", "ộ", "ợ", "ự",
];

/// A deterministic 64-bit linear congruential generator (Knuth's MMIX constants).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    /// A number in `[0, n)` (`n > 0`).
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    /// A number in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        self.next() as f64 / (1u64 << 53) as f64
    }
}

fn nfd(s: &str) -> String {
    DecomposingNormalizerBorrowed::new_nfd().normalize(s).into_owned()
}

/// Canonically equivalent to `s` but not in canonical order: marks of different combining class
/// swapped (a dot below typed after the circumflex).
fn scramble(s: &str) -> String {
    let ccc = CanonicalCombiningClassMapBorrowed::new();
    let mut out: Vec<char> = Vec::new();
    for c in nfd(s).chars() {
        let class = ccc.get_u8(c);
        let prev = out.last().map(|p| ccc.get_u8(*p)).unwrap_or(0);
        if class != 0 && prev != 0 && class != prev {
            let at = out.len() - 1;
            out.insert(at, c);
        } else {
            out.push(c);
        }
    }
    out.into_iter().collect()
}

/// Capitalise the first letter of each space-separated word.
fn title(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(f) => f.to_uppercase().chain(cs).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `n` distinct Vietnamese-looking names, deterministic in `seed`: one to three syllables,
/// Capitalised words (60%), ALL CAPS (10%) or lowercase; then about 30% decomposed (NFD) and
/// 10% with marks in a non-canonical order. Letters and spaces only.
fn names(n: usize, seed: u64) -> Vec<String> {
    let mut rng = Lcg(seed);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    while out.len() < n {
        let count = 1 + rng.below(3);
        let words: Vec<&str> = (0..count).map(|_| SYLL[rng.below(SYLL.len() as u64) as usize]).collect();
        let mut name = words.join(" ");
        let r = rng.unit();
        name = if r < 0.6 {
            title(&name)
        } else if r < 0.7 {
            name.to_uppercase()
        } else {
            name
        };
        let r = rng.unit();
        if r < 0.1 {
            name = scramble(&name);
        } else if r < 0.4 {
            name = nfd(&name);
        }
        if seen.insert(name.clone()) {
            out.push(name);
        }
    }
    out
}

fn icu_vi() -> CollatorBorrowed<'static> {
    CollatorBorrowed::try_new(CollatorPreferences::from(&locale!("vi")), CollatorOptions::default()).expect("compiled vi collation data")
}

fn ours(a: &str, b: &str) -> Ordering {
    sort_order(a, b, Some("vi"), true)
}

fn esc(s: &str) -> String {
    s.escape_default().to_string()
}

#[test]
fn generator_is_deterministic_and_varied() {
    let a = names(500, 7);
    assert_eq!(a, names(500, 7));
    assert_ne!(a, names(500, 8));
    assert_eq!(a.iter().collect::<HashSet<_>>().len(), 500);
    assert!(a.iter().any(|s| s != &nfd(s) && s.chars().any(|c| c.is_uppercase())), "some decomposed names");
    assert!(a.iter().any(|s| s.contains('\u{323}') && s.contains('\u{302}') && *s != nfd(s)), "some non-canonical mark order");
    assert!(a.iter().all(|s| s.chars().all(|c| c.is_alphabetic() || c == ' ' || (('\u{300}'..='\u{36f}').contains(&c)))));
}

#[test]
fn agrees_with_icu_vi_on_generated_names() {
    let icu = icu_vi();
    let input = names(2000, 7);
    let mut by_ours = input.clone();
    by_ours.sort_by(|a, b| ours(a, b));
    let mut by_icu = input.clone();
    by_icu.sort_by(|a, b| icu.compare(a, b));

    if by_ours != by_icu {
        // Up to ten pairs the two orders disagree on, for the report: neighbours in ICU's order
        // that ours ranks differently, then pairs from the first names.
        let mut bad = Vec::new();
        let mut note = |a: &String, b: &String| {
            if bad.len() < 10 && ours(a, b) != icu.compare(a, b) {
                bad.push(format!("{} | {}: ours {:?}, icu {:?}", esc(a), esc(b), ours(a, b), icu.compare(a, b)));
            }
        };
        for w in by_icu.windows(2) {
            if let [a, b] = w {
                note(a, b);
            }
        }
        let head = &input[..input.len().min(300)];
        for (i, a) in head.iter().enumerate() {
            for b in &head[i + 1..] {
                note(a, b);
            }
        }
        panic!("order differs from ICU vi; disagreeing pairs:\n{}", bad.join("\n"));
    }
    for w in by_ours.windows(2) {
        if let [a, b] = w {
            assert_eq!(ours(a, b) == Ordering::Equal, icu.compare(a, b) == Ordering::Equal, "{} / {}", esc(a), esc(b));
        }
    }
}

/// Every tone on a set of vowels and syllables, each in precomposed and decomposed spelling and
/// in two cases. The generated names rarely put two tones of one letter side by side, so this
/// pins the tone order itself.
#[test]
fn agrees_with_icu_vi_on_tone_matrix() {
    let icu = icu_vi();
    let tones = ["", "\u{300}", "\u{309}", "\u{303}", "\u{301}", "\u{323}"];
    let mut input = Vec::new();
    for base in ["a", "\u{103}", "\u{e2}", "e", "\u{ea}", "i", "o", "\u{f4}", "\u{1a1}", "u", "\u{1b0}", "y", "ma", "ng\u{1ecd}c", "hoa"] {
        for tone in tones {
            let word = format!("{}{tone}", nfd(base));
            input.push(word.clone());
            input.push(word.to_uppercase());
            input.push(nfd(&word));
            input.push(title(&word));
            // The grave and acute tone marks U+0340 and U+0341, which decompose to U+0300 and U+0301.
            input.push(word.replace('\u{300}', "\u{340}").replace('\u{301}', "\u{341}"));
        }
    }
    let mut by_ours = input.clone();
    by_ours.sort_by(|a, b| ours(a, b));
    let mut by_icu = input.clone();
    by_icu.sort_by(|a, b| icu.compare(a, b));
    assert_eq!(by_ours, by_icu);
}

const V1_INPUT: &[&str] = &[
    "Yến", "Ưng", "Uyên", "Út", "Phan", "Phạm", "Ơn", "Ông", "Oanh", "Nguyễn", "Ngô", "Huỳnh", "Hồ", "Hoàng", "Ê-đê", "Em", "Đỗ", "Đinh", "Đặng",
    "Đào", "Dương", "Duy", "Dũng", "Bùi", "Bình", "Âu", "Ấn", "Ân", "Ăn", "Ắc", "Ánh", "Ảnh", "Anh", "An",
];

const V3_INPUT: &[&str] = &[
    "Nguyễn Văn Ấn",
    "Nguyen Thi Ngoc Anh",
    "Nguyễn Thị Ngọc Ánh",
    "Nguyễn Thị Ngọc Anh",
    "Nguyễn Văn An",
    "Ngô Bảo Châu",
    "Nguyễn Thị Ngọc Ảnh",
    "Nguyễn Văn Ân",
    "Đắk Lắk",
    "Đà Nẵng",
    "Hà Nội",
    "Hải Phòng",
    "Huế",
    "Hà Giang",
    "Hòa Bình",
    "Hoà Bình",
    "Cần Thơ",
    "Bà Rịa Vũng Tàu",
];

const V2_ROWS: &[&[&str]] =
    &[&["ma", "mà", "mả", "mã", "má", "mạ"], &["MA", "MÀ", "MẢ", "MÃ", "MÁ", "MẠ"], &["ă", "ằ", "ẳ", "ẵ", "ắ", "ặ"], &["ơ", "ờ", "ở", "ỡ", "ớ", "ợ"]];

#[test]
fn agrees_with_icu_vi_on_spec_vectors() {
    let icu = icu_vi();
    for input in [V1_INPUT, V3_INPUT] {
        let mut by_ours: Vec<&str> = input.to_vec();
        by_ours.sort_by(|a, b| ours(a, b));
        let mut by_icu: Vec<&str> = input.to_vec();
        by_icu.sort_by(|a, b| icu.compare(a, b));
        assert_eq!(by_ours, by_icu);
    }
    // The tone rows (V2) are strictly increasing in both.
    for row in V2_ROWS {
        for w in row.windows(2) {
            if let [a, b] = w {
                assert_eq!(icu.compare(a, b), Ordering::Less, "icu {a} < {b}");
                assert_eq!(ours(a, b), Ordering::Less, "ours {a} < {b}");
            }
        }
    }
}

/// One generated piece: a name syllable in some encoding and case, ASCII, loose combining marks,
/// or any scalar value.
fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        (0..SYLL.len(), 0u8..5).prop_map(|(i, how)| {
            let s = SYLL.get(i).copied().unwrap_or("a");
            match how {
                0 => s.to_string(),
                1 => nfd(s),
                2 => scramble(s),
                3 => title(s),
                _ => nfd(&s.to_uppercase()),
            }
        }),
        "[ -~]{0,4}",
        "[\u{300}-\u{36f}]{1,3}",
        any::<char>().prop_map(|c| c.to_string()),
    ]
}

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(piece(), 0..6).prop_map(|v| v.concat())
}

fn lang() -> impl Strategy<Value = Option<&'static str>> {
    prop_oneof![Just(None), Just(Some("vi")), Just(Some("pl"))]
}

/// `cmp` is a total preorder on `a`, `b`, `c`: reflexive, antisymmetric, transitive.
fn check_total_order(a: &str, b: &str, c: &str, cmp: impl Fn(&str, &str) -> Ordering) -> Result<(), TestCaseError> {
    for x in [a, b, c] {
        prop_assert_eq!(cmp(x, x), Ordering::Equal);
    }
    for (x, y) in [(a, b), (b, c), (a, c)] {
        prop_assert_eq!(cmp(x, y), cmp(y, x).reverse());
    }
    for (x, y, z) in [(a, b, c), (a, c, b), (b, a, c), (b, c, a), (c, a, b), (c, b, a)] {
        if cmp(x, y) != Ordering::Greater && cmp(y, z) != Ordering::Greater {
            prop_assert!(cmp(x, z) != Ordering::Greater, "{} <= {} <= {} but not {} <= {}", esc(x), esc(y), esc(z), esc(x), esc(z));
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn total_order(a in text(), b in text(), c in text(), lang in lang()) {
        check_total_order(&a, &b, &c, |x, y| collate(x, y, lang))?;
        check_total_order(&a, &b, &c, |x, y| sort_order(x, y, lang, true))?;
    }

    #[test]
    fn equality_is_language_independent(a in text(), b in text()) {
        prop_assert_eq!(
            collate(&a, &b, None) == Ordering::Equal,
            collate(&a, &b, Some("vi")) == Ordering::Equal,
            "{} / {}", esc(&a), esc(&b)
        );
    }
}
