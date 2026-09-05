//! Kanji extraction: which characters of a saved term get indexed.
//!
//! Pure and dependency-free so the interesting cases can be tested directly.

/// The character ranges Lanobe treats as kanji.
///
/// Extension B and the compatibility supplement are above U+FFFF, so this must
/// operate on `char` (a code point) and never on UTF-16 units — `𠮟` is one
/// kanji, not two.
fn is_kanji(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'      // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}'    // Extension A
        | '\u{20000}'..='\u{2EBEF}'  // Extension B through F
        | '\u{2F800}'..='\u{2FA1F}'  // Compatibility Ideographs Supplement
    )
}

/// Every distinct kanji in `term`, in first-appearance order.
///
/// Deliberately excludes `々` (U+3005) and `ヶ` (U+30F6). Both sit outside the
/// ranges above, but they are easy to add by reaching for a broader "CJK" test —
/// and they are iteration and abbreviation marks, not characters anyone studies.
/// Indexing them would highlight the mark in `人々` and `一ヶ月` in every book,
/// which is noise rather than vocabulary.
///
/// Order matters: `saved_kanji.first_term_id` should point at the term that
/// introduced a character, so callers insert in the order returned here.
pub fn extract_kanji(term: &str) -> Vec<char> {
    let mut seen = Vec::new();
    for c in term.chars() {
        if is_kanji(c) && !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::extract_kanji;

    #[test]
    fn extracts_each_kanji_once_in_first_appearance_order() {
        assert_eq!(extract_kanji("図書館"), vec!['図', '書', '館']);
        // 日 appears twice; the index stores it once, keyed on its first sighting.
        assert_eq!(extract_kanji("日曜日"), vec!['日', '曜']);
    }

    #[test]
    fn kana_and_punctuation_are_not_kanji() {
        assert!(extract_kanji("ある").is_empty());
        assert!(extract_kanji("コーヒー").is_empty());
        assert!(extract_kanji("、。！？").is_empty());
        assert!(extract_kanji("").is_empty());
        // Mixed: only the kanji stem is indexed, not the okurigana.
        assert_eq!(extract_kanji("食べる"), vec!['食']);
    }

    #[test]
    fn iteration_and_abbreviation_marks_are_excluded() {
        // 々 repeats the previous character and ヶ abbreviates 箇 — neither is a
        // character to study, and indexing them would highlight them everywhere.
        assert_eq!(extract_kanji("人々"), vec!['人']);
        assert_eq!(extract_kanji("一ヶ月"), vec!['一', '月']);
        assert_eq!(extract_kanji("時々"), vec!['時']);
    }

    #[test]
    fn characters_above_the_bmp_count_as_one_kanji() {
        // 𠮟 is U+20B9F, a surrogate pair in UTF-16. Getting this wrong would
        // index two bogus half-characters that match nothing.
        let extracted = extract_kanji("𠮟る");
        assert_eq!(extracted, vec!['\u{20B9F}']);
        assert_eq!(extracted.len(), 1);
    }

    #[test]
    fn extension_a_is_included() {
        assert_eq!(extract_kanji("㐀"), vec!['\u{3400}']);
    }

    #[test]
    fn latin_and_digits_are_not_kanji() {
        assert!(extract_kanji("LINE").is_empty());
        assert!(extract_kanji("2026年").len() == 1);
    }
}
