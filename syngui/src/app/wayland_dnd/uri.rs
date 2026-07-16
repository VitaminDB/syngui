use std::path::PathBuf;

use percent_encoding::percent_decode_str;

pub fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for raw in text.split(|c| c == '\r' || c == '\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(rest) = line.strip_prefix("file://") else {
            continue;
        };
        let path_part = match rest.find('/') {
            Some(idx) => &rest[idx..],
            None => continue,
        };
        let decoded = percent_decode_str(path_part).decode_utf8_lossy();
        out.push(PathBuf::from(decoded.into_owned()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_file_uri() {
        let paths = parse_uri_list("file:///home/user/photo.png\r\n");
        assert_eq!(paths, vec![PathBuf::from("/home/user/photo.png")]);
    }

    #[test]
    fn percent_decodes_spaces_and_unicode() {
        let paths = parse_uri_list("file:///tmp/Hello%20World.txt\r\n");
        assert_eq!(paths, vec![PathBuf::from("/tmp/Hello World.txt")]);
        let paths = parse_uri_list("file:///tmp/%D1%84%D0%B0%D0%B9%D0%BB.txt");
        assert_eq!(paths, vec![PathBuf::from("/tmp/файл.txt")]);
    }

    #[test]
    fn skips_comments_and_blanks() {
        let txt = "# comment line\r\n\r\nfile:///a.txt\r\n# another\r\nfile:///b.txt\r\n";
        let paths = parse_uri_list(txt);
        assert_eq!(paths, vec![PathBuf::from("/a.txt"), PathBuf::from("/b.txt")]);
    }

    #[test]
    fn ignores_non_file_schemes() {
        let txt = "http://example.com/x.png\r\nfile:///c.txt\r\n";
        let paths = parse_uri_list(txt);
        assert_eq!(paths, vec![PathBuf::from("/c.txt")]);
    }

    #[test]
    fn handles_host_form() {
        let paths = parse_uri_list("file://localhost/etc/hosts\r\n");
        assert_eq!(paths, vec![PathBuf::from("/etc/hosts")]);
    }
}
