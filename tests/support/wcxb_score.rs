//! WCXB's pinned `evaluate.py` scoring semantics.
//!
//! Python's `\w` is Unicode letters and numbers plus underscore. The
//! general-category table is pinned because Rust's `is_alphanumeric` also
//! accepts some combining marks that Python's regular expression rejects.

use std::collections::HashMap;
use unicode_general_category::{get_general_category, GeneralCategory};

pub fn word_prf(predicted: &str, reference: &str) -> (f64, f64, f64) {
    let predicted = word_counter(predicted);
    let reference = word_counter(reference);
    if reference.is_empty() {
        return if predicted.is_empty() {
            (1.0, 1.0, 1.0)
        } else {
            (0.0, 0.0, 0.0)
        };
    }
    if predicted.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let overlap: usize = predicted
        .iter()
        .filter_map(|(word, &count)| reference.get(word).map(|&other| count.min(other)))
        .sum();
    let predicted_total: usize = predicted.values().sum();
    let reference_total: usize = reference.values().sum();
    let precision = overlap as f64 / predicted_total as f64;
    let recall = overlap as f64 / reference_total as f64;
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    (precision, recall, f1)
}

pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();

    // The order matters: Python lowercases the whole string before applying
    // `\w+`. U+0130 therefore becomes `i` + a combining dot, which splits.
    for character in text.to_lowercase().chars() {
        if character == '_' || is_python_alphanumeric(character) {
            token.push(character);
        } else if !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

fn word_counter(text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for token in tokenize(text) {
        *counts.entry(token).or_insert(0) += 1;
    }
    counts
}

fn is_python_alphanumeric(character: char) -> bool {
    matches!(
        get_general_category(character),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
            | GeneralCategory::DecimalNumber
            | GeneralCategory::LetterNumber
            | GeneralCategory::OtherNumber
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_pinned_python_regex_fixtures() {
        assert_eq!(
            tokenize("can't foo-bar snake_case 中文 cafe\u{301} café"),
            [
                "can",
                "t",
                "foo",
                "bar",
                "snake_case",
                "中文",
                "cafe",
                "café"
            ]
        );
        assert_eq!(tokenize("İstanbul"), ["i", "stanbul"]);
        assert_eq!(tokenize("क\u{93f} ש\u{5b0}"), ["क", "ש"]);
        assert_eq!(tokenize("² ①"), ["²", "①"]);
    }

    #[test]
    fn matches_pinned_empty_side_semantics() {
        assert_eq!(word_prf("", ""), (1.0, 1.0, 1.0));
        assert_eq!(word_prf("word", ""), (0.0, 0.0, 0.0));
        assert_eq!(word_prf("", "word"), (0.0, 0.0, 0.0));
    }

    #[test]
    fn counts_duplicate_words() {
        assert_eq!(word_prf("x x", "x"), (0.5, 1.0, 2.0 / 3.0));
    }
}
