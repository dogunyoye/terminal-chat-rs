/// Strip characters that could corrupt the terminal or spoof text.
///
/// Chat content, nicknames and timestamps all originate from remote peers, so
/// they are untrusted. C0/C1 control characters (including `ESC`) can drive the
/// terminal emulator — for example an OSC sequence can rewrite the window title
/// or the clipboard — and Unicode bidirectional controls can visually reorder
/// text. Since every message is rendered as a single line, none of these
/// characters are useful, so they are removed.
pub fn sanitize(input: &str) -> String {
    input.chars().filter(|c| !is_unsafe(*c)).collect()
}

fn is_unsafe(c: char) -> bool {
    c.is_control() || is_bidi_control(c)
}

fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_escape_and_control_characters() {
        assert_eq!(sanitize("hi\u{1b}]0;pwned\u{7}"), "hi]0;pwned");
        assert_eq!(sanitize("a\u{0}b\nc\rd\te"), "abcde");
        assert_eq!(sanitize("bell\u{7}del\u{7f}"), "belldel");
    }

    #[test]
    fn removes_bidi_overrides() {
        assert_eq!(sanitize("safe\u{202e}txet"), "safetxet");
    }

    #[test]
    fn keeps_normal_text_and_unicode() {
        assert_eq!(sanitize("hello, 世界! 🙂"), "hello, 世界! 🙂");
        assert_eq!(sanitize(""), "");
    }
}
