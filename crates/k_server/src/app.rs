//! The `dist/app` archive: a copy of the slim runtime executable with the
//! app appended (`docs/compiler/server.md`, "Packaging").
//!
//! Layout of the appended trailer, read from the end of the file:
//!
//! ```text
//! [executable bytes][payload JSON][payload length: u64 LE][magic: b"K-APP-v1"]
//! ```
//!
//! The payload is a JSON [`AppPayload`]: the server modules, the rpc
//! paths, the hashed client assets, the configuration table and the
//! session-free endpoint templates. Every v0 asset is text (JavaScript,
//! CSS, source maps); binary assets, the migrations and the V8 snapshot
//! (which replaces the module sources) join the payload with the features
//! that need them (server.md, "Packaging").

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The trailer's magic bytes.
pub const MAGIC: &[u8; 8] = b"K-APP-v1";

/// One embedded file.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmbeddedFile {
    /// Its path, a module specifier or an asset URL path.
    pub path: String,
    /// Its text (every v0 asset is text: JS, CSS, source maps).
    pub text: String,
}

/// One embedded binary asset: a font a theme ships
/// (07-themes-and-units.md, "Shipped fonts").
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmbeddedBinary {
    /// Its URL path, `/assets/fonts/Inter.3f2a1b9c.woff2`.
    pub path: String,
    /// Its `Content-Type`, `font/woff2`.
    pub media_type: String,
    /// Its bytes, base64 (RFC 4648, padded) in the trailer's JSON.
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
}

/// Bytes as a base64 string in JSON, a third larger rather than the four
/// times of an array of numbers.
mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    /// The standard alphabet.
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Encodes `bytes`, padded.
    pub(super) fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    /// Decodes padded base64; `None` for anything else.
    pub(super) fn decode(text: &str) -> Option<Vec<u8>> {
        let text = text.trim_end_matches('=');
        let mut out = Vec::with_capacity(text.len() * 3 / 4);
        let mut acc: u32 = 0;
        let mut bits = 0;
        for c in text.bytes() {
            let v = ALPHABET.iter().position(|&a| a == c)?;
            acc = (acc << 6) | u32::try_from(v).ok()?;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(u8::try_from((acc >> bits) & 0xFF).ok()?);
            }
        }
        Some(out)
    }

    /// Writes `bytes` as a base64 string.
    pub(super) fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(bytes))
    }

    /// Reads a base64 string.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        decode(&text).ok_or_else(|| serde::de::Error::custom("invalid base64"))
    }
}

/// Everything `dist/app` carries besides its own code.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct AppPayload {
    /// The app's name from `k.toml`.
    pub name: String,
    /// The server bundle's modules.
    pub modules: Vec<EmbeddedFile>,
    /// The entry module's specifier.
    pub entry: String,
    /// Every rpc path.
    pub rpc_paths: Vec<String>,
    /// The hashed client assets, by URL path.
    pub assets: Vec<EmbeddedFile>,
    /// The hashed binary client assets (shipped fonts), by URL path.
    /// Absent from archives written before they existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binary: Vec<EmbeddedBinary>,
    /// Every `@Env` field the build reads at startup (the build manifest's
    /// config table). Absent from archives written before it existed.
    #[serde(default)]
    pub config: Vec<ConfigSpec>,
    /// URL templates of endpoint pages that don't inject `Session`
    /// ([`crate::ServerOptions::forgery_exempt`]).
    #[serde(default)]
    pub forgery_exempt: Vec<String>,
    /// The database half of the build: statements by number, migrations,
    /// unique indexes (`docs/compiler/db.md` §6).
    #[serde(default)]
    pub database: Option<k_db::manifest::DbManifest>,
    /// The background jobs (the build manifest's `jobs`), which the jobs
    /// worker of `serve` runs.
    #[serde(default)]
    pub jobs: Vec<k_db::stdlib::JobSpec>,
}

/// One configuration variable as the build manifest describes it
/// (`k_codegen::ConfigEntry`, without depending on the compiler).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ConfigSpec {
    /// The variable, `SMOKE_TOKEN`.
    pub env: String,
    /// Where it is declared, `ops.SmokeConfig.token`.
    pub declared_at: String,
    /// The K type as text, `Secret<String>`.
    pub ty: String,
    /// `true` for `Secret<T>`.
    pub secret: bool,
    /// `true` when it has no default and isn't optional.
    pub required: bool,
    /// The default as environment text.
    pub default: Option<String>,
    /// Enum case names, for an enum type.
    #[serde(default)]
    pub cases: Vec<String>,
    /// `@Range(lo...hi)` bounds of an `Int`.
    #[serde(default)]
    pub range: Option<(i64, i64)>,
    /// The pattern of a `@Matches` constraint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// That constraint's `message:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_message: Option<String>,
}

