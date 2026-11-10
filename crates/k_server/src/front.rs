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
/// The guest session cookie (`docs/design/26-auth-and-sessions.md`, "Guest
/// sessions"), renamed as [`SESSION_COOKIE`] is: `__Host-k_guest`,
/// `k_guest_<port>`.
pub const GUEST_COOKIE: &str = "k_guest";
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

/// The name browsers see for the session cookie of an app serving
/// `public_origin`, when it isn't the runtime's own `k_session`: on an
/// `http` origin with an explicit port, `k_session_<port>`.
///
/// Browsers scope cookies by host, not by port, so every app on
/// `localhost` would otherwise share one `k_session` cookie, and each would
/// clear the others' sessions as unknown (`docs/design/26-auth-and-sessions.md`,
/// "The session cookie"). An `https` origin keeps `__Host-k_session`
/// (one app per origin), and so does an `http` origin without a port.
/// The front renames the cookie both ways ([`to_runtime_cookies`],
/// [`to_public_set_cookie`]), so the runtime only ever sees `k_session`.
#[must_use]
pub fn public_session_cookie(public_origin: &str) -> Option<String> {
    let rest = public_origin.strip_prefix("http://")?;
    let host = rest.split('/').next().unwrap_or("");
    // `[::1]:8080` or `localhost:8080`: the port is after the last `:`
    // that follows any bracketed address.
    let after = host.rsplit_once(']').map_or(host, |(_, a)| a);
    let (_, port) = after.rsplit_once(':')?;
    (!port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{SESSION_COOKIE}_{port}"))
}

/// The theme cookie (`docs/design/07-themes-and-units.md`, "Switching
/// variants at runtime"), set by the browser and scoped as
/// [`SESSION_COOKIE`] is: `__Host-k_theme`, `k_theme_<port>`.
pub const THEME_COOKIE: &str = "k_theme";

/// The runtime's cookie names with their public names for the public
/// session cookie name `public` (`k_session_8081`, `k_guest_8081`,
/// `k_theme_8081`).
fn renamed(public: &str) -> [(&'static str, String); 3] {
    let suffix = public.strip_prefix(SESSION_COOKIE).unwrap_or("");
    [
        (SESSION_COOKIE, public.to_owned()),
        (GUEST_COOKIE, format!("{GUEST_COOKIE}{suffix}")),
        (THEME_COOKIE, format!("{THEME_COOKIE}{suffix}")),
    ]
}

/// A request's `Cookie` header value as the runtime sees it, for the
/// public cookie name `public`: `public=…` becomes `k_session=…` (and the
/// guest and theme cookies likewise), and a plain `k_session`, `k_guest`
/// or `k_theme` (another app's on the same host) is dropped.
#[must_use]
pub fn to_runtime_cookies(value: &str, public: &str) -> String {
    let names = renamed(public);
    value
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .filter_map(|pair| {
            let (name, rest) = pair.split_once('=').unwrap_or((pair, ""));
            let name = name.trim();
            if names.iter().any(|(runtime, _)| name == *runtime) {
                None
            } else if let Some((runtime, _)) = names.iter().find(|(_, p)| name == p) {
                Some(format!("{runtime}={rest}"))
            } else {
                Some(pair.to_owned())
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// A `Set-Cookie` value from the runtime as browsers see it: `k_session=`
/// becomes `public=`, `k_guest=` the guest cookie's public name; other
/// cookies are unchanged.
#[must_use]
pub fn to_public_set_cookie(value: &str, public: &str) -> String {
    for (runtime, p) in renamed(public) {
        if let Some(rest) = value.strip_prefix(runtime)
            && rest.starts_with('=')
        {
            return format!("{p}{rest}");
        }
    }
    value.to_owned()
}

/// Returns `true` if the request carries the session cookie or the guest
/// cookie (whose session holds OAuth states and passkey challenges).
#[must_use]
pub fn has_session_cookie(headers: &[(String, String)]) -> bool {
    headers.iter().filter(|(n, _)| n == "cookie").any(|(_, v)| {
        v.split(';').any(|pair| {
            let name = pair.split('=').next().unwrap_or("").trim();
            let name = name.strip_prefix("__Host-").unwrap_or(name);
            name == SESSION_COOKIE || name == GUEST_COOKIE
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
    fn the_public_session_cookie_is_named_after_an_http_port() {
        assert_eq!(
            public_session_cookie("http://localhost:8081").as_deref(),
            Some("k_session_8081")
        );
        assert_eq!(
            public_session_cookie("http://[::1]:5200/").as_deref(),
            Some("k_session_5200")
        );
        assert_eq!(public_session_cookie("http://localhost"), None);
        assert_eq!(public_session_cookie("http://[::1]"), None);
        assert_eq!(public_session_cookie("https://shop.example:8443"), None);
        assert_eq!(
            to_runtime_cookies("k_session=x; a=1;k_session_81=y", "k_session_81"),
            "a=1; k_session=y"
        );
        assert_eq!(
            to_public_set_cookie("k_session=; Max-Age=0", "k_session_81"),
            "k_session_81=; Max-Age=0"
        );
        assert_eq!(
            to_public_set_cookie("k_sessionx=1", "k_session_81"),
            "k_sessionx=1"
        );
        // The guest cookie is renamed alike.
        assert_eq!(
            to_runtime_cookies("k_guest=x; k_guest_81=g; k_session_81=y", "k_session_81"),
            "k_guest=g; k_session=y"
        );
        assert_eq!(
            to_public_set_cookie("k_guest=g; Path=/", "k_session_81"),
            "k_guest_81=g; Path=/"
        );
        // So is the theme cookie the browser sets: another app's on the
        // same host is not this app's choice.
        assert_eq!(
            to_runtime_cookies("k_theme=dark; k_theme_81=highContrast", "k_session_81"),
            "k_theme=highContrast"
        );
        assert_eq!(to_runtime_cookies("k_theme=dark", "k_session_81"), "");
    }

    #[test]
    fn cookie_detection() {
        assert!(has_session_cookie(&h(&[("cookie", "k_session=1")])));
        assert!(has_session_cookie(&h(&[("cookie", "__Host-k_session=1")])));
        assert!(has_session_cookie(&h(&[("cookie", "a=1; k_guest=1")])));
        assert!(has_session_cookie(&h(&[("cookie", "__Host-k_guest=1")])));
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
