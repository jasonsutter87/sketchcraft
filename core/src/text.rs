//! Characters a name must not carry when it is shown or sent on as data (shared by the AI prompt and the engine's notices).

/// Unicode format characters (category Cf) the prompt must not carry: zero-width and direction marks, the tag block
/// (U+E0000..E007F, which can hide text), joiners, invisible operators, BOM, soft hyphen and the other format marks.
pub fn is_format_char(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{0600}'..='\u{0605}' | '\u{061C}' | '\u{06DD}' | '\u{070F}' | '\u{0890}'..='\u{0891}' | '\u{08E2}'
        | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}'
        | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}' | '\u{110BD}' | '\u{110CD}' | '\u{13430}'..='\u{1343F}' | '\u{1BCA0}'..='\u{1BCA3}'
        | '\u{1D173}'..='\u{1D17A}' | '\u{E0000}'..='\u{E007F}')
}

/// Characters that draw nothing (or only a blank) but are not format characters or spaces: Hangul and Khmer fillers,
/// the Braille blank, the combining grapheme joiner, Mongolian free variation selectors and the variation selectors.
pub fn is_invisible_filler(c: char) -> bool {
    matches!(c,
        '\u{034F}' | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}' | '\u{180B}'..='\u{180F}' | '\u{2800}' | '\u{3164}'
        | '\u{FE00}'..='\u{FE0F}' | '\u{FFA0}' | '\u{E0100}'..='\u{E01EF}')
}

/// Spaces other than the plain one (no-break, en and em, thin, ideographic ...): the plain one stands in for them.
pub fn is_odd_space(c: char) -> bool {
    matches!(c, '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}')
}
