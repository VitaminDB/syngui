const PASTE_END_MARKER: &str = "\x1b[201~";
const PASTE_BEGIN: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

pub fn sanitize_paste(s: &str) -> String {
    let stripped_marker = s.replace(PASTE_END_MARKER, "");

    let mut out = String::with_capacity(stripped_marker.len());
    let mut prev_cr = false;
    for ch in stripped_marker.chars() {
        match ch {
            '\x1b' | '\x00' => {
                prev_cr = false;
            }
            '\r' => {
                out.push('\r');
                prev_cr = true;
            }
            '\n' => {
                if !prev_cr {
                    out.push('\r');
                }
                prev_cr = false;
            }
            other => {
                out.push(other);
                prev_cr = false;
            }
        }
    }
    out
}

pub fn wrap_bracketed(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + PASTE_BEGIN.len() + PASTE_END.len());
    out.extend_from_slice(PASTE_BEGIN);
    out.extend_from_slice(bytes);
    out.extend_from_slice(PASTE_END);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_esc() {
        assert_eq!(sanitize_paste("hello\x1bworld"), "helloworld");
    }

    #[test]
    fn strips_nul() {
        assert_eq!(sanitize_paste("hello\x00world"), "helloworld");
    }

    #[test]
    fn neutralizes_paste_terminator() {
        let payload = "abc\x1b[201~def";
        let cleaned = sanitize_paste(payload);
        assert!(!cleaned.contains("[201~"));
        assert!(!cleaned.contains('\x1b'));
        assert_eq!(cleaned, "abcdef");
    }

    #[test]
    fn normalizes_lf_to_cr() {
        assert_eq!(sanitize_paste("line1\nline2"), "line1\rline2");
    }

    #[test]
    fn normalizes_crlf_to_cr() {
        assert_eq!(sanitize_paste("line1\r\nline2"), "line1\rline2");
    }

    #[test]
    fn preserves_bare_cr() {
        assert_eq!(sanitize_paste("line1\rline2"), "line1\rline2");
    }

    #[test]
    fn preserves_unicode_and_tabs() {
        assert_eq!(sanitize_paste("каф\tcafé"), "каф\tcafé");
    }

    #[test]
    fn wrap_brackets_message() {
        let bytes = wrap_bracketed("hi");
        assert_eq!(bytes, b"\x1b[200~hi\x1b[201~");
    }

    #[test]
    fn wrap_handles_empty_text() {
        let bytes = wrap_bracketed("");
        assert_eq!(bytes, b"\x1b[200~\x1b[201~");
    }
}
