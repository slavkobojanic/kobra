//! The pre-paint theme snippet of client-rendered documents
//! (`themeSnippet` in `k_runtime/js/runtime/app.js`,
//! `docs/design/07-themes-and-units.md`, "Switching variants at runtime") is
//! the same text in every app, so a Content-Security-Policy allows it by
//! its hash. The kanban example's nginx image sends such a policy: its hash
//! must be the snippet's, or the browser blocks the snippet and a reload
//! loses the chosen theme.

use sha2::{Digest, Sha256};

/// The snippet's text: the single-quoted pieces of the `themeSnippet`
/// concatenation, joined.
fn snippet() -> String {
    let source = k_runtime::module("k:/runtime/app.js")
        .expect("the server runtime")
        .source;
    let start = source
        .find("export const themeSnippet =")
        .expect("themeSnippet is exported");
    let rest = &source[start..];
    let end = rest.find(";\n").expect("the end of the declaration");
    let decl = &rest[..end];
    let mut out = String::new();
    let mut parts = decl.split('\'');
    parts.next();
    while let Some(piece) = parts.next() {
        out.push_str(piece);
        if parts.next().is_none() {
            break;
        }
    }
    out
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn the_kanban_images_policy_allows_the_theme_snippet_by_its_hash() {
    let text = snippet();
    assert!(
        text.starts_with("try{") && text.ends_with("catch(x){}"),
        "{text}"
    );
    let hash = base64(&Sha256::digest(text.as_bytes()));
    let conf = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/kanban/nginx.conf"),
    )
    .expect("examples/kanban/nginx.conf");
    let policies: Vec<&str> = conf
        .lines()
        .filter(|l| l.contains("Content-Security-Policy"))
        .collect();
    assert!(!policies.is_empty());
    for p in policies {
        assert!(
            p.contains(&format!("script-src 'self' 'sha256-{hash}'")),
            "the policy must allow the theme snippet ('sha256-{hash}'): {p}"
        );
    }
}
