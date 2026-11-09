//! Golden files for `k.crypto`'s wire formats (docs/design/33-crypto.md,
//! "Testing"): `Sealed`, `WrappedKey`, `Envelope` and `EnvelopedKey`, made
//! on the server's `ops` backend from `TestKeys` and a fixed random source,
//! so a change to a format is a reviewed diff of `tests/crypto/*.golden`.
//! Each golden value is also opened again, so values written by an earlier
//! build keep opening.
//!
//! ```text
//! cargo test -p k_server --test it -- crypto_wire::
//! K_BLESS=1 cargo test -p k_server --test it -- crypto_wire::   # rewrite them
//! ```

#![allow(clippy::panic)]

use std::path::PathBuf;
use std::time::Duration;

use k_server::{Engine, EngineOptions};

/// `crates/k_server/tests/crypto/<name>`.
fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/crypto")
        .join(name)
}

/// Runs `body` (module code that sets `out`, an array of lines) with the
/// crypto module as `c`, its `ops` backend and a random source that yields
/// 0, 1, 2, … so every value is the same on every run.
fn run(body: &str) -> String {
    let mut e = Engine::new(EngineOptions {
        timeout: Some(Duration::from_secs(20)),
        ..Default::default()
    })
    .expect("engine");
    let code = format!(
        "import * as c from \"k:/runtime/crypto.js\";\n\
         import {{ codecs, utf8 }} from \"k:runtime\";\n\
         c.useOps(Deno.core.ops);\n\
         let n = 0;\n\
         globalThis.crypto = {{ getRandomValues: (a) => {{ for (let i = 0; i < a.length; i++) a[i] = n++ & 255; return a; }} }};\n\
         const out = [];\n\
         {body}\n\
         Deno.core.ops.op_k_set_result(out.join(\"\\n\") + \"\\n\");\n"
    );
    e.run_main(&code).expect("runs").expect("a result")
}

/// Compares `actual` with golden file `name`, or rewrites it under
/// `K_BLESS=1`.
fn assert_golden(name: &str, actual: &str) {
    let path = golden(name);
    if std::env::var("K_BLESS").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("creates the directory");
        std::fs::write(&path, actual).expect("writes the golden file");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (bless with K_BLESS=1)", path.display()));
    assert_eq!(
        actual,
        expected,
        "{} changed: a wire format change is a reviewed diff (bless with K_BLESS=1)",
        path.display()
    );
}

/// The values of `wire.golden`, by name, as the K members make them.
const MAKE: &str = r#"
const aes = c.TestKeys.aesGcm(1);
const exportable = (type, seed) => c.ops.fromRaw(type, c.ops.digest(256, utf8(seed)), true);
out.push("sealed bytes: " + await c.AesGcmKey$seal(aes, null, codecs.bytes, utf8("hello"), utf8("note:1")));
out.push("sealed json: " + await c.AesGcmKey$seal(aes, null, codecs.list(codecs.string), ["a", "b"]));
out.push("wrapped AesGcmKey: " + await c.AesGcmKey$wrap(aes, null, exportable("AesGcmKey", "vault")));
out.push("wrapped Ed25519PrivateKey: " + await c.AesGcmKey$wrap(aes, null, exportable("Ed25519PrivateKey", "signer")));
const recipient = c.X25519PrivateKey$publicKey(c.TestKeys.x25519(1));
out.push("recipient: " + recipient);
out.push("envelope: " + await c.X25519PublicKey$seal(recipient, null, codecs.string, "for you", utf8("share:1")));
out.push("enveloped AesGcmKey: " + await c.X25519PublicKey$wrap(recipient, null, exportable("AesGcmKey", "note")));
out.push("ed25519 public key: " + c.Ed25519PrivateKey$publicKey(c.TestKeys.ed25519(1)));
out.push("ed25519 signature: " + await c.Ed25519PrivateKey$sign(c.TestKeys.ed25519(1), null, utf8("manifest")));
out.push("p256 public key: " + c.EcdsaP256PrivateKey$publicKey(c.TestKeys.ecdsaP256(1)));
out.push("passphrase salt: " + c.PassphraseSalt$generate());
"#;

#[test]
fn the_wire_formats_match_their_golden_file() {
    assert_golden("wire.golden", &run(MAKE));
}

#[test]
fn golden_values_still_open() {
    let text = std::fs::read_to_string(golden("wire.golden")).expect("the golden file");
    let value = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{name}: ")))
            .unwrap_or_else(|| panic!("{name} in wire.golden"))
            .to_owned()
    };
    let opened = run(&format!(
        r#"
const aes = c.TestKeys.aesGcm(1);
const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
out.push(String.fromCharCode(...(await c.AesGcmKey$open(aes, null, codecs.bytes, {sealed:?}, utf8("note:1")))));
out.push((await c.AesGcmKey$open(aes, null, codecs.list(codecs.string), {json:?})).join(","));
const vault = await c.AesGcmKey$unwrap(aes, null, "AesGcmKey", {wrapped:?});
out.push(hex(vault.secret) === hex(c.ops.digest(256, utf8("vault"))) ? "vault unwrapped" : "vault differs");
const signer = await c.AesGcmKey$unwrap(aes, null, "Ed25519PrivateKey", {signer:?});
out.push(c.Ed25519PrivateKey$publicKey(signer).length === 43 ? "signer unwrapped" : "signer differs");
out.push(await c.X25519PrivateKey$open(c.TestKeys.x25519(1), null, codecs.string, {envelope:?}, utf8("share:1")));
const note = await c.X25519PrivateKey$unwrap(c.TestKeys.x25519(1), null, "AesGcmKey", {enveloped:?});
out.push(hex(note.secret) === hex(c.ops.digest(256, utf8("note"))) ? "note key unwrapped" : "note key differs");
out.push(String(await c.Ed25519PublicKey$verify({public:?}, null, {signature:?}, utf8("manifest"))));
"#,
        sealed = value("sealed bytes"),
        json = value("sealed json"),
        wrapped = value("wrapped AesGcmKey"),
        signer = value("wrapped Ed25519PrivateKey"),
        envelope = value("envelope"),
        enveloped = value("enveloped AesGcmKey"),
        public = value("ed25519 public key"),
        signature = value("ed25519 signature"),
    ));
    assert_eq!(
        opened,
        "hello\na,b\nvault unwrapped\nsigner unwrapped\nfor you\nnote key unwrapped\ntrue\n"
    );
}
