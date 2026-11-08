use nameh::{
    document::{ExternalScheme, LinkTarget, LocalTarget, classify_link},
    navigation::{self, Destination},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("mdview-navigation-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn relative_unicode_fragments_and_once_only_decoding() {
    let document = Path::new("/tmp/docs/current.md");
    for (encoded, path, fragment) in [
        (
            "notes/Unicode%20caf%C3%A9.md#caf%C3%A9",
            "/tmp/docs/notes/Unicode café.md",
            Some("café"),
        ),
        (
            "../Upper.MD#section-1",
            "/tmp/docs/../Upper.MD",
            Some("section-1"),
        ),
        (
            "literal%2520name.md#percent%2520",
            "/tmp/docs/literal%20name.md",
            Some("percent%20"),
        ),
        ("/tmp/absolute.markdown", "/tmp/absolute.markdown", None),
        ("name%23part.mdown", "/tmp/docs/name#part.mdown", None),
        ("name%3Fpart.mkd", "/tmp/docs/name?part.mkd", None),
    ] {
        let target = classify_link(encoded).unwrap();
        assert_eq!(
            navigation::resolve(document, &target).unwrap(),
            Destination::Markdown {
                path: path.into(),
                fragment: fragment.map(str::to_owned)
            }
        );
    }
    for (encoded, decoded) in [("#caf%C3%A9", "café"), ("#", ""), ("", "")] {
        assert_eq!(
            navigation::resolve(document, &classify_link(encoded).unwrap()).unwrap(),
            Destination::Heading(decoded.into())
        );
    }
    let uri_like = LinkTarget::Local(LocalTarget {
        path: "https%3A%2F%2Fexample.org%2Ffile.md".into(),
        fragment: None,
    });
    assert_eq!(
        navigation::resolve(document, &uri_like).unwrap(),
        Destination::Markdown {
            path: PathBuf::from("/tmp/docs/https:/example.org/file.md"),
            fragment: None
        }
    );
    let slash = LinkTarget::Local(LocalTarget {
        path: "%2F%2Ftmp%2Fimage.png".into(),
        fragment: None,
    });
    assert!(matches!(
        navigation::resolve(document, &slash).unwrap(),
        Destination::OtherFile(_)
    ));
}

#[test]
fn external_targets_and_invalid_encodings() {
    let document = Path::new("/tmp/doc.md");
    for uri in [
        "https://example.org/a?b=c#d",
        "http://example.org",
        "mailto:a@example.org",
    ] {
        assert_eq!(
            navigation::resolve(document, &classify_link(uri).unwrap()).unwrap(),
            Destination::ExternalUri(uri.into())
        );
    }
    for uri in [
        "javascript:alert(1)",
        "data:text/plain,x",
        "file:///tmp/file.md",
        "//example.org/a",
        "ftp://example.org",
        "x%00.md",
        "x%5c.md",
    ] {
        assert!(classify_link(uri).is_none());
    }
    for invalid in ["%00", "%5C", "%FF", "%", "%GG"] {
        assert!(navigation::decode_component(invalid).is_none());
        assert!(
            navigation::resolve(
                document,
                &LinkTarget::Local(LocalTarget {
                    path: invalid.into(),
                    fragment: None
                })
            )
            .is_err()
        );
        assert!(navigation::resolve(document, &LinkTarget::Heading(invalid.into())).is_err());
    }
    assert!(
        navigation::resolve(
            document,
            &LinkTarget::External {
                scheme: ExternalScheme::Https,
                uri: "javascript:alert(1)".into()
            }
        )
        .is_err()
    );
}

#[test]
fn readonly_loading_errors_and_safe_other_files() {
    let temp = Temp::new();
    let path = temp.0.join("Unicode café #1.md");
    let bytes = "# Café\n\n[heading](#caf%C3%A9)\n".as_bytes();
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    let loaded = navigation::load_document(&path).unwrap();
    assert_eq!(loaded.document.source.as_bytes(), bytes);
    assert_eq!(loaded.path, path);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let encoded_name = temp.0.join("literal%20 café.md");
    fs::write(&encoded_name, bytes).unwrap();
    let target = classify_link("literal%2520%20caf%C3%A9.md#caf%C3%A9").unwrap();
    let Destination::Markdown {
        path: resolved,
        fragment,
    } = navigation::resolve(&path, &target).unwrap()
    else {
        panic!("expected Markdown path");
    };
    assert_eq!(fragment.as_deref(), Some("café"));
    assert_eq!(
        navigation::load_document(&resolved).unwrap().path,
        encoded_name
    );

    assert!(
        navigation::load_document(&temp.0)
            .unwrap_err()
            .contains("regular file")
    );
    assert!(navigation::load_document(&temp.0.join("missing.md")).is_err());
    let invalid = temp.0.join("invalid.md");
    fs::write(&invalid, [0xff, 0xfe]).unwrap();
    assert!(
        navigation::load_document(&invalid)
            .unwrap_err()
            .contains("UTF-8")
    );
    let unreadable = temp.0.join("unreadable.md");
    fs::write(&unreadable, bytes).unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o0)).unwrap();
    assert!(
        navigation::load_document(&unreadable).is_err(),
        "run this test as an unprivileged user"
    );
    for (name, bytes, mode, allowed) in [
        ("text.txt", b"ordinary text".as_slice(), 0o644, true),
        ("report.pdf", b"%PDF-1.4".as_slice(), 0o644, true),
        ("run.txt", b"hello".as_slice(), 0o755, false),
        (
            "script.txt",
            b"#!/bin/sh\necho bad".as_slice(),
            0o644,
            false,
        ),
        ("binary.bin", b"\x7fELFmore".as_slice(), 0o644, false),
        (
            "fake.desktop",
            b"[Desktop Entry]\nExec=bad".as_slice(),
            0o644,
            false,
        ),
        (
            "disguised.txt",
            b"# comment\n[Desktop Entry]\nExec=bad".as_slice(),
            0o644,
            false,
        ),
        ("script.sh", b"echo bad".as_slice(), 0o644, false),
    ] {
        let path = temp.0.join(name);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(
            navigation::check_other_file(&path).is_ok(),
            allowed,
            "{name}"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    symlink(temp.0.join("script.sh"), temp.0.join("symlink.txt")).unwrap();
    // A non-executable, shebang-free script disguised through a symlink must
    // still be checked using the resolved target's extension.
    assert!(navigation::check_other_file(&temp.0.join("symlink.txt")).is_err());
}
