//! Vietnamese collation (CLDR `vi` standard) through the public API: tailored letters, tones as
//! a secondary difference, the case level, heading letters and language-tag handling.
//!
//! Expected orders are the `vi` vectors of the M3 collation analysis (ICU 77.1 / CLDR 47 `vi`,
//! `vectors.json`), copied here as literals.

use std::cmp::Ordering;

use wordcraft_doc::collate::{CaseKey, case_key, collate, heading_letter, sort_order};

/// `xs` sorted (stably) by `sort_order` in language `lang`.
fn sorted(xs: &[&str], lang: &str, cs: bool) -> Vec<String> {
    let mut v: Vec<String> = xs.iter().map(|s| s.to_string()).collect();
    v.sort_by(|a, b| sort_order(a, b, Some(lang), cs));
    v
}

const V1_INPUT: &[&str] = &[
    "Yến", "Ưng", "Uyên", "Út", "Phan", "Phạm", "Ơn", "Ông", "Oanh", "Nguyễn", "Ngô", "Huỳnh", "Hồ", "Hoàng", "Ê-đê", "Em", "Đỗ", "Đinh", "Đặng",
    "Đào", "Dương", "Duy", "Dũng", "Bùi", "Bình", "Âu", "Ấn", "Ân", "Ăn", "Ắc", "Ánh", "Ảnh", "Anh", "An",
];
const V1_EXPECTED: &[&str] = &[
    "An", "Anh", "Ảnh", "Ánh", "Ắc", "Ăn", "Ân", "Ấn", "Âu", "Bình", "Bùi", "Dũng", "Duy", "Dương", "Đào", "Đặng", "Đinh", "Đỗ", "Em", "Ê-đê",
    "Hoàng", "Hồ", "Huỳnh", "Ngô", "Nguyễn", "Oanh", "Ông", "Ơn", "Phạm", "Phan", "Út", "Uyên", "Ưng", "Yến",
];

const V2_EXPECTED: &[&[&str]] =
    &[&["ma", "mà", "mả", "mã", "má", "mạ"], &["MA", "MÀ", "MẢ", "MÃ", "MÁ", "MẠ"], &["ă", "ằ", "ẳ", "ẵ", "ắ", "ặ"], &["ơ", "ờ", "ở", "ỡ", "ớ", "ợ"]];

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
const V3_EXPECTED: &[&str] = &[
    "Bà Rịa Vũng Tàu",
    "Cần Thơ",
    "Đà Nẵng",
    "Đắk Lắk",
    "Hà Giang",
    "Hà Nội",
    "Hải Phòng",
    "Hoà Bình",
    "Hòa Bình",
    "Huế",
    "Ngô Bảo Châu",
    "Nguyen Thi Ngoc Anh",
    "Nguyễn Thị Ngọc Anh",
    "Nguyễn Thị Ngọc Ảnh",
    "Nguyễn Thị Ngọc Ánh",
    "Nguyễn Văn An",
    "Nguyễn Văn Ân",
    "Nguyễn Văn Ấn",
];

const CASE_SENSITIVE_EXPECTED: &[&[&str]] = &[
    &["an", "An", "AN"],
    &["\u{111}", "\u{110}"],
    &["\u{e1}nh", "\u{c1}nh", "\u{c1}NH"],
    &["nguy\u{1ec5}n v\u{103}n an", "Nguy\u{1ec5}n V\u{103}n An", "NGUY\u{1ec4}N V\u{102}N AN"],
];

const EQUAL_CI: &[(&str, &str)] = &[
    ("Nguy\u{1ec5}n V\u{103}n An", "nguy\u{1ec5}n v\u{103}n an"),
    ("\u{110}\u{e0} N\u{1eb5}ng", "\u{110}\u{c0} N\u{1eb4}NG"),
    ("\u{c1}nh", "\u{c1}NH"),
];

