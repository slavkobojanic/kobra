//! The compiler's literal checks and the runtime's parsers agree.
//!
//! `EmailAddress("…")`, `Url("…")` and `Uuid("…")` literals are validated
//! and normalized when a program is compiled, by `k_sema::literals`
//! (sema.md §9.7, K2404); text known only at run time goes through the
//! runtime's `parseEmail`, `parseUrl` and `parseUuid` (`std.js`). A literal
//! must be exactly the value `Type.parse` gives for the same text, so these
//! tests run both on the same inputs, a fixed list of tricky ones and
//! thousands generated from an alphabet of the characters the formats care
//! about, and compare the results: the normalized text, or a refusal.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use std::time::Duration;

use k_server::{Engine, EngineOptions};
use serde::Deserialize;

/// What the runtime's parsers give for each input, in order.
#[derive(Deserialize)]
struct Parsed {
    /// `parseEmail(text)`.
    email: Vec<Option<String>>,
    /// `parseUrl(text)`.
    url: Vec<Option<String>>,
    /// `parseUuid(text)`.
    uuid: Vec<Option<String>>,
}

/// Runs the runtime's parsers on every input in one isolate.
fn runtime_parse(inputs: &[String]) -> Parsed {
    let mut engine = Engine::new(EngineOptions {
        timeout: Some(Duration::from_secs(60)),
        ..Default::default()
    })
    .expect("engine");
    let json = serde_json::to_string(inputs).unwrap();
    let code = format!(
        "import {{ parseEmail, parseUrl, parseUuid }} from \"k:runtime\";\n\
         const inputs = {json};\n\
         Deno.core.ops.op_k_set_result(JSON.stringify({{\n\
           email: inputs.map(parseEmail),\n\
           url: inputs.map(parseUrl),\n\
           uuid: inputs.map(parseUuid),\n\
         }}));\n"
    );
    let text = engine.run_main(&code).expect("runs").expect("a result");
    serde_json::from_str(&text).unwrap()
}

/// Inputs that exercise each rule of the three formats.
const FIXED: &[&str] = &[
    "",
    " ",
    "nobody",
    "ana@example.com",
    "  Ana@Example.COM ",
    "\u{feff}ana@example.com\u{3000}",
    "\u{85}ana@example.com",
    "a@b",
    "a@..cd",
    "a@.cd",
    "a@b.",
    "@b.cd",
    "a@@b.cd",
    "a b@c.de",
    "a@b.c\nBcc: x@y.z",
    "a@b.cd,c@d.ef",
    "a(b@c.de",
    "<a@b.cd>",
    "Ana <ana@example.com>",
    "  K Shop   <orders@Shop.EXAMPLE> ",
    "\"Ana\" <ana@example.com>",
    "J. Doe <j@doe.example>",
    "\"Doe, Ana\" <ana@example.com>",
    "\"Ana \\\"the\\\" Doe\" <ana@example.com>",
    "\"Ana \\\\ Doe\" <ana@example.com>",
    "\"Ana \\",
    "\"  \" <a@b.cd>",
    "\"\" <a@b.cd>",
    "Doe, Ana <ana@example.com>",
    "Ana <ana@example.com",
    "Ana <nobody>",
    "Ana <a@b.cd> x",
    "\"Ana <a@b.cd>",
    "Ana <a@b.cd>, Bo <b@c.de>",
    "A<n>a <a@b.cd>",
    "Ana\u{2028}Doe <a@b.cd>",
    "Ana\u{a0}\u{a0}Doe <a@b.cd>",
    "Ana <a@b.cd>\u{2028}",
    "Ana\t<a@b.cd>",
    "ΣΑΣ <Σ@ΑΣ.ΣΣ>",
    "x@İSTANBUL.example",
    "https://api.stripe.com",
    "http://shop.internal:8080",
    "wss://k.dev/live?room=1#top",
    "https://user:pw@[::1]:8443/p",
    "HTTPS://K.DEV",
    "https://k.dev?q",
    "https://k.dev#f",
    "https://a@b/c@d",
    "/icon.svg",
    "javascript://alert(1)",
    "javascript:alert(1)",
    "data://x",
    "https://",
    "https:///p",
    "https://exa mple.com",
    "https://k.dev:123456",
    "https://k.dev:12345",
    "https://k.dev:",
    "https://k.dev:80x",
    "https://[zz]/",
    "https://[]/",
    "https://[::1]x",
    "https://a@b@c",
    "https://k.dev\\x",
    "https://k.dev/\u{2028}",
    "https://k.dev/é",
    "1https://k.dev",
    "a:b://k.dev",
    "https://:80",
    "0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b",
    "0B6F2C1E-4D1A-4F8E-9A3B-2C5D6E7F8A9B",
    "0b6f2c1e",
    "0b6f2c1e4d1a4f8e9a3b2c5d6e7f8a9b",
    "{0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b}",
    "0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9g",
    "0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b-",
];

