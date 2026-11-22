//! Kobra's releases on GitHub, which are private
//! (`docs/design/48-releases-and-versioning.md`, "Private distribution"):
//! `k toolchain install` and `k self update` read them through GitHub's
//! REST API with the user's token.
//!
//! * **The token** is `GH_TOKEN`, else `GITHUB_TOKEN`, else what `gh auth
//!   token` prints when the GitHub CLI is on the `PATH`, asked only when a
//!   request to the API needs it.
//! * **It goes to the API's origin only** (`https://api.github.com`, or
//!   `K_GITHUB_API`, which the tests point at a loopback fake): never to
//!   an index or archive on another host, and never to the storage host an
//!   asset's API URL redirects to (that URL is signed already).
//! * **The index** is the `index.json` asset of the newest published
//!   release (the largest `published_at`, drafts skipped): every release
//!   and nightly attaches the whole index as it was when it was published.
//! * **An archive URL** of the form
//!   `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>`,
//!   the URL the index lists and the one a public repository serves, is
//!   fetched as that release's asset through the API.

use std::sync::OnceLock;

/// The repository the releases are published in.
pub(crate) const REPO: &str = "slavkobojanic/kobra";

/// The variable naming the API's URL (for tests).
const API_VAR: &str = "K_GITHUB_API";

/// GitHub's REST API.
const DEFAULT_API: &str = "https://api.github.com";

/// The API's media type for JSON.
const JSON: &str = "application/vnd.github+json";

/// The API's media type for an asset's bytes.
pub(crate) const OCTET_STREAM: &str = "application/octet-stream";

/// What to do without access: the sentence every access failure ends with.
pub(crate) const SIGN_IN: &str = "sign in with the GitHub CLI (`gh auth login`), \
     or set GH_TOKEN to a token that can read slavkobojanic/kobra";

/// The API and the token for it.
pub(crate) struct Github {
    /// The API's URL, without a trailing `/`.
    pub(crate) api: String,
    /// The token, looked up on first use.
    token: OnceLock<Option<String>>,
}

impl Github {
    /// The API `K_GITHUB_API` names, else GitHub's.
    pub(crate) fn from_env() -> Github {
        let api = std::env::var(API_VAR)
            .ok()
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| DEFAULT_API.to_owned());
        Github {
            api: api.trim_end_matches('/').to_owned(),
            token: OnceLock::new(),
        }
    }

    /// The token: `GH_TOKEN`, `GITHUB_TOKEN`, else `gh auth token`.
    pub(crate) fn token(&self) -> Option<&str> {
        self.token
            .get_or_init(|| {
                for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
                    if let Some(t) = std::env::var(var)
                        .ok()
                        .map(|t| t.trim().to_owned())
                        .filter(|t| !t.is_empty())
                    {
                        return Some(t);
                    }
                }
                let out = std::process::Command::new("gh")
                    .args(["auth", "token"])
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .ok()?;
                let token = String::from_utf8(out.stdout).ok()?.trim().to_owned();
                (out.status.success() && !token.is_empty()).then_some(token)
            })
            .as_deref()
    }

    /// `true` when `url` is on the API's origin (scheme, host and port).
    pub(crate) fn is_api(&self, url: &str) -> bool {
        origin(url).is_some_and(|o| Some(o) == origin(&self.api))
    }

    /// The headers a request to `url` carries: on the API's origin, the
    /// token and `accept`; elsewhere none.
    pub(crate) fn headers(&self, url: &str, accept: &str) -> Vec<(String, String)> {
        if !self.is_api(url) {
            return Vec::new();
        }
        let mut headers = vec![
            ("accept".to_owned(), accept.to_owned()),
            ("x-github-api-version".to_owned(), "2022-11-28".to_owned()),
        ];
        if let Some(token) = self.token() {
            headers.push(("authorization".to_owned(), format!("Bearer {token}")));
        }
        headers
    }

    /// The JSON headers for `url` (see [`Github::headers`]).
    pub(crate) fn json_headers(&self, url: &str) -> Vec<(String, String)> {
        self.headers(url, JSON)
    }

    /// The URL of an API path (`repos/<owner>/<repo>/releases`).
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}/{}", self.api, path.trim_start_matches('/'))
    }

    /// The failure when no token was found.
    pub(crate) fn no_token() -> String {
        format!(
            "Kobra's releases are in the private repository {REPO}, and no GitHub token was found \
             (GH_TOKEN, GITHUB_TOKEN or `gh auth token`): {SIGN_IN}"
        )
    }
}

/// `scheme://host[:port]` of `url`, in lower case.
fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    // Credentials in a URL are never a GitHub origin.
    if authority.contains('@') || authority.is_empty() {
        return None;
    }
    let scheme = scheme.to_ascii_lowercase();
    let mut authority = authority.to_ascii_lowercase();
    let default = match scheme.as_str() {
        "https" => ":443",
        "http" => ":80",
        _ => "",
    };
    if let Some(stripped) = authority.strip_suffix(default) {
        authority = stripped.to_owned();
    }
    Some(format!("{scheme}://{authority}"))
}