const CANONICAL_EQUAL: &[(&str, &str)] = &[
    ("\u{1ead}", "a\u{302}\u{323}"),
    ("\u{1eb7}", "a\u{306}\u{323}"),
    ("\u{1ec7}", "e\u{302}\u{323}"),
    ("\u{1ed9}", "o\u{302}\u{323}"),
    ("\u{1ee3}", "o\u{323}\u{31b}"),
    ("\u{1ef1}", "u\u{323}\u{31b}"),
    ("Nguy\u{1ec5}n", "Nguye\u{302}\u{303}n"),
    ("\u{c1}nh", "A\u{301}nh"),
];

/// Both marks have combining class 230: no reordering, so e + tilde + circumflex is not ễ.
const CANONICAL_NOT_EQUAL: &[(&str, &str)] = &[("\u{1ec5}", "e\u{303}\u{302}")];

const ALL_DISTINCT_VI: &[&str] = &["a", "à", "ả", "ã", "á", "ạ", "ă", "ằ", "ẳ", "ẵ", "ắ", "ặ", "â", "ầ", "ẩ", "ẫ", "ấ", "ậ", "ä", "å", "ā", "ą", "ǎ"];

const HEADING_LETTER_VI: &[(&str, &str)] = &[("Ấn", "Â"), ("Đào", "Đ"), ("Ảnh", "A"), ("ơn", "Ơ"), ("Ưng", "Ư"), ("Ê-đê", "Ê")];

/// ICU `vi` orders these differently; the hand-written key keeps code-point order for
/// punctuation and does not claim parity there.
const PUNCTUATION_CODEPOINT_ORDER: &[&str] = &[" a", "'a", ",a", "-a", ".a", "_a"];

fn assert_strictly_increasing(xs: &[&str], cmp: impl Fn(&str, &str) -> Ordering) {
    for w in xs.windows(2) {
        if let [a, b] = w {
            assert_eq!(cmp(a, b), Ordering::Less, "{a} < {b}");
            assert_eq!(cmp(b, a), Ordering::Greater, "{b} > {a}");
        }
    }
}

#[test]
fn v1_single_syllable_names() {
    assert_eq!(sorted(V1_INPUT, "vi-VN", false), V1_EXPECTED);
}

#[test]
fn v2_tones_are_secondary() {
    for xs in V2_EXPECTED {
        assert_strictly_increasing(xs, |a, b| collate(a, b, Some("vi")));
    }
}

#[test]
fn v3_multi_word_names_and_places() {
    assert_eq!(sorted(V3_INPUT, "vi", false), V3_EXPECTED);
    assert_eq!(collate("Hoà Bình", "Hòa Bình", Some("vi")), Ordering::Less);
}

#[test]
fn case_insensitive_ties_keep_input_order() {
    assert_eq!(sorted(&["AN", "an", "An"], "vi", false), ["AN", "an", "An"]);
    for (a, b) in EQUAL_CI {
        assert_eq!(collate(a, b, Some("vi")), Ordering::Equal, "{a} = {b}");
        assert_eq!(sort_order(a, b, Some("vi"), false), Ordering::Equal, "{a} = {b}");
    }
    // NFC and NFD spellings of one name are a tie: either input order is kept.
    let (nfc, nfd) = ("Nguy\u{1ec5}n", "Nguye\u{302}\u{303}n");
    assert_eq!(sorted(&[nfc, nfd], "vi", false), [nfc, nfd]);
    assert_eq!(sorted(&[nfd, nfc], "vi", false), [nfd, nfc]);
}

