use std::path::{Component, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedRef {
    Path(String),
    Url(String),
}

pub(crate) fn resolve_ref(base: Option<&str>, raw: &str) -> ResolvedRef {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('#') {
        return ResolvedRef::Url(raw.to_string());
    }
    if url_scheme(raw).is_some() {
        return ResolvedRef::Url(raw.to_string());
    }
    if is_absolute_fs(raw) {
        return ResolvedRef::Path(raw.to_string());
    }
    match base {
        None => ResolvedRef::Url(raw.to_string()),
        Some(base) => match url_scheme(base).as_deref() {
            Some("http") | Some("https") => ResolvedRef::Url(join_url(base, raw)),
            Some("file") => ResolvedRef::Path(join_path(
                base.strip_prefix("file://").unwrap_or(base),
                raw,
            )),
            _ => ResolvedRef::Path(join_path(base, raw)),
        },
    }
}

pub(crate) fn resolve_link(base: Option<&str>, raw: &str) -> String {
    match resolve_ref(base, raw) {
        ResolvedRef::Url(url) => url,
        ResolvedRef::Path(path) => file_url_from_path(&path),
    }
}

fn url_scheme(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b':' {
            break;
        }
        if c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.' {
            i += 1;
        } else {
            return None;
        }
    }
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    let scheme = &s[..i];
    if scheme.len() == 1 {
        match bytes.get(i + 1) {
            Some(b'\\') | Some(b'/') | None => return None,
            _ => {}
        }
    }
    Some(scheme.to_ascii_lowercase())
}

fn is_absolute_fs(s: &str) -> bool {
    let b = s.as_bytes();
    if b.first() == Some(&b'/') {
        return true;
    }
    if s.starts_with("\\\\") {
        return true;
    }
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return matches!(b.get(2), Some(b'\\') | Some(b'/'));
    }
    false
}

fn join_path(base: &str, rel: &str) -> String {
    let mut pb = PathBuf::from(base);
    pb.push(rel);
    let mut out: Vec<Component> = Vec::new();
    for comp in pb.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.last(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(comp);
                }
            }
            other => out.push(other),
        }
    }
    let mut res = PathBuf::new();
    for c in out {
        res.push(c.as_os_str());
    }
    res.to_string_lossy().into_owned()
}

fn join_url(base: &str, rel: &str) -> String {
    let base = base.trim_end_matches('/');
    let mut segs: Vec<&str> = base.split('/').collect();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if segs.len() > 3 {
                    segs.pop();
                }
            }
            other => segs.push(other),
        }
    }
    segs.join("/")
}

fn file_url_from_path(path: &str) -> String {
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_url_passthrough() {
        assert_eq!(
            resolve_ref(Some("/docs"), "https://example.com/a.png"),
            ResolvedRef::Url("https://example.com/a.png".into())
        );
        assert_eq!(
            resolve_ref(Some("/docs"), "data:image/png;base64,AAAA"),
            ResolvedRef::Url("data:image/png;base64,AAAA".into())
        );
    }

    #[test]
    fn anchor_and_empty_passthrough() {
        assert_eq!(resolve_ref(Some("/docs"), "#section"), ResolvedRef::Url("#section".into()));
        assert_eq!(resolve_ref(None, ""), ResolvedRef::Url("".into()));
    }

    #[test]
    fn relative_against_local_dir() {
        assert_eq!(
            resolve_ref(Some("/home/u/docs"), "img/a.png"),
            ResolvedRef::Path("/home/u/docs/img/a.png".into())
        );
        assert_eq!(
            resolve_ref(Some("/home/u/docs"), "./img/a.png"),
            ResolvedRef::Path("/home/u/docs/img/a.png".into())
        );
        assert_eq!(
            resolve_ref(Some("/home/u/docs/sub"), "../img/a.png"),
            ResolvedRef::Path("/home/u/docs/img/a.png".into())
        );
    }

    #[test]
    fn relative_against_http_dir() {
        assert_eq!(
            resolve_ref(Some("https://host.com/blog/"), "img/a.png"),
            ResolvedRef::Url("https://host.com/blog/img/a.png".into())
        );
        assert_eq!(
            resolve_ref(Some("https://host.com/blog/post"), "../a.png"),
            ResolvedRef::Url("https://host.com/blog/a.png".into())
        );
    }

    #[test]
    fn file_base_resolves_to_path() {
        assert_eq!(
            resolve_ref(Some("file:///home/u/docs"), "a.png"),
            ResolvedRef::Path("/home/u/docs/a.png".into())
        );
    }

    #[test]
    fn absolute_local_path_routes_to_path() {
        assert_eq!(
            resolve_ref(None, "/var/img/a.png"),
            ResolvedRef::Path("/var/img/a.png".into())
        );
    }

    #[test]
    fn no_base_relative_stays_url() {
        assert_eq!(resolve_ref(None, "img/a.png"), ResolvedRef::Url("img/a.png".into()));
    }

    #[test]
    fn windows_drive_is_not_scheme() {
        assert!(url_scheme("C:\\a\\b.png").is_none());
        assert!(is_absolute_fs("C:\\a\\b.png"));
    }

    #[test]
    fn link_local_relative_becomes_file_url() {
        assert_eq!(resolve_link(Some("/home/u/docs"), "page.md"), "file:///home/u/docs/page.md");
        assert_eq!(resolve_link(Some("/home/u/docs"), "#top"), "#top");
        assert_eq!(
            resolve_link(Some("https://host.com/d/"), "x.html"),
            "https://host.com/d/x.html"
        );
    }
}