/// The repository, tag and file name of a release download URL,
/// `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>`.
pub(crate) fn release_download(url: &str) -> Option<(String, String, String)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [owner, repo, "releases", "download", tag, name]
            if !owner.is_empty() && !repo.is_empty() && !tag.is_empty() && !name.is_empty() =>
        {
            Some((
                format!("{owner}/{repo}"),
                (*tag).to_owned(),
                (*name).to_owned(),
            ))
        }
        _ => None,
    }
}

/// The API URL of the asset `name` in a release's JSON.
pub(crate) fn asset_url(release: &serde_json::Value, name: &str) -> Option<String> {
    release
        .get("assets")?
        .as_array()?
        .iter()
        .find(|a| a.get("name").and_then(serde_json::Value::as_str) == Some(name))?
        .get("url")?
        .as_str()
        .map(ToOwned::to_owned)
}

/// The newest published release (largest `published_at`, drafts skipped)
/// in a list of releases that has an asset `name`: its tag and the asset's
/// API URL.
pub(crate) fn newest_with(releases: &serde_json::Value, name: &str) -> Option<(String, String)> {
    releases
        .as_array()?
        .iter()
        .filter(|r| r.get("draft").and_then(serde_json::Value::as_bool) != Some(true))
        .filter_map(|r| {
            let published = r.get("published_at")?.as_str()?;
            let tag = r.get("tag_name")?.as_str()?;
            Some((published, tag.to_owned(), asset_url(r, name)?))
        })
        .max_by(|a, b| a.0.cmp(b.0))
        .map(|(_, tag, url)| (tag, url))
}

/// `message` (a failed download of an API URL) with what to do about it
/// when GitHub refused: 404 is what GitHub answers for a private
/// repository the token can't read, 401 for a bad token.
pub(crate) fn explain(message: String) -> String {
    if message.contains("answered 404") || message.contains("answered 403") {
        format!("{message}: you may not have access to {REPO}; {SIGN_IN}")
    } else if message.contains("answered 401") {
        format!("{message}: GitHub refused the token; {SIGN_IN}")
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github(api: &str) -> Github {
        let g = Github {
            api: api.to_owned(),
            token: OnceLock::new(),
        };
        let _ = g.token.set(Some("t0k".to_owned()));
        g
    }

    #[test]
    fn the_token_goes_to_the_apis_origin_only() {
        let g = github("https://api.github.com");
        assert!(g.is_api("https://api.github.com/repos/a/b"));
        assert!(g.is_api("https://API.github.com:443/x"));
        for other in [
            "http://api.github.com/x",
            "https://api.github.com.evil.example/x",
            "https://evil.example/https://api.github.com/",
            "https://user@api.github.com/x",
            "https://objects.githubusercontent.com/x",
            "https://github.com/slavkobojanic/kobra/releases/download/v1/k.tar.gz",
            "not a url",
        ] {
            assert!(!g.is_api(other), "{other}");
            assert!(g.headers(other, JSON).is_empty(), "{other}");
        }
        let headers = g.json_headers("https://api.github.com/repos/a/b/releases");
        assert!(headers.contains(&("authorization".to_owned(), "Bearer t0k".to_owned())));
        let local = github("http://127.0.0.1:5300");
        assert!(local.is_api("http://127.0.0.1:5300/repos"));
        assert!(!local.is_api("http://127.0.0.1:5301/repos"));
    }

    #[test]
    fn release_download_urls_are_recognised() {
        assert_eq!(
            release_download(
                "https://github.com/slavkobojanic/kobra/releases/download/v0.1.0/k-0.1.0-x.tar.gz"
            ),
            Some((
                "slavkobojanic/kobra".to_owned(),
                "v0.1.0".to_owned(),
                "k-0.1.0-x.tar.gz".to_owned()
            ))
        );
        for no in [
            "https://github.com/slavkobojanic/kobra/releases/latest/download/k.tar.gz",
            "https://example.com/slavkobojanic/kobra/releases/download/v1/k",
            "http://github.com/a/b/releases/download/v1/k",
        ] {
            assert_eq!(release_download(no), None, "{no}");
        }
    }

    #[test]
    fn the_index_is_the_newest_published_releases() {
        let list = serde_json::json!([
            { "tag_name": "v3", "draft": true, "published_at": null,
              "assets": [{ "name": "index.json", "url": "u3" }] },
            { "tag_name": "v1", "draft": false, "published_at": "2026-11-01T00:00:00Z",
              "assets": [{ "name": "index.json", "url": "u1" }] },
            { "tag_name": "v2", "draft": false, "published_at": "2026-11-02T00:00:00Z",
              "assets": [{ "name": "index.json", "url": "u2" }] },
            { "tag_name": "v4", "draft": false, "published_at": "2026-11-04T00:00:00Z",
              "assets": [{ "name": "SHA256SUMS", "url": "u4" }] },
        ]);
        assert_eq!(
            newest_with(&list, "index.json"),
            Some(("v2".to_owned(), "u2".to_owned()))
        );
        assert_eq!(newest_with(&serde_json::json!([]), "index.json"), None);
    }
}
