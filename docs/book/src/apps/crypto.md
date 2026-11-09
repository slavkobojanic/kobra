# Bytes and cryptography

`Bytes` is a core type, and `k.crypto` provides HMAC, hashing, secure random values, UUIDs and password
hashing. **Safety comes from types:** a MAC is a distinct `Mac` type that can't be compared with `==`, so
the constant-time comparison is the only one that compiles.

```k
// examples/shop/shop/src/payments/StripeSignature.k
func matches(payload: Bytes, secret: Secret<String>) async -> Bool {
    let signedPayload = "{signedAt.unixSeconds}.".utf8 + payload
    let expected = await Hmac.sha256(key: secret, message: signedPayload)

    return candidates.contains { candidate => expected.matches(candidate) }
}
```

## `Bytes`

`Bytes` is an immutable byte sequence with value semantics, needing no import. It implements `Equatable`
and `Hashable`.

| Member | Meaning |
|---|---|
| `"text".utf8` | a string's UTF-8 encoding |
| `a + b` | concatenation |
| `bytes.count` | the length |
| `bytes.hex` | lowercase hex text |
| `Bytes.parse(hex: text) -> Bytes?` | parses hex in either case; `nil` for odd lengths or other characters |
| `bytes.base64`, `Bytes.parse(base64: text) -> Bytes?` | Base64 |
| `bytes.utf8Text -> String?` | decodes UTF-8; `nil` if invalid |

## HMAC: `Hmac` and `Mac`

| Member | Meaning |
|---|---|
| `Hmac.sha256(key:message:) async -> Mac`, `Hmac.sha512(key:message:) async -> Mac` | the HMAC of `message: Bytes`. `key` is `Bytes` or a `Secret<String>`, so a configured secret needs no `reveal()`. |
| `mac.matches(_ other: Bytes) -> Bool` | compares in constant time, so timing doesn't reveal how many leading bytes match |
| `mac.bytes`, `mac.hex` | the digest, to send it somewhere |

- **`Mac` doesn't implement `Equatable`.** Comparing two MACs with `==` is a compile error by type, with a
  hint to use `matches`.
- **Digests are `async`,** because they are built on Web Crypto, which is asynchronous in browsers and in
  the server runtime. They are available on both sides; in practice the key is a `Secret`, which keeps
  the calling code on the server.

Verifying a webhook signature, with the header's hex signatures already parsed by `Bytes.parse(hex:)`:

```k
~import k.crypto.Hmac
/// Checks the signatures of a `Stripe-Signature` header against the raw request body.
func matches(payload: Bytes, timestamp: String, candidates: List<Bytes>, secret: Secret<String>) async -> Bool {
    let signedPayload = "{timestamp}.".utf8 + payload
    let expected = await Hmac.sha256(key: secret, message: signedPayload)

    return candidates.contains { candidate => expected.matches(candidate) }
}
```

## Hashing

`Hash.sha256(data: Bytes) async -> Bytes` and `Hash.sha512(data:)` compute plain digests. A digest isn't
a secret, so it is ordinary `Bytes`.

## `Uuid`

| Member | Meaning |
|---|---|
| `Uuid("0b6f2c1e-…")` | a literal, validated at compile time |
| `Uuid.parse(text) -> Uuid?` | parses a runtime string, in canonical form, either case |
| `Uuid.random()` | a version 4 UUID from a secure random source |
| `"{uuid}"` | canonical lowercase text |

- `Uuid` implements `Codable` (as a string), `Hashable` and `ConfigValue`, and **works as a `@Param`
  type**: an invalid id in a URL is a `404`.
- **Random UUIDs are unguessable** (122 random bits), so a URL containing one can act as a capability
  link, such as a receipt page.

## Secure random values

`SecureRandom.bytes(count:) -> Bytes` and `SecureRandom.token(bytes:) -> String` (URL-safe Base64) make
tokens. There is no non-secure random source in `k.crypto`.

## Passwords

```k
~import k.crypto.Password
~
~struct SignUp {
~    let password: String
~}
~
~func example(request: SignUp, typed: String, stored: Secret<String>) async {
let hash = await Password.hash(password: request.password)            // Argon2id, as a PHC string
let matches = await Password.verify(password: typed, hash: stored)     // constant time
~}
```

- Both are server-only and `async`, because hashing is deliberately slow.
- **`Password.unusableHash`** is a valid hash that no password matches. Verifying against it when no
  account has the email keeps sign-in timing the same whether or not the email is registered
  ([Authentication and sessions](auth.md)).
- `hash` may also be a `Secret<String>`, for a hash held in configuration.
- Don't log a field named `password` (a warning for it is **Planned** password-log-lint). A
  `Secret<…>` logs as `Secret(•••)`.
