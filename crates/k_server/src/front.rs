//! The checks the HTTP front makes before any JavaScript runs.
//!
//! They cover body limits, RPC preconditions and request forgery
//! (`docs/compiler/server.md`, "Request lifecycle"), and are pure functions of the
//! request head, so they are unit-tested exhaustively here.

use std::collections::BTreeSet;

/// The session cookie on an `https` public origin.
pub const SECURE_SESSION_COOKIE: &str = "__Host-k_session";
/// The session cookie on an `http` public origin (local development).
pub const SESSION_COOKIE: &str = "k_session";
/// The default request body limit: 1 MB, for rpcs and endpoint pages
/// (`docs/design/28-rpc-wire-format.md`, `27-endpoint-pages.md`).
pub const DEFAULT_MAX_BODY_BYTES: usize = 1024 * 1024;

/// How long a connection may take to send a request's headers, from the
/// moment it is accepted or its previous response was written, before
/// the server closes it (slowloris; `docs/compiler/server.md` §5).
pub const HEADER_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// How long the server waits for a whole request body (at most
/// [`DEFAULT_MAX_BODY_BYTES`] by default) before it gives up with `408`
/// and closes the connection.
///
/// A client that declares a length and stalls
/// holds no connection forever.
pub const BODY_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// What the front knows about the app.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FrontPolicy {
    /// Every rpc path of the app, `/shop.cart.CartProtocol/Add`.
    pub rpc_paths: BTreeSet<String>,
    /// `K_PUBLIC_ORIGIN`, `https://shop.example` (no trailing slash).
    pub public_origin: String,
    /// The request body limit in bytes.
    pub max_body_bytes: usize,
}

/// A response the front sends itself.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FrontResponse {
    /// The status code.
    pub status: u16,
    /// Extra headers (besides content type and caching).
    pub headers: Vec<(String, String)>,
    /// The content type of `body`.
    pub content_type: &'static str,
    /// The body text, of type `content_type`.
    pub body: String,
}

impl FrontResponse {
    /// A Connect-style error response (`{"code": …, "message": …}` JSON), the
    /// shape rpc clients decode.
    fn connect(status: u16, code: &str, message: &str) -> FrontResponse {
        let body = serde_json::json!({ "code": code, "message": message }).to_string();
        FrontResponse {
            status,
            headers: Vec::new(),
            content_type: "application/json",
            body,
        }
    }

    /// A `text/plain` response with `message` as its body, for pages and
    /// endpoints.
    fn plain(status: u16, message: &str) -> FrontResponse {
        FrontResponse {
            status,
            headers: Vec::new(),
            content_type: "text/plain; charset=utf-8",
            body: message.to_owned(),
        }
    }
}

/// Returns the value of header `name` (lower case), joining repeats with
/// `, ` as endpoint pages see them.
#[must_use]
pub fn header<'a>(
    headers: &'a [(String, String)],
    name: &str,
) -> Option<std::borrow::Cow<'a, str>> {
    let mut values = headers
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, v)| v.as_str());
    let first = values.next()?;
    let rest: Vec<&str> = values.collect();
    if rest.is_empty() {
        Some(std::borrow::Cow::Borrowed(first))
    } else {
        let mut all = vec![first];
        all.extend(rest);
        Some(std::borrow::Cow::Owned(all.join(", ")))
    }
}

/// Returns `true` if the request carries the session cookie.
#[must_use]
pub fn has_session_cookie(headers: &[(String, String)]) -> bool {
    headers.iter().filter(|(n, _)| n == "cookie").any(|(_, v)| {
        v.split(';').any(|pair| {
            let name = pair.split('=').next().unwrap_or("").trim();
            name == SESSION_COOKIE || name == SECURE_SESSION_COOKIE
        })
    })
}

