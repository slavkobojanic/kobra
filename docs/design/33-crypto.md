# Bytes and Cryptography

## Decision

`Bytes` is a core type. `k.crypto` provides HMAC, hashing, secure random values, UUIDs and password hashing. **Safety comes from types:** a MAC is a distinct `Mac` type that can't be compared with `==`, so the constant-time comparison is the only one that compiles.

```k
let mac = await Hmac.sha256(key: config.signingSecret, message: "{timestamp}.".utf8 + payload)

guard mac.matches(signature) else {
    throw WebhookError.invalidSignature
}
```

## `Bytes` (core, no import)

`Bytes` is an immutable byte sequence with value semantics. It implements `Equatable` and `Hashable`.

| Member | Meaning |
|---|---|
| `"text".utf8` | a string's UTF-8 encoding |
| `a + b` | concatenation |
| `bytes.count` | the length |
| `bytes.hex` | lowercase hex text |
| `Bytes.parse(hex: text) -> Bytes?` | parses hex (either case); `nil` for odd lengths or other characters |
| `bytes.base64`, `Bytes.parse(base64: text) -> Bytes?` | Base64, as in `k.encoding` |
| `bytes.utf8Text -> String?` | decodes UTF-8; `nil` if invalid |

## HMAC: `Hmac` and `Mac`

| Member | Meaning |
|---|---|
| `Hmac.sha256(key:message:) async -> Mac`, `Hmac.sha512(key:message:) async -> Mac` | the HMAC of `message: Bytes`. `key` is `Bytes` or a `Secret<String>` (used as UTF-8), so a configured secret needs no `reveal()`. |
| `mac.matches(_ other: Bytes) -> Bool` | compares in constant time, so timing doesn't reveal how many leading bytes match |
| `mac.bytes`, `mac.hex` | the digest, to send it somewhere |

- **`Mac` doesn't implement `Equatable`.** Comparing two MACs with `==` is a compile error by type, with a hint to use `matches`. No data-flow analysis is involved, which is the technique `Secret` uses too ([25-config-and-secrets.md](25-config-and-secrets.md#secrett)).
- **Digests are `async`,** because they are built on Web Crypto's `subtle` API, which is asynchronous in browsers and in the server runtime ([13-tooling.md](13-tooling.md)). They are available on both sides; in practice the key is a `Secret`, which keeps the calling code on the server.

## Hashing

`Hash.sha256(data: Bytes) async -> Bytes` and `Hash.sha512(data:)` compute plain digests. A digest isn't a secret, so it is ordinary `Bytes`.

## `Uuid`

| Member | Meaning |
|---|---|
| `Uuid("0b6f2c1e-…")` | a literal, validated at compile time ([20-standard-library.md](20-standard-library.md#literals-and-parsing)) |
| `Uuid.parse(text) -> Uuid?` | parses a runtime string, in canonical form, either case |
| `Uuid.random()` | a version 4 UUID from a secure random source |
| `"{uuid}"` | canonical lowercase text |

- `Uuid` implements `Codable` (as a string), `Hashable` and `ConfigValue`, and **works as a `@Param` type**: an invalid id in a URL is a `404` ([12-routing.md](12-routing.md#parameters)).
- **Random UUIDs are unguessable** (122 random bits), so a URL containing one can act as a capability link, such as a receipt page.

## Secure random values

`SecureRandom.bytes(count:) -> Bytes` and `SecureRandom.token(bytes:) -> String` (URL-safe Base64) for tokens. There is no non-secure random source in `k.crypto`.

## Passwords

```k
let hash = await Password.hash(password: request.password)            // Argon2id, as a PHC string
let matches = await Password.verify(password: typed, hash: stored)     // constant time
```

- Both are server-only and `async`, because hashing is deliberately slow.
- **`Password.unusableHash`** is a valid hash that no password matches. Verifying against it when no account has the email keeps sign-in timing the same whether or not the email is registered ([26-auth-and-sessions.md](26-auth-and-sessions.md)).
- `hash` may also be a `Secret<String>`, for a hash held in configuration.
- The plain-text password is a `String` only for the duration of the request. Logging a field named `password`, or one whose type is `Secret<…>`, is a compile-time warning.

## Why

- A constant-time comparison is only useful if it is the obvious way. A separate `Mac` type makes it the only way, enforced by the type checker.
- Async digests match the platform API they are built on, instead of hiding a blocking call or a thread pool in the runtime.
- A UUID literal form that is checked at compile time keeps fixtures and seed data readable without a failable call.