#[test]
fn case_sensitive_lower_first() {
    for xs in CASE_SENSITIVE_EXPECTED {
        assert_strictly_increasing(xs, |a, b| sort_order(a, b, Some("vi"), true));
    }
    // An uppercase NFD spelling against an NFC one: a total order, independent of the encoding.
    let anh_nfd_upper = "A\u{301}NH";
    assert_eq!(sort_order(anh_nfd_upper, "\u{c1}nh", Some("vi"), true), Ordering::Greater);
    assert_eq!(sort_order("\u{e1}nh", "A\u{301}nh", Some("vi"), true), Ordering::Less);
    assert_eq!(sort_order("A\u{301}nh", "\u{c1}nh", Some("vi"), true), Ordering::Equal);
    assert_eq!(sort_order(anh_nfd_upper, "\u{c1}NH", Some("vi"), true), Ordering::Equal);
    // The case level only breaks ties: it never overrides a letter or a tone difference.
    assert_eq!(sort_order("AN", "anh", Some("vi"), true), Ordering::Less);
    assert_eq!(sort_order("MA", "mà", Some("vi"), true), Ordering::Less);
    // Case keys: lowercase first, one flag per letter of the canonical form.
    assert!(case_key("an") < case_key("An"));
    assert!(case_key("An") < case_key("AN"));
    assert_eq!(case_key("A\u{301}nh"), case_key("\u{c1}nh"));
    assert_eq!(case_key(""), CaseKey::default());
}

#[test]
fn canonical_equivalence_in_vi() {
    for (a, b) in CANONICAL_EQUAL {
        assert_eq!(collate(a, b, Some("vi")), Ordering::Equal, "{a} = {b:?}");
        assert_eq!(sort_order(a, b, Some("vi"), true), Ordering::Equal, "{a} = {b:?}");
    }
    for (a, b) in CANONICAL_NOT_EQUAL {
        assert_ne!(collate(a, b, Some("vi")), Ordering::Equal, "{a} != {b:?}");
    }
}

#[test]
fn vi_letters_beat_foreign_diacritics() {
    assert_eq!(collate("ä", "ấ", Some("vi")), Ordering::Less);
    for (i, a) in ALL_DISTINCT_VI.iter().enumerate() {
        for b in ALL_DISTINCT_VI.iter().skip(i + 1) {
            assert_ne!(collate(a, b, Some("vi")), Ordering::Equal, "{a} != {b}");
        }
    }
}

#[test]
fn heading_letters_vi() {
    for (s, h) in HEADING_LETTER_VI {
        assert_eq!(heading_letter(s, Some("vi")).as_deref(), Some(*h), "{s}");
    }
    // Typed decomposed, the same letters.
    assert_eq!(heading_letter("A\u{302}\u{301}n", Some("vi")).as_deref(), Some("Â"));
    assert_eq!(heading_letter("o\u{31b}n", Some("vi")).as_deref(), Some("Ơ"));
    assert_eq!(heading_letter("", Some("vi")), None);
}

#[test]
fn punctuation_keeps_code_point_order() {
    let reversed: Vec<&str> = PUNCTUATION_CODEPOINT_ORDER.iter().rev().copied().collect();
    assert_eq!(sorted(&reversed, "vi", false), PUNCTUATION_CODEPOINT_ORDER);
}

#[test]
fn language_tag_variants() {
    for tag in ["vi", "vi-VN", "VI_vn", "vi-"] {
        assert_eq!(sorted(V1_INPUT, tag, false), V1_EXPECTED, "{tag}");
        assert_eq!(collate("Đào", "Dương", Some(tag)), Ordering::Greater, "{tag}");
    }
    // The language-neutral order treats Đ as D with a secondary difference.
    for tag in ["", "-", "zz"] {
        assert_eq!(collate("Đào", "Dương", Some(tag)), Ordering::Less, "{tag:?}");
    }
    assert_eq!(collate("Đào", "Dương", None), Ordering::Less);
}

#[test]
fn equality_does_not_depend_on_language() {
    let mut words: Vec<&str> = V1_INPUT.iter().chain(V3_INPUT).copied().collect();
    for (a, b) in CANONICAL_EQUAL.iter().chain(EQUAL_CI) {
        words.push(a);
        words.push(b);
    }
    for a in &words {
        for b in &words {
            let root = collate(a, b, None) == Ordering::Equal;
            let vi = collate(a, b, Some("vi")) == Ordering::Equal;
            assert_eq!(root, vi, "{a:?} {b:?}");
        }
    }
}
