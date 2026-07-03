//! Convert bracketed furigana to HTML `<ruby>` markup via `mecab-furigana-rs`.
//!
//! Thin delegation — the actual parser and HTML renderer live upstream in the
//! `mecab-furigana-rs` crate so that any consumer can render furigana without
//! reimplementing the bracket-format parser.

/// Convert a bracketed furigana string to `<ruby>` element markup for the HUD overlay.
///
/// Delegates to `mecab_furigana_rs::furigana_to_html()`.
///
/// # Examples
///
/// ```
/// use lenzu::furigana_html::bracketed_to_furigana_html;
///
/// let html = bracketed_to_furigana_html("食[た]べ物[もの]");
/// assert_eq!(
///     html,
///     "<ruby>食<rt>た</rt></ruby>べ<ruby>物<rt>もの</rt></ruby>"
/// );
/// ```
pub fn bracketed_to_furigana_html(input: &str) -> String {
    mecab_furigana_rs::furigana_to_html(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Happy paths ──────────────────────────────────────────────────────────

    #[test]
    fn basic_single_kanji_with_reading() {
        assert_eq!(
            bracketed_to_furigana_html("食[た]べる"),
            "<ruby>食<rt>た</rt></ruby>べる"
        );
    }

    #[test]
    fn multiple_kanji_groups_with_readings() {
        assert_eq!(
            bracketed_to_furigana_html("食[た]べ物[もの]が好[す]き"),
            "<ruby>食<rt>た</rt></ruby>べ<ruby>物<rt>もの</rt></ruby>が<ruby>好<rt>す</rt></ruby>き"
        );
    }

    #[test]
    fn consecutive_kanji_groups() {
        assert_eq!(
            bracketed_to_furigana_html("東京[とうきょう]大阪[おおさか]"),
            "<ruby>東京<rt>とうきょう</rt></ruby><ruby>大阪<rt>おおさか</rt></ruby>"
        );
    }

    #[test]
    fn kanji_with_multi_char_reading() {
        assert_eq!(
            bracketed_to_furigana_html("日本語[にほんご]"),
            "<ruby>日本語<rt>にほんご</rt></ruby>"
        );
    }

    // ── Edge cases ───────────────────────────────────────────────────────────

    #[test]
    fn no_brackets_passthrough() {
        assert_eq!(bracketed_to_furigana_html("日本語"), "日本語");
    }

    #[test]
    fn empty_string() {
        assert_eq!(bracketed_to_furigana_html(""), "");
    }

    #[test]
    fn bracket_without_preceding_kanji_is_literal() {
        assert_eq!(bracketed_to_furigana_html("[hello]"), "[hello]");
    }

    #[test]
    fn empty_reading_after_kanji() {
        assert_eq!(
            bracketed_to_furigana_html("漢字[]"),
            "<ruby>漢字<rt></rt></ruby>"
        );
    }

    #[test]
    fn non_cjk_text_passthrough() {
        assert_eq!(bracketed_to_furigana_html("abc123!@#"), "abc123!@#");
    }

    #[test]
    fn kanji_without_reading_rendered_plain() {
        assert_eq!(bracketed_to_furigana_html("東京"), "東京");
    }

    #[test]
    fn mixed_kanji_and_kana_no_brackets() {
        assert_eq!(bracketed_to_furigana_html("食べる"), "食べる");
    }

    #[test]
    fn reading_with_punctuation() {
        assert_eq!(
            bracketed_to_furigana_html("漢字[かん・じ]"),
            "<ruby>漢字<rt>かん・じ</rt></ruby>"
        );
    }

    #[test]
    fn non_kanji_bracket_is_literal() {
        assert_eq!(
            bracketed_to_furigana_html("CPU[シーピーユー]"),
            "CPU[シーピーユー]"
        );
    }

    #[test]
    fn multiple_brackets_only_first_matched_as_reading() {
        assert_eq!(
            bracketed_to_furigana_html("漢字[かんじ]あと[追加]"),
            "<ruby>漢字<rt>かんじ</rt></ruby>あと[追加]"
        );
    }

    #[test]
    fn whitespace_preserved() {
        assert_eq!(
            bracketed_to_furigana_html("漢字[かんじ] と 記号[きごう]"),
            "<ruby>漢字<rt>かんじ</rt></ruby> と <ruby>記号<rt>きごう</rt></ruby>"
        );
    }

    // ── is_kanji consistency check against upstream crate ────────────────────

    #[test]
    fn cjk_unified_ideographs_detected() {
        assert!(mecab_furigana_rs::has_kanji("好"));
        assert!(mecab_furigana_rs::has_kanji("食"));
        assert!(mecab_furigana_rs::has_kanji("日"));
    }
}
