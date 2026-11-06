# Proposal: Bytes, HMAC and UUIDs (`k.crypto`)

> **Status: proposal.** Not yet accepted into the design. First used by `examples/shop`.

## Problem

[20-standard-library.md](../20-standard-library.md) lists `k.crypto` as "hashing, secure random numbers, UUIDs, password hashing". Password hashing is specified in [auth-and-sessions.md](auth-and-sessions.md). Verifying a webhook signature without a vendor SDK also needs a byte type, HMAC, hex encoding, and a comparison that doesn't leak timing. Entities need UUIDs that can be written as literals, parsed from URLs and generated.

## Proposal

### `Bytes` (core, no import)

`Bytes` is an immutable byte sequence with value semantics.

| Member | Meaning |
|---|---|
| `"text".utf8` | a string's UTF-8 encoding |
| `a + b` | concatenation |
| `bytes.hex` | lowercase hex text |
| `Bytes(hex: text) -> Bytes?` | parses hex (either case); `nil` for odd lengths or other characters |
| `bytes.base64`, `Bytes(base64:)` | Base64, as in `k.encoding` |
| `bytes.utf8Text -> String?` | decodes UTF-8; `nil` if invalid |
| `bytes.count` | the length |
| `a.isEqualInConstantTime(to: b) -> Bool` | compares without an early exit, so timing doesn't reveal how many leading bytes match |

- **`==` on `Bytes` is a compile-time warning when either side comes from `Hmac` or a `Secret`,** with a quick fix to `isEqualInConstantTime(to:)`. Comparing MACs with an early-exit comparison is a classic vulnerability, and the compiler can see where MACs come from.

### `Hmac`

```k
let digest = Hmac.sha256(key: secret.utf8, message: "{timestamp}.".utf8 + payload)
```

- `Hmac.sha256(key:message:) -> Bytes` and `Hmac.sha512(key:message:) -> Bytes`, built on Web Crypto, which both runtimes provide ([13-tooling.md](../13-tooling.md)). They are synchronous: the K runtime runs them on its own thread pool.
- `Hash.sha256(data:) -> Bytes` covers plain digests.
- HMAC is available on both sides of the boundary, since it has browser uses, but its key in practice is a `Secret`, which keeps the calling code on the server ([config-and-secrets.md](config-and-secrets.md)).

### `Uuid`

| Member | Meaning |
|---|---|
| `Uuid("0b6f2c1e-…")` | a literal, validated at compile time, like `Url("…")` in [http-client.md](http-client.md) |
| `Uuid.parse(text) -> Uuid?` | parses a runtime string, in canonical form, either case |
| `Uuid.random()` | a version 4 UUID from a secure random source |
| `"{uuid}"` | canonical lowercase text |

- `Uuid` implements `Codable` (as a string), `Hashable` and `ConfigValue`, and **works as a `@Param` type**: an invalid id in a URL is a `404`, as [12-routing.md](../12-routing.md) requires of typed parameters.
- **Random UUIDs are unguessable** (122 random bits), so a URL containing one can act as a capability link, as the shop's receipt page does.

### Secure random

`SecureRandom.bytes(count:) -> Bytes` and `SecureRandom.token(bytes:) -> String` (URL-safe Base64) for tokens. There is no non-secure random source in `k.crypto`.

## Why

- A constant-time comparison is only useful if it is the obvious way. Putting it on `Bytes`, and warning on `==` for MACs, makes the safe choice the default.
- A UUID literal form that is checked at compile time keeps fixtures and seed data readable without a failable call.
