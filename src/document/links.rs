//! Conservative target classification. No decoding, resolution or dispatch here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalScheme {
    Http,
    Https,
    Mailto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// Still percent encoded, like LocalTarget::fragment.
    Heading(String),
    Local(LocalTarget),
    External {
        scheme: ExternalScheme,
        uri: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTarget {
    /// URI-encoded spelling, resolved against the document directory by loaders.
    /// Decode exactly once and use a filesystem API, never a URI launcher.
    pub path: String,
    pub fragment: Option<String>,
}

/// Allow local paths, heading fragments, HTTP(S) and mailto only. Reject
/// scheme-relative URLs, backslashes, controls, invalid escapes and unknown
/// schemes (including file:, data:, javascript: and desktop-specific handlers).
/// Percent escapes stay encoded: decoding before classification can change
/// which characters are path/fragment separators. Local results MUST remain
/// filesystem paths after decoding, even when they contain ':' or '//'.
pub fn classify_link(destination: &str) -> Option<LinkTarget> {
    if destination.chars().any(|ch| ch.is_control() || ch == '\\')
        || destination.trim() != destination
        || destination.starts_with("//")
        || !valid_escapes(destination)
    {
        return None;
    }
    if let Some(fragment) = destination.strip_prefix('#') {
        return Some(LinkTarget::Heading(fragment.to_owned()));
    }
    let prefix = destination
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if let Some((scheme, _)) = prefix.split_once(':') {
        // Any colon before a separator is reserved for URI syntax, even if the
        // spelling is malformed. './name:part.md' remains a usable local path.
        let scheme = match scheme.to_ascii_lowercase().as_str() {
            "http" => ExternalScheme::Http,
            "https" => ExternalScheme::Https,
            "mailto" => ExternalScheme::Mailto,
            _ => return None,
        };
        if destination.chars().any(char::is_whitespace) {
            return None;
        }
        let (_, rest) = destination.split_once(':')?;
        match scheme {
            ExternalScheme::Http | ExternalScheme::Https => {
                let authority = rest.strip_prefix("//")?.split(['/', '?', '#']).next()?;
                if authority.is_empty() {
                    return None;
                }
            }
            ExternalScheme::Mailto => {
                if rest.is_empty() || rest.starts_with("//") {
                    return None;
                }
            }
        }
        return Some(LinkTarget::External {
            scheme,
            uri: destination.to_owned(),
        });
    }
    let (path, fragment) = match destination.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment.to_owned())),
        None => (destination, None),
    };
    Some(LinkTarget::Local(LocalTarget {
        path: path.to_owned(),
        fragment,
    }))
}

fn valid_escapes(value: &str) -> bool {
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let Some(high) = bytes.next().and_then(|b| (b as char).to_digit(16)) else {
                return false;
            };
            let Some(low) = bytes.next().and_then(|b| (b as char).to_digit(16)) else {
                return false;
            };
            let decoded = (high * 16 + low) as u8;
            if decoded.is_ascii_control() || decoded == b'\\' {
                return false;
            }
        }
    }
    true
}
