//! Formatting helpers for the message box: wrap (or unwrap) the selected text with markdown markers.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Bold,
    Italic,
    Underline,
    Strike,
    Spoiler,
    Code,
    CodeBlock,
    Quote,
}

impl Format {
    pub const ALL: [Format; 8] = [Format::Bold, Format::Italic, Format::Underline, Format::Strike, Format::Spoiler, Format::Code, Format::CodeBlock, Format::Quote];

    pub fn label(self) -> &'static str {
        match self {
            Format::Bold => "Bold",
            Format::Italic => "Italic",
            Format::Underline => "Underline",
            Format::Strike => "Strikethrough",
            Format::Spoiler => "Spoiler",
            Format::Code => "Code",
            Format::CodeBlock => "Code block",
            Format::Quote => "Quote",
        }
    }

    /// The text put before and after the selection.
    fn markers(self) -> (&'static str, &'static str) {
        match self {
            Format::Bold => ("**", "**"),
            Format::Italic => ("*", "*"),
            Format::Underline => ("__", "__"),
            Format::Strike => ("~~", "~~"),
            Format::Spoiler => ("||", "||"),
            Format::Code => ("`", "`"),
            Format::CodeBlock => ("```\n", "\n```"),
            Format::Quote => ("> ", ""),
        }
    }
}

/// Applies `format` to the characters `start..end` of `text` (positions in characters, not bytes).
/// Returns the new text and the new selection. With nothing selected, the markers are inserted and the cursor
/// lands between them. If the selection is already wrapped in those markers, they are removed instead.
pub fn apply(text: &str, start: usize, end: usize, format: Format) -> (String, usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = (start.min(end).min(chars.len()), start.max(end).min(chars.len()));
    let (open, close) = format.markers();
    let (open, close): (Vec<char>, Vec<char>) = (open.chars().collect(), close.chars().collect());

    let wrapped = start >= open.len() && chars[start - open.len()..start] == open[..] && chars.len() - end >= close.len() && chars[end..end + close.len()] == close[..];
    if wrapped && (start < end || !open.is_empty()) {
        // unwrap: drop the markers around the selection
        let mut out: Vec<char> = chars[..start - open.len()].to_vec();
        out.extend_from_slice(&chars[start..end]);
        out.extend_from_slice(&chars[end + close.len()..]);
        return (out.into_iter().collect(), start - open.len(), end - open.len());
    }

    let mut out: Vec<char> = chars[..start].to_vec();
    out.extend_from_slice(&open);
    out.extend_from_slice(&chars[start..end]);
    out.extend_from_slice(&close);
    out.extend_from_slice(&chars[end..]);
    (out.into_iter().collect(), start + open.len(), end + open.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_the_selection() {
        assert_eq!(apply("hello world", 6, 11, Format::Bold), ("hello **world**".to_string(), 8, 13));
        assert_eq!(apply("hello", 0, 5, Format::Italic), ("*hello*".to_string(), 1, 6));
    }

    #[test]
    fn with_nothing_selected_the_cursor_lands_between_the_markers() {
        assert_eq!(apply("ab", 1, 1, Format::Strike), ("a~~~~b".to_string(), 3, 3));
        assert_eq!(apply("", 0, 0, Format::Code), ("``".to_string(), 1, 1));
    }

    #[test]
    fn a_second_click_removes_the_formatting() {
        let (text, a, b) = apply("hello", 0, 5, Format::Bold);
        assert_eq!(apply(&text, a, b, Format::Bold), ("hello".to_string(), 0, 5));
        let (text, a, b) = apply("x", 0, 1, Format::CodeBlock);
        assert_eq!(text, "```\nx\n```");
        assert_eq!(apply(&text, a, b, Format::CodeBlock), ("x".to_string(), 0, 1));
    }

    #[test]
    fn quote_adds_a_prefix_only() {
        assert_eq!(apply("hi", 0, 2, Format::Quote), ("> hi".to_string(), 2, 4));
    }

    #[test]
    fn positions_are_in_characters_so_accents_and_emojis_are_fine() {
        assert_eq!(apply("ça 🙂 va", 3, 4, Format::Spoiler), ("ça ||🙂|| va".to_string(), 5, 6));
        assert_eq!(apply("é", 0, 1, Format::Underline), ("__é__".to_string(), 2, 3));
    }

    #[test]
    fn out_of_range_positions_are_clamped() {
        assert_eq!(apply("ab", 5, 9, Format::Bold), ("ab****".to_string(), 4, 4));
        assert_eq!(apply("ab", 2, 0, Format::Bold), ("**ab**".to_string(), 2, 4));
    }
}
