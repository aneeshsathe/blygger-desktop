//! Studio 0.68's `isNearSame`, case by case (`src/change-size.ts`).

use super::*;

#[test]
fn the_edit_span_is_what_lies_between_the_common_ends() {
    assert_eq!(edit_span("same", "same"), 0);
    assert_eq!(edit_span("", ""), 0);
    assert_eq!(edit_span("abc", ""), 3);
    // One letter changed in the middle.
    assert_eq!(edit_span("the cat sat", "the bat sat"), 1);
    // Inserted text counts in the longer text.
    assert_eq!(edit_span("ab", "aXYZb"), 3);
    // Prefix and suffix don't overlap: "aaa" → "aa" is one.
    assert_eq!(edit_span("aaa", "aa"), 1);
    // An upper bound: two far-apart edits span everything between them.
    assert_eq!(edit_span("Xbcdefghij", "abcdefghiY"), 10);
}

#[test]
fn lengths_are_utf16_units_as_the_studio_counts_them() {
    // An emoji is two units in JavaScript's charCodeAt.
    assert_eq!(edit_span("a😀b", "ab"), 2);
    assert_eq!(edit_span("é", "e"), 1);
}

#[test]
fn near_same_is_at_most_40_characters_and_2_percent() {
    let long = "x".repeat(2000);
    // 40 of 2000 = 2%: near-same at both limits.
    let edited = format!("{}{}", "y".repeat(40), &long[40..]);
    assert!(is_near_same(&long, &edited));
    // 41 characters: past the character limit.
    let edited = format!("{}{}", "y".repeat(41), &long[41..]);
    assert!(!is_near_same(&long, &edited));
    // A short text: one character of 40 is 2.5%, too much.
    let short = "a".repeat(40);
    let one = format!("b{}", &short[1..]);
    assert!(!is_near_same(&short, &one));
    // One of 50 is exactly 2%.
    let fifty = "a".repeat(50);
    assert!(is_near_same(&fifty, &format!("b{}", &fifty[1..])));
    // The same text, and two empty texts, are near-same.
    assert!(is_near_same("same words", "same words"));
    assert!(is_near_same("", ""));
    // Anything from nothing is not.
    assert!(!is_near_same("", "a"));
    assert_eq!(NEAR_SAME_MAX_CHARS, 40);
    assert!((NEAR_SAME_MAX_FRACTION - 0.02).abs() < f64::EPSILON);
}