/// The characters generated inputs are made of: the ones the formats give
/// a meaning to, JavaScript whitespace that Rust's isn't (and the reverse),
/// line terminators, and letters whose case mapping is unusual.
const ALPHABET: &[char] = &[
    'a', 'b', 'Z', '0', '9', 'f', 'F', 'g', '.', '@', '<', '>', '"', '\\', ',', ';', ':', ' ',
    '\t', '\n', '\u{0}', '\u{7f}', '\u{a0}', '\u{85}', '\u{feff}', '\u{2028}', '\u{3000}', '(',
    ')', '[', ']', '/', '?', '#', '-', '+', 'é', 'İ', 'Σ', '😀',
];

/// The characters of the words generated inputs are built from.
const WORD: &[char] = &['a', 'B', '0', 'f', '-', '_', '.', 'é', 'İ', 'Σ'];

/// The characters of generated display names.
const NAME: &[char] = &[
    'A', 'n', ' ', ' ', '.', ',', '"', '\\', '@', '\u{a0}', '\u{2028}', '\t',
];

/// A fixed-seed xorshift generator, so a failure reproduces.
struct Gen(u64);

impl Gen {
    /// The next number.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number below `n` (`n > 0`).
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
    }

    /// One of `items`.
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    /// Up to `max` characters of `alphabet`.
    fn chars(&mut self, alphabet: &[char], min: usize, max: usize) -> String {
        let len = min + self.below(max - min + 1);
        (0..len).map(|_| self.pick(alphabet)).collect()
    }

    /// `local@domain.tld` from [`WORD`].
    fn email(&mut self) -> String {
        format!(
            "{}@{}.{}",
            self.chars(WORD, 0, 5),
            self.chars(WORD, 0, 5),
            self.chars(WORD, 0, 3)
        )
    }

    /// One input in one of the formats' shapes, or noise.
    fn input(&mut self) -> String {
        let mut s = match self.below(6) {
            0 => self.chars(ALPHABET, 0, 24),
            1 => self.email(),
            2 => format!(
                "{}{}<{}>",
                self.chars(NAME, 0, 8),
                self.chars(&[' ', '\t'], 0, 2),
                self.email()
            ),
            3 => format!("\"{}\" <{}>", self.chars(NAME, 0, 8), self.email()),
            4 => {
                let scheme =
                    self.pick(&["http", "https", "WSS", "ws", "ftp", "javascript", "h+t.p"]);
                let user = if self.below(3) == 0 {
                    format!("{}@", self.chars(WORD, 0, 3))
                } else {
                    String::new()
                };
                let host = if self.below(4) == 0 {
                    format!("[{}]", self.chars(&[':', '1', 'f', '.', 'g'], 0, 5))
                } else {
                    self.chars(WORD, 0, 6)
                };
                let port = if self.below(3) == 0 {
                    format!(":{}", self.chars(&['8', '0'], 0, 6))
                } else {
                    String::new()
                };
                let path = self.chars(&['/', '?', '#', 'a', '@', ':', '['], 0, 4);
                format!("{scheme}://{user}{host}{port}{path}")
            }
            _ => [8, 4, 4, 4, 12]
                .iter()
                .map(|&n| self.chars(&['0', '9', 'a', 'F', 'b'], n, n))
                .collect::<Vec<_>>()
                .join("-"),
        };
        // Mutate a third of them with one character anywhere.
        if self.below(3) == 0 {
            let chars: Vec<char> = s.chars().collect();
            let at = self.below(chars.len() + 1);
            let c = self.pick(ALPHABET);
            s = chars[..at]
                .iter()
                .chain(std::iter::once(&c))
                .chain(chars[at..].iter())
                .collect();
        }
        s
    }
}

/// `count` inputs from a fixed-seed generator.
fn generated(count: usize) -> Vec<String> {
    let mut g = Gen(0x9e37_79b9_7f4a_7c15);
    (0..count).map(|_| g.input()).collect()
}

/// Compares the compiler's parsers with the runtime's on `inputs`.
fn compare(inputs: &[String]) {
    let js = runtime_parse(inputs);
    let mut problems = Vec::new();
    for (i, text) in inputs.iter().enumerate() {
        let email = k_sema::literals::parse_email(text);
        if email != js.email[i] {
            problems.push(format!(
                "email {text:?}: compiler {email:?}, runtime {:?}",
                js.email[i]
            ));
        }
        let url = k_sema::literals::parse_url(text).then(|| text.clone());
        if url != js.url[i] {
            problems.push(format!(
                "url {text:?}: compiler {url:?}, runtime {:?}",
                js.url[i]
            ));
        }
        let uuid = k_sema::literals::parse_uuid(text);
        if uuid != js.uuid[i] {
            problems.push(format!(
                "uuid {text:?}: compiler {uuid:?}, runtime {:?}",
                js.uuid[i]
            ));
        }
        // A literal check refuses exactly what the parser refuses, and
        // gives the parser's value.
        assert_eq!(
            k_sema::literals::email_address(text).ok(),
            email,
            "{text:?}"
        );
        assert_eq!(k_sema::literals::url(text).ok(), url, "{text:?}");
        assert_eq!(k_sema::literals::uuid(text).ok(), uuid, "{text:?}");
    }
    assert!(
        problems.is_empty(),
        "{} disagreements:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn the_fixed_inputs_parse_alike() {
    let inputs: Vec<String> = FIXED.iter().map(|s| (*s).to_owned()).collect();
    compare(&inputs);
}

#[test]
fn generated_inputs_parse_alike() {
    compare(&generated(20_000));
}