impl ConfigSpec {
    /// The field [`crate::config::resolve`] reads, or `None` for a type the
    /// runtime can't read from text.
    #[must_use]
    pub fn field(&self) -> Option<crate::config::ConfigField> {
        crate::config::ConfigField::from_manifest(
            &self.env,
            &self.declared_at,
            &self.ty,
            self.secret,
            self.required,
            self.default.as_deref(),
            &self.cases,
            self.range,
        )
        .map(|f| f.with_pattern(self.pattern.as_deref(), self.pattern_message.as_deref()))
    }

    /// The server's own variables (`k.server.ServerConfig`), which every
    /// app reads; used when an archive lists no configuration.
    #[must_use]
    pub fn server_defaults() -> Vec<ConfigSpec> {
        let one = |env: &str, field: &str, ty: &str, default: Option<&str>| ConfigSpec {
            env: env.to_owned(),
            declared_at: format!("k.server.ServerConfig.{field}"),
            ty: ty.to_owned(),
            default: default.map(str::to_owned),
            ..ConfigSpec::default()
        };
        vec![
            one("K_PUBLIC_ORIGIN", "publicOrigin", "Url", None),
            one("K_SHUTDOWN_GRACE", "shutdownGrace", "Duration", Some("25s")),
            one(
                "K_TRUSTED_PROXIES",
                "trustedProxies",
                "List<String>",
                Some(""),
            ),
            one("PORT", "port", "Int", Some("8080")),
            ConfigSpec {
                range: Some((16, 16384)),
                ..one("K_WORKER_HEAP_MB", "workerHeapMb", "Int", Some("256"))
            },
            one(
                "K_PUBLIC_SOURCE_MAPS",
                "publicSourceMaps",
                "Bool",
                Some("false"),
            ),
        ]
    }
}

/// Why an archive couldn't be read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ArchiveError(pub String);

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not a Kobra app archive: {}", self.0)
    }
}

impl std::error::Error for ArchiveError {}

/// Appends `payload` to `executable` as a trailer.
///
/// # Errors
///
/// Serialization failure, as an [`ArchiveError`].
pub fn append(executable: &[u8], payload: &AppPayload) -> Result<Vec<u8>, ArchiveError> {
    let json = serde_json::to_vec(payload).map_err(|e| ArchiveError(e.to_string()))?;
    let mut out = Vec::with_capacity(executable.len() + json.len() + 16);
    out.extend_from_slice(executable);
    out.extend_from_slice(&json);
    out.extend_from_slice(&(json.len() as u64).to_le_bytes());
    out.extend_from_slice(MAGIC);
    Ok(out)
}

/// Reads the trailer of `file` (the running executable's bytes). Returns
/// `Ok(None)` when the file has no trailer (the plain `k` binary).
///
/// # Errors
///
/// A trailer that is present but truncated or malformed.
pub fn read(file: &[u8]) -> Result<Option<AppPayload>, ArchiveError> {
    let Some(rest) = file.strip_suffix(MAGIC.as_slice()) else {
        return Ok(None);
    };
    if rest.len() < 8 {
        return Err(ArchiveError("truncated trailer".to_owned()));
    }
    let (body, len) = rest.split_at(rest.len() - 8);
    let len = u64::from_le_bytes(
        len.try_into()
            .map_err(|_| ArchiveError("bad length".to_owned()))?,
    );
    let len = usize::try_from(len).map_err(|_| ArchiveError("bad length".to_owned()))?;
    if len > body.len() {
        return Err(ArchiveError(
            "the payload is longer than the file".to_owned(),
        ));
    }
    let json = &body[body.len() - len..];
    serde_json::from_slice(json)
        .map(Some)
        .map_err(|e| ArchiveError(e.to_string()))
}