/// Returns `true` if `path` (without the query) has the shape of an rpc
/// path, `/{package}.{Service}/{Method}`, whether or not the app has it.
#[must_use]
pub fn looks_like_rpc(path: &str) -> bool {
    let Some(rest) = path.strip_prefix('/') else {
        return false;
    };
    let mut parts = rest.split('/');
    let (Some(service), Some(method), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    service.contains('.')
        && service.split('.').all(ident)
        && method
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        && ident(method)
}

/// Checks a request head. Returns `None` to hand the request to
/// JavaScript, or the response the front sends instead.
///
/// In order:
///
/// 1. a declared `Content-Length` over the limit is `413`
///    (`resource_exhausted` for an rpc); the streamed body is checked
///    again while it is read;
/// 2. an rpc-shaped path the app doesn't have is `404` (`unimplemented`);
/// 3. an rpc must be `POST` (else `405` with `Allow: POST`), with
///    `Content-Type: application/json` and `Connect-Protocol-Version: 1`
///    (else `415`; binary Protobuf is deferred, so `application/proto` is
///    `415` in v0 as well);
/// 4. a request that carries the session cookie and isn't `GET` or `HEAD`
///    must have an `Origin` equal to the public origin or
///    `Sec-Fetch-Site: same-origin` (else `403`), for rpcs and endpoint
///    pages alike (`docs/design/26-auth-and-sessions.md`).
#[must_use]
pub fn check(
    policy: &FrontPolicy,
    method: &str,
    path_and_query: &str,
    headers: &[(String, String)],
) -> Option<FrontResponse> {
    let path = path_and_query.split('?').next().unwrap_or("");
    let rpc = policy.rpc_paths.contains(path);
    let declared = header(headers, "content-length").and_then(|v| v.trim().parse::<u64>().ok());
    if declared.is_some_and(|n| n > policy.max_body_bytes as u64) {
        return Some(if rpc {
            FrontResponse::connect(
                413,
                "resource_exhausted",
                "the request body is larger than 1 MB",
            )
        } else {
            FrontResponse::plain(413, "request body too large")
        });
    }
    if !rpc && looks_like_rpc(path) {
        return Some(FrontResponse::connect(404, "unimplemented", "no such rpc"));
    }
    if rpc {
        if method != "POST" {
            let mut r = FrontResponse::connect(405, "unimplemented", "rpcs are called with POST");
            r.headers.push(("allow".to_owned(), "POST".to_owned()));
            return Some(r);
        }
        let content_type = header(headers, "content-type").unwrap_or_default();
        let media = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        // Unary rpcs are `application/json`; server-streaming ones send one
        // enveloped message as `application/connect+json` (the handler
        // checks which kind the rpc is).
        if media != "application/json" && media != "application/connect+json" {
            return Some(FrontResponse::plain(415, "rpcs accept application/json"));
        }
        if header(headers, "connect-protocol-version").as_deref() != Some("1") {
            return Some(FrontResponse::plain(
                415,
                "rpcs require Connect-Protocol-Version: 1",
            ));
        }
    }
    if method != "GET" && method != "HEAD" && has_session_cookie(headers) {
        let origin_ok = header(headers, "origin").is_some_and(|o| o == policy.public_origin);
        let same_site = header(headers, "sec-fetch-site").is_some_and(|s| s == "same-origin");
        if !origin_ok && !same_site {
            return Some(if rpc {
                FrontResponse::connect(403, "permission_denied", "cross-origin request refused")
            } else {
                FrontResponse::plain(403, "cross-origin request refused")
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> FrontPolicy {
        FrontPolicy {
            rpc_paths: ["/shop.cart.CartProtocol/Add".to_owned()]
                .into_iter()
                .collect(),
            public_origin: "https://shop.example".to_owned(),
            max_body_bytes: 100,
        }
    }

    fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    const RPC: &str = "/shop.cart.CartProtocol/Add";
    const JSON: (&str, &str) = ("content-type", "application/json");
    const CONNECT: (&str, &str) = ("connect-protocol-version", "1");

    #[test]
    fn a_well_formed_rpc_passes() {
        assert_eq!(check(&policy(), "POST", RPC, &h(&[JSON, CONNECT])), None);
        let charset = ("content-type", "Application/JSON; charset=utf-8");
        assert_eq!(check(&policy(), "POST", RPC, &h(&[charset, CONNECT])), None);
    }

    #[test]
    fn a_streaming_rpc_request_passes_with_the_connect_streaming_type() {
        let stream = ("content-type", "application/connect+json");
        assert_eq!(check(&policy(), "POST", RPC, &h(&[stream, CONNECT])), None);
        let proto = ("content-type", "application/connect+proto");
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[proto, CONNECT])).map(|r| r.status),
            Some(415)
        );
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[stream])).map(|r| r.status),
            Some(415)
        );
    }

    #[test]
    fn rpc_preconditions() {
        let r = check(&policy(), "GET", RPC, &h(&[JSON, CONNECT])).expect("refused");
        assert_eq!(
            (r.status, r.headers.clone()),
            (405, vec![("allow".into(), "POST".into())])
        );
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[CONNECT])).map(|r| r.status),
            Some(415)
        );
        let form = ("content-type", "application/x-www-form-urlencoded");
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[form, CONNECT])).map(|r| r.status),
            Some(415)
        );
        let proto = ("content-type", "application/proto");
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[proto, CONNECT])).map(|r| r.status),
            Some(415)
        );
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON])).map(|r| r.status),
            Some(415)
        );
        let v2 = ("connect-protocol-version", "2");
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, v2])).map(|r| r.status),
            Some(415)
        );
    }

    #[test]
    fn unknown_rpcs_are_unimplemented() {
        let r = check(
            &policy(),
            "POST",
            "/shop.cart.CartProtocol/Remove",
            &h(&[JSON, CONNECT]),
        )
        .expect("404");
        assert_eq!(r.status, 404);
        assert!(r.body.contains("unimplemented"));
        assert_eq!(
            check(&policy(), "GET", "/product/42", &[]),
            None,
            "pages are not rpcs"
        );
    }

    #[test]
    fn rpc_shape() {
        assert!(looks_like_rpc("/a.B/C"));
        assert!(looks_like_rpc("/shop.cart.CartProtocol/Add"));
        assert!(!looks_like_rpc("/product/42"));
        assert!(!looks_like_rpc("/a.B/c"));
        assert!(!looks_like_rpc("/a.B/C/D"));
        assert!(!looks_like_rpc("/.B/C"));
        assert!(!looks_like_rpc("/app.3f2a.js"));
        assert!(!looks_like_rpc(""));
    }

    #[test]
    fn bodies_over_the_limit() {
        let big = ("content-length", "101");
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, CONNECT, big])).map(|r| r.status),
            Some(413)
        );
        assert_eq!(
            check(&policy(), "POST", "/webhooks/stripe", &h(&[big])).map(|r| r.status),
            Some(413)
        );
        let exact = ("content-length", "100");
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, CONNECT, exact])),
            None
        );
    }

    #[test]
    fn forgery_checks_apply_only_with_the_session_cookie() {
        let cookie = ("cookie", "k_theme=dark; __Host-k_session=abc");
        let evil = ("origin", "https://evil.example");
        let good = ("origin", "https://shop.example");
        let same = ("sec-fetch-site", "same-origin");
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, CONNECT, cookie, evil])).map(|r| r.status),
            Some(403)
        );
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, CONNECT, cookie])).map(|r| r.status),
            Some(403)
        );
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, CONNECT, cookie, good])),
            None
        );
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, CONNECT, cookie, same])),
            None
        );
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, CONNECT, evil])),
            None,
            "no cookie, no check"
        );
        assert_eq!(
            check(&policy(), "GET", "/account", &h(&[cookie, evil])),
            None,
            "GET is exempt"
        );
        assert_eq!(
            check(&policy(), "POST", "/webhooks/x", &h(&[cookie, evil])).map(|r| r.status),
            Some(403)
        );
        let prefix = ("origin", "https://shop.example.evil.example");
        assert_eq!(
            check(&policy(), "POST", RPC, &h(&[JSON, CONNECT, cookie, prefix])).map(|r| r.status),
            Some(403)
        );
    }

    #[test]
    fn cookie_detection() {
        assert!(has_session_cookie(&h(&[("cookie", "k_session=1")])));
        assert!(has_session_cookie(&h(&[
            ("cookie", "a=1"),
            ("cookie", " k_session=2")
        ])));
        assert!(!has_session_cookie(&h(&[(
            "cookie",
            "k_sessionx=1; xk_session=2"
        )])));
        assert!(!has_session_cookie(&[]));
    }

    #[test]
    fn repeated_headers_join() {
        let hs = h(&[("x", "a"), ("y", "b"), ("x", "c")]);
        assert_eq!(header(&hs, "x").as_deref(), Some("a, c"));
        assert_eq!(header(&hs, "z"), None);
    }
}
