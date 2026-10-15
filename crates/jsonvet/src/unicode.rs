//! Code-point predicates shared by the parser, the positions and the messages.

/// JSON5's whitespace outside ASCII: NBSP, the line and paragraph separators, the BOM, and
/// every other space separator (Unicode category Zs).
pub(crate) fn is_json5_space(c: char) -> bool {
    matches!(
        c,
        '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Whether U+2028 or U+2029 starts at `at`. In UTF-8 they are `E2 80 A8` and `E2 80 A9`.
pub(crate) fn line_separator_at(bytes: &[u8], at: usize) -> bool {
    matches!(bytes.get(at..at + 3), Some([0xE2, 0x80, 0xA8 | 0xA9]))
}

/// Unicode's `Bidi_Control` characters: the marks and the embedding, override and isolate
/// controls.
pub(crate) fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{61C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

/// Unicode noncharacters: U+FDD0 to U+FDEF, and the last two code points of every plane.
pub(crate) fn is_noncharacter(code_point: u32) -> bool {
    (0xFDD0..=0xFDEF).contains(&code_point) || code_point & 0xFFFE == 0xFFFE
}

/// The first half of a UTF-16 surrogate pair.
pub(crate) fn is_high_surrogate(unit: u32) -> bool {
    (0xD800..0xDC00).contains(&unit)
}

/// The second half of a UTF-16 surrogate pair.
pub(crate) fn is_low_surrogate(unit: u32) -> bool {
    (0xDC00..0xE000).contains(&unit)
}

/// Either half of a UTF-16 surrogate pair, which is no character on its own.
pub(crate) fn is_surrogate(code_point: u32) -> bool {
    (0xD800..0xE000).contains(&code_point)
}

/// ECMAScript 5.1's `IdentifierStart` (section 7.6), which JSON5 identifiers follow: `$`, `_` or a
/// letter by general category. Not Unicode's `ID_Start`, which adds and drops a few characters.
pub(crate) fn is_identifier_start(c: char) -> bool {
    use unicode_properties::GeneralCategory::{
        LetterNumber, LowercaseLetter, ModifierLetter, OtherLetter, TitlecaseLetter,
        UppercaseLetter,
    };
    use unicode_properties::UnicodeGeneralCategory;
    if c.is_ascii() {
        return c.is_ascii_alphabetic() || c == '$' || c == '_';
    }
    matches!(
        c.general_category(),
        UppercaseLetter
            | LowercaseLetter
            | TitlecaseLetter
            | ModifierLetter
            | OtherLetter
            | LetterNumber
    )
}

/// ECMAScript 5.1's `IdentifierPart`: a start character, a combining mark, a decimal digit, a
/// connector punctuation, ZWNJ or ZWJ.
pub(crate) fn is_identifier_part(c: char) -> bool {
    use unicode_properties::GeneralCategory::{
        ConnectorPunctuation, DecimalNumber, NonspacingMark, SpacingMark,
    };
    use unicode_properties::UnicodeGeneralCategory;
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '$' || c == '_';
    }
    is_identifier_start(c)
        || matches!(c, '\u{200C}' | '\u{200D}')
        || matches!(
            c.general_category(),
            NonspacingMark | SpacingMark | DecimalNumber | ConnectorPunctuation
        )
}