/// Reads the trailer of the file at `path` without reading the whole file:
/// the last 16 bytes first, then only the payload. Returns `Ok(None)` for a
/// file without a trailer.
///
/// # Errors
///
/// I/O failures and malformed trailers, as [`ArchiveError`].
pub fn read_file(path: &Path) -> Result<Option<AppPayload>, ArchiveError> {
    let io = |e: std::io::Error| ArchiveError(e.to_string());
    let mut f = std::fs::File::open(path).map_err(io)?;
    let size = f.metadata().map_err(io)?.len();
    if size < 16 {
        return Ok(None);
    }
    f.seek(SeekFrom::End(-16)).map_err(io)?;
    let mut tail = [0u8; 16];
    f.read_exact(&mut tail).map_err(io)?;
    if &tail[8..] != MAGIC.as_slice() {
        return Ok(None);
    }
    let len = u64::from_le_bytes(
        tail[..8]
            .try_into()
            .map_err(|_| ArchiveError("bad length".to_owned()))?,
    );
    if len > size - 16 {
        return Err(ArchiveError(
            "the payload is longer than the file".to_owned(),
        ));
    }
    f.seek(SeekFrom::End(
        -16 - i64::try_from(len).map_err(|_| ArchiveError("bad length".to_owned()))?,
    ))
    .map_err(io)?;
    let mut json =
        vec![0u8; usize::try_from(len).map_err(|_| ArchiveError("bad length".to_owned()))?];
    f.read_exact(&mut json).map_err(io)?;
    serde_json::from_slice(&json)
        .map(Some)
        .map_err(|e| ArchiveError(e.to_string()))
}

/// Sends `GET url` (an `http://host:port/path` URL) and returns `true` for
/// a 2xx answer within `timeout` (`dist/app probe`,
/// `docs/design/38-deployment.md`). Never panics; any failure is `false`.
#[must_use]
pub fn probe(url: &str, timeout: std::time::Duration) -> bool {
    use std::io::Write;
    use std::net::ToSocketAddrs;
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let Some(addr) = authority.to_socket_addrs().ok().and_then(|mut a| a.next()) else {
        return false;
    };
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(&addr, timeout) else {
        return false;
    };
    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
    {
        return false;
    }
    let request = format!("GET {path} HTTP/1.1\r\nhost: {authority}\r\nconnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut head = [0u8; 12];
    if stream.read_exact(&mut head).is_err() {
        return false;
    }
    head.starts_with(b"HTTP/1.") && head[9] == b'2'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> AppPayload {
        AppPayload {
            name: "demo".into(),
            modules: vec![EmbeddedFile {
                path: "k:/app/server.js".into(),
                text: "export {}".into(),
            }],
            entry: "k:/app/server.js".into(),
            rpc_paths: vec!["/demo.A/B".into()],
            assets: vec![],
            binary: vec![EmbeddedBinary {
                path: "/assets/fonts/Inter.3f2a1b9c.woff2".into(),
                media_type: "font/woff2".into(),
                bytes: vec![b'w', b'O', b'F', b'2', 0, 0xFF, 7],
            }],
            config: ConfigSpec::server_defaults(),
            forgery_exempt: vec!["/hooks/{id}".into()],
            database: None,
            jobs: Vec::new(),
        }
    }

    #[test]
    fn round_trips_in_memory_and_on_disk() {
        let bytes = append(b"\x7fELF fake executable", &payload()).expect("appends");
        assert_eq!(read(&bytes).expect("reads"), Some(payload()));
        let path = std::env::temp_dir().join(format!("k_app_archive_{}", std::process::id()));
        std::fs::write(&path, &bytes).expect("writes");
        assert_eq!(read_file(&path).expect("reads"), Some(payload()));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn binary_assets_are_base64_in_the_trailer() {
        for n in 0..8usize {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 200) as u8).collect();
            let text = base64_bytes::encode(&bytes);
            assert_eq!(text.len() % 4, 0);
            assert_eq!(base64_bytes::decode(&text), Some(bytes));
        }
        assert_eq!(base64_bytes::encode(b"wOF2"), "d09GMg==");
    }

    #[test]
    fn files_without_a_trailer_are_not_apps() {
        assert_eq!(read(b"plain binary").expect("reads"), None);
        assert_eq!(read(b"").expect("reads"), None);
    }

    #[test]
    fn malformed_trailers_are_errors() {
        let mut bad = b"xx".to_vec();
        bad.extend_from_slice(&1000u64.to_le_bytes());
        bad.extend_from_slice(MAGIC);
        assert!(read(&bad).is_err());
        let mut garbage = b"not json".to_vec();
        garbage.extend_from_slice(&8u64.to_le_bytes());
        garbage.extend_from_slice(MAGIC);
        assert!(read(&garbage).is_err());
    }
}
