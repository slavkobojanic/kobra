# Bytes and Cryptography

## Decision

`Bytes` is a core type. `k.crypto` provides HMAC, hashing, secure random values, UUIDs and password hashing, and on top of them **authenticated encryption, key derivation, signatures, key agreement, sealing to a public key, JWK/PEM key formats and JWT verification**. **Safety comes from types:** a MAC is a distinct `Mac` type that can't be compared with `==`, so the constant-time comparison is the only one that compiles.

```k
let mac = await Hmac.sha256(key: config.signingSecret, message: "{timestamp}.".utf8 + payload)

guard mac.matches(signature) else {
    throw WebhookError.invalidSignature
}
```

The same approach covers the rest of the layer:

- **One key type per algorithm and role.** An `AesGcmKey` can't verify a signature, an `Ed25519PublicKey` can't decrypt, and an `EcdsaP256PrivateKey` can't be used for key agreement. Each mistake is a type error, not a runtime `InvalidAccessError`.
- **Nonces are generated, never supplied.** `seal` takes no nonce parameter, so a nonce can't be reused by mistake.
- **Secret key material can't leave by accident.** Secret and private keys aren't `Codable`, `Equatable` or printable. They are exported only through an explicit `Exportable<K>`, and the bytes come back as a `Secret<Bytes>`, so the existing `Secret` rules apply ([25-config-and-secrets.md](25-config-and-secrets.md#secrett)).
- **Raw key-agreement output isn't a key.** `agree` returns a `SharedSecret` that can only be passed to a key derivation.
- **Both sides, one behaviour.** Everything below except `Password` and `KeyStore` works in the browser (on Web Crypto) and on the server (on Rust ops). Configured keys stay server-only, because configs do.

```k
// Encrypt in the browser with a key the server never sees.
let sealed: Sealed<PrivateText> = await vaultKey.seal(PrivateText(body: draft.body), context: "note:{id}".utf8)

// Later, in any browser holding the key.
let text = try await vaultKey.open(sealed, context: "note:{id}".utf8)
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
- **`Password` verifies; it doesn't derive keys.** Turning a passphrase into an encryption key in the browser is `AesGcmKey.derive(fromPassphrase:salt:)` ([below](#from-a-passphrase-pbkdf2)), a different job with a different threat model.

## The key types

Every key is a value of a type named after its algorithm and role. None of them share a supertype that operations accept, so passing the wrong kind of key never compiles.

| Type | Algorithm | Role | Secret? | `Codable` | `Equatable` |
|---|---|---|---|---|---|
| `AesGcmKey` | AES-256-GCM | encrypt and decrypt (`seal`, `open`, `wrap`, `unwrap`) | yes | no | no |
| `Ed25519PrivateKey` | Ed25519 | sign | yes | no | no |
| `Ed25519PublicKey` | Ed25519 | verify | no | yes | yes |
| `EcdsaP256PrivateKey` | ECDSA P-256, SHA-256 | sign | yes | no | no |
| `EcdsaP256PublicKey` | ECDSA P-256, SHA-256 | verify | no | yes | yes |
| `RsaPublicKey` | RSASSA-PKCS1-v1_5 and PSS, SHA-256 | verify only, for interop | no | yes | yes |
| `X25519PrivateKey` | X25519 | key agreement, open an `Envelope` | yes | no | no |
| `X25519PublicKey` | X25519 | key agreement, seal an `Envelope` | no | yes | yes |
| `SharedSecret` | output of `agree` | input to a key derivation only | yes | no | no |

- **Generating:** `AesGcmKey.generate()`, `Ed25519PrivateKey.generate()` and so on. Each returns the key and is `async`. A private key has `publicKey`, so there is no separate key-pair type.
- **Secret values print as `AesGcmKey(•••)`,** like `Secret`. Logging one, putting it in an rpc, a `@Load` field, a job payload, an entity or `LocalStorage` is a compile error, because each requires `Codable`.
- **Public keys are ordinary data.** They implement `Codable` (as base64url text of the raw form), `Equatable` and `Hashable`, so they travel in rpcs and are stored in entities. `publicKey.fingerprint` is the SHA-256 of the raw form as hex, for showing to people ("compare these on both devices").
- **P-256 is for signatures only.** Using one key for both signing and agreement is a known cross-protocol risk, and the one protocol an app meets that needs P-256 agreement, Web Push's payload encryption, uses an ephemeral key inside the stdlib's `WebPush` ([45-web-platform.md](45-web-platform.md#push-tier-2)). So there are no public ECDH P-256 types; they can be added for a protocol that needs them, as separate types from the ECDSA ones.
- **There is one AES variant:** 256-bit keys in GCM mode. There is no 128-bit option, no CBC and no CTR. Each one would add a choice that doesn't improve an application.

### Exportable keys

By default **a key can't be exported at all.** A key generated in the browser is a non-extractable `CryptoKey`, so script injection on the page can use the key while the page is open but can't copy it. Exporting is opt-in, and the type records it:

```k
let exportable = await AesGcmKey.generateExportable()   // Exportable<AesGcmKey>
let key = exportable.key                                 // the plain AesGcmKey
let raw = exportable.exportRaw()                         // Secret<Bytes>
```

| Member of `Exportable<K>` | Meaning |
|---|---|
| `K.generateExportable() async -> Exportable<K>` | generates a key that can be exported |
| `.key` | the key itself, for every ordinary operation |
| `.exportRaw() -> Secret<Bytes>` | the raw form: 32 bytes for AES, Ed25519 seeds and X25519; the scalar for P-256 |
| `.exportJwk() -> Secret<String>` | the JWK, with `alg`, `use` and `key_ops` set for the type |
| `.exportPem() -> Secret<String>` | PKCS #8 PEM (private keys only) |

- **Plain keys have no export members.** `vaultKey.exportRaw()` is a compile error ("`AesGcmKey` can't be exported; generate it with `generateExportable()`"). Whether a key can leave is part of its type, never a flag checked at run time.
- **The exported bytes are a `Secret`,** so they can't be logged, compared with `==` or put in an rpc, and `reveal()` stays the greppable point where key material becomes plain bytes.
- **Wrapping requires `Exportable`,** because Web Crypto's `wrapKey` does: `wrappingKey.wrap(exportable)` ([Key wrapping](#key-wrapping)).
- **Public keys are always exportable:** `publicKey.raw`, `publicKey.jwk`, `publicKey.pem` (SPKI).

### Importing keys

The runtime parsers follow the stdlib convention: `Type.parse(form: …) -> Type?`, with the label naming the form ([20-standard-library.md](20-standard-library.md#literals-and-parsing)). Secret forms take a `Secret`, so key material in code can't be a plain string by accident.

| Form | Public keys | Secret and private keys |
|---|---|---|
| raw | `parse(raw: Bytes)` | `parse(raw: Secret<Bytes>)` |
| JWK | `parse(jwk: String)` | `parse(jwk: Secret<String>)` |
| PEM | `parse(pem: String)` (SPKI) | `parse(pem: Secret<String>)` (PKCS #8) |

- **Parsing checks everything it can, synchronously.** It checks lengths, that P-256 points lie on the curve, the RSA modulus size (at least 2048 bits), and a JWK's `kty`, `crv` and `alg` against the type. An X25519 JWK passed to `Ed25519PublicKey.parse(jwk:)` is `nil`. The platform import happens on first use and is cached in the value.
- **An imported secret key is non-exportable,** unless it is imported with `Exportable<K>.parse(…)`.
- **There are no literals for key types.** A key in source code is a key in git, so `Ed25519PublicKey("…")` doesn't exist. Public keys come from config ([below](#keys-in-configuration)), from rpcs or from a JWK set.

### Keys in configuration

Key types implement `ConfigValue`, so a server reads them like any other setting, validated at startup:

```k
public config ExportSigningConfig {
    /// Signs export manifests. A PKCS #8 PEM, a JWK or a base64url seed.
    @Env("EXPORT_SIGNING_KEY") let signingKey: Secret<Ed25519PrivateKey>

    /// Verifies tokens from the billing service, a sibling in the workspace.
    @Env("BILLING_TOKEN_KEY") let billingKey: Ed25519PublicKey
}
```

- **A secret or private key field must be `Secret<K>`.** A bare `Ed25519PrivateKey` field is a compile error with a quick fix that wraps it. That keeps the rule in [25-config-and-secrets.md](25-config-and-secrets.md#where-values-come-from) that only `Secret` fields accept `NAME_FILE`, so a key can arrive as a Docker or Kubernetes secret file, and the startup report never prints it.
- **`Secret<K>` has K's operations** (`sign`, `seal`, `open`, `agree`, `wrap`, `unwrap`), the way `Hmac` accepts a `Secret<String>`. A configured key therefore needs no `reveal()`: `await config.signingKey.sign(manifest)`.
- Public-key fields are plain values. They aren't secret, but they are still server-only, like every config ([25-config-and-secrets.md](25-config-and-secrets.md#injection)).
- **`k keys new ed25519`** (also `x25519`, `p256-ecdsa`, `aes-gcm`) prints a new private key as a JWK for the environment and its public JWK, so nobody needs an external tool to make a key.

## Authenticated encryption: `Sealed<T>`

```k
let key = await AesGcmKey.generate()

let sealed: Sealed<Bytes> = await key.seal(photo.bytes)
let photoBytes = try await key.open(sealed)
```

| Member | Meaning |
|---|---|
| `key.seal(_ value: T, context: Bytes = Bytes()) async -> Sealed<T>` | encrypts `value` with a fresh random nonce. `T` is `Bytes`, encrypted as is, or any `Codable`, encrypted as its JSON. |
| `key.open(_ sealed: Sealed<T>, context: Bytes = Bytes()) async throws OpenError -> T` | checks and decrypts |
| `Sealed<T>` | the ciphertext. It implements `Codable` (as text), `Equatable` and `Hashable`. |
| `sealed.bytes`, `Sealed<T>.parse(bytes:)`, `"{sealed}"`, `Sealed<T>.parse(_ text:)` | the wire forms, for storing outside K |

```k
public enum OpenError implements Error, Equatable {
    case authenticationFailed           // wrong key, wrong context, or the bytes were changed
    case undecodable(DecodingError)     // authentic, but not a T: written by another app version
}
```

- **No nonce parameter.** `seal` draws a 96-bit nonce from the secure random source every time. Random nonces are safe for 2³² seals under one key (NIST SP 800-38D). Apps that seal more than that with one key derive per-purpose keys with HKDF ([below](#hkdf)). The book documents the limit, and `k check` doesn't try to enforce it.
- **`context` is authenticated but not encrypted** (AEAD associated data). Binding a ciphertext to where it belongs, such as `"note:{id}".utf8`, means a server that swaps two notes' ciphertexts produces `.authenticationFailed`, not the wrong note.
- **The type parameter documents the content, and the server can hold one it can't open.** A `Sealed<PrivateText>` field in an entity tells a reader what's inside. Opening it needs the key, which the server doesn't have. Sealed values of different `T` don't convert into each other.
- **Wire format:** a version byte `0x01`, the 12-byte nonce, the ciphertext, then the 16-byte tag. The text form is `k1.` followed by base64url, so a format change can be detected and rejected.
- **Whole values, not streams.** `seal` works on values that fit in memory, like `File` ([42-files.md](42-files.md)). Streaming encryption is an open question.

## Key derivation

### HKDF

| Member | Meaning |
|---|---|
| `AesGcmKey.derive(from: input, salt: Bytes, info: String) async -> AesGcmKey` | an AES key from high-entropy input (HKDF-SHA256, RFC 5869) |
| `Hkdf.sha256(from: input, salt: Bytes, info: String, count: Int) async -> Secret<Bytes>` | raw output, for implementing a protocol that specifies HKDF |

`input` is a `SharedSecret`, a `Secret<Bytes>`, a `Secret<String>` or a `Secret<AesGcmKey>` (whose raw bytes the runtime uses without exposing them). It is never plain `Bytes`, because HKDF input is key material. `info` is required and names the purpose (`"notes/attachments/v1"`), so two derivations from one secret can't collide by accident.

### From a passphrase: PBKDF2

A passphrase is low-entropy, so it needs a deliberately slow derivation. The only one the browser platform provides is PBKDF2.

```k
let salt = PassphraseSalt.generate()                                   // 16 random bytes and the work factor
let wrappingKey = await AesGcmKey.derive(fromPassphrase: typed, salt: salt)
```

| Member | Meaning |
|---|---|
| `PassphraseSalt.generate()` | a fresh salt, with the current work factor: PBKDF2-HMAC-SHA256 at 600,000 iterations (OWASP's 2023 figure) |
| `PassphraseSalt` | `Codable` and `Equatable`. It isn't secret: it is stored next to what it protects. |
| `AesGcmKey.derive(fromPassphrase: String, salt: PassphraseSalt) async -> AesGcmKey` | the derived key, non-exportable |

- **The salt carries its work factor,** so raising the default later doesn't break existing vaults. A salt below 100,000 iterations parses as `nil`, except in `.test` ([Testing](#testing)).
- **How this relates to `Password`:** `Password` (Argon2id) is the server's way to check a sign-in password and is never available in the browser. `derive(fromPassphrase:)` makes a key on whichever side holds the passphrase, usually the browser, and that key never leaves it. They must not share a passphrase. If the sign-in password also unlocked the vault, the server that receives it at sign-in could derive the vault key. The book's end-to-end chapter says so, and the notes example uses a separate vault passphrase.
- **Argon2id in the browser is rejected for now:** Web Crypto doesn't provide it, and K ships no WebAssembly or JavaScript cryptography ([Alternatives](#alternatives-rejected)).

## Key wrapping

Wrapping is sealing a key with another key, so the key can be stored or sent without being exported in the clear.

| Member | Meaning |
|---|---|
| `key.wrap(_ other: Exportable<K>) async -> WrappedKey<K>` | `K` is `AesGcmKey` or any private key type |
| `key.unwrap(_ wrapped: WrappedKey<K>) async throws OpenError -> K` | the key, non-exportable |
| `key.unwrapExportable(_ wrapped:) async throws OpenError -> Exportable<K>` | when the key will be wrapped again, for example for a new device |
| `WrappedKey<K>` | `Codable` and `Equatable`. It is AES-GCM over the raw form, with the key type's name as the context, so a wrapped X25519 key can't be unwrapped as an `AesGcmKey`. |

A wrong passphrase therefore shows up as `.authenticationFailed` from `unwrap`, which is how an app tells the person the passphrase was wrong. The app never stores a password verifier for it.

## Signatures

```k
let signature = await config.signingKey.sign(manifest.bytes)          // Ed25519Signature

guard await publicKey.verify(signature, of: manifest.bytes) else {
    throw ImportError.notSignedByUs
}
```

| Member | Meaning |
|---|---|
| `privateKey.sign(_ message: Bytes) async -> Ed25519Signature` (and `EcdsaP256Signature`) | signs. Ed25519 is deterministic; ECDSA uses the platform's randomized nonces. |
| `publicKey.verify(_ signature:, of message: Bytes) async -> Bool` | `false` for any invalid signature, never an error |
| `Ed25519Signature`, `EcdsaP256Signature`, `RsaSignature` | `Codable` (base64url), `bytes`, `parse(bytes:)`. ECDSA signatures are the 64-byte `r‖s` form Web Crypto and JWS use, and `parse(der:)` reads the DER form. |

- **A signature type matches its key type.** Verifying an `Ed25519Signature` with an `EcdsaP256PublicKey` is a type error.
- **`verify` returns `Bool`,** like `mac.matches`, and is written with `guard`. Not verifying is visible in review, because the code that uses the message sits after the `guard`.
- **Ed25519 is the default.** ECDSA P-256 is there for interop (JWT `ES256`, Web Push VAPID, the platforms that only speak P-256), and RSA for verifying tokens from identity providers that still sign with it. K never signs with RSA.

## Key agreement

```k
let shared = await myPrivate.agree(with: theirPublic)                 // SharedSecret
let key = await AesGcmKey.derive(from: shared, salt: sessionSalt, info: "chat/v1")
```

- `X25519PrivateKey.agree(with: X25519PublicKey) async -> SharedSecret`.
- **A `SharedSecret` has no members besides being HKDF input.** Raw Diffie–Hellman output isn't uniformly random and isn't bound to a purpose, so using it as a key directly is the classic mistake. Here it doesn't compile.
- An all-zero X25519 result (a small-order public key) is rejected inside `agree`, which then panics with a typed message. That only happens with a malicious peer key, and `X25519PublicKey.parse` already refuses the known small-order points.

## Sealing to a public key: `Envelope<T>`

Most apps that use key agreement want "encrypt this so only the holder of that private key can read it". `Envelope` is that operation, built as **HPKE base mode** (RFC 9180) with DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and AES-256-GCM, so other HPKE implementations can open it and the RFC's test vectors apply.

| Member | Meaning |
|---|---|
| `recipient.seal(_ value: T, context: Bytes = Bytes()) async -> Envelope<T>` | `recipient: X25519PublicKey`. A fresh ephemeral key is used per envelope. |
| `privateKey.open(_ envelope: Envelope<T>, context:) async throws OpenError -> T` | decrypts |
| `Envelope<T>` | `Codable` (text `k1e.` + base64url), `Equatable` |
| `recipient.wrap(_ key: Exportable<K>) async -> EnvelopedKey<K>`, `privateKey.unwrap(_:) async throws OpenError -> K` | a key sealed to a public key, the way `WrappedKey` seals one to an `AesGcmKey`. A key isn't `Codable`, so `Envelope<AesGcmKey>` doesn't compile and this is the way to share one. |

- An `Envelope` and a `Sealed` are different types, so opening an envelope with an `AesGcmKey` is a type error.
- **It isn't authenticated to a sender.** As with HPKE base mode, anyone can seal to a public key. When the sender matters, the app signs the envelope's bytes as well, and the book shows the pattern.

## Comparing in constant time

- **Types that hold secrets aren't `Equatable`:** `Mac`, `Secret`, `AesGcmKey`, every private key and `SharedSecret`. `==` on them is the existing operator-not-defined error, with a hint naming the constant-time member.
- **`ConstantTime.equals(_ a: Bytes, _ b: Bytes) -> Bool`** covers protocol code holding plain bytes, such as a token's hash read from a database. It takes time that depends only on the lengths, and returns `false` early only when the lengths differ.
- On the server it is a Rust loop with no data-dependent branch. In the browser it is a JavaScript loop of the same shape. A JIT gives no hard guarantee, which is acceptable because the browser compares the user's own data.

## JWS and JWT, for interop

K's own sessions are cookies ([26-auth-and-sessions.md](26-auth-and-sessions.md)), and K services call each other with rpcs ([40-workspaces-and-services.md](40-workspaces-and-services.md)). JWTs are for **talking to systems outside K**: an identity provider's ID token, a webhook that signs with JWS, or an API that wants a signed assertion.

```k
public struct BillingClaims implements Codable {
    let sub: String
    let plan: String
}

let claims: BillingClaims = try await Jwt.verify(
    token,
    key: config.billingKey,                         // Ed25519PublicKey, so only EdDSA is accepted
    issuer: "https://billing.internal",
    audience: "notes",
    now: clock.now,
)
```

| Member | Meaning |
|---|---|
| `Jwt.verify(_ token: String, key:, issuer: String, audience: String, now: Date, leeway: Duration = 1.minutes) async throws JwtError -> Claims` | checks the signature, `exp` (required), `nbf`, `iss` and `aud`, then decodes the payload as `Claims` |
| `Jwt.sign(_ claims: Claims, key:, keyId: String? = nil, expiresIn: Duration, now: Date) async -> String` | issues a compact JWT with `iat` and `exp` set |
| `Jws.verify(_ compact: String, key:) async throws JwtError -> Bytes`, `Jws.sign(_ payload: Bytes, key:)` | the signature layer alone, for JWS that isn't a JWT |
| `JwkSet.parse(_ json: String) -> JwkSet?` | a JWKS document, such as an identity provider's `jwks_uri`. It keeps only public signature keys K supports. |

**The algorithm comes from the key's type, never from the token:**

| `key:` | Accepted `alg` |
|---|---|
| `Ed25519PublicKey`, `Secret<Ed25519PrivateKey>` to sign | `EdDSA` |
| `EcdsaP256PublicKey`, `Secret<EcdsaP256PrivateKey>` | `ES256` |
| `RsaPublicKey` (verify only) | `RS256`, `PS256` |
| `Secret<String>` or `Secret<Bytes>` | `HS256` |
| `JwkSet` (verify only) | the key whose `kid` matches the header, at that key's own algorithm |

- **`alg: none` is never accepted,** and a header `alg` that doesn't match the key's type is `.wrongAlgorithm`. The known attack, an RS256 public key reused as an HS256 secret, can't happen: a `JwkSet` holds no symmetric keys, and HMAC needs a `Secret`.
- **`issuer`, `audience` and `now` are required.** `now` follows the clock rule for plain functions ([35-time.md](35-time.md)), so tests control expiry with `TestClock`.

```k
public enum JwtError implements Error, Equatable {
    case malformed
    case wrongAlgorithm(String)
    case unknownKeyId(String?)
    case badSignature
    case expired
    case notYetValid
    case wrongIssuer
    case wrongAudience
    case missingExpiry
    case claims(DecodingError)
}
```

- Fetching and caching a JWKS is the app's own `HttpClient` call ([32-http-client.md](32-http-client.md)). A built-in caching `JwksClient` waits for OAuth providers ([26-auth-and-sessions.md](26-auth-and-sessions.md)), which are deferred.
- **JWE (encrypted JWTs) isn't provided.** It is rarely needed for interop and has a history of dangerous algorithm choices. `Envelope` covers encrypting to a recipient.

## Keeping keys in the browser: `KeyStore`

A browser key has to survive a reload without being written down as bytes. `KeyStore` keeps keys in IndexedDB as structured-cloned `CryptoKey`s, which keeps them non-extractable.

```k
@Platform(side: .client)
public interface KeyStore {
    func get<K>(_ name: KeyName<K>) async throws KeyStoreError -> K?

    func put<K>(_ key: K, as name: KeyName<K>) async throws KeyStoreError

    func delete<K>(_ name: KeyName<K>) async throws KeyStoreError

    @UserAction
    func requestPersistence() async -> StoragePersistence
}

public enum KeyStoreError implements Error, Equatable {
    case unavailable                    // IndexedDB is blocked, such as in some private modes
}

public enum StoragePersistence implements Equatable {
    case persistent                     // the browser won't evict the store
    case bestEffort                     // it may, under storage pressure
}
```

- **`KeyName<K>(name: "vault")` takes a string literal,** like `StorageKey` ([36-local-storage.md](36-local-storage.md#keys)), and fixes the key type, so `get` returns the type that `put` stored. Names join `storage.lock`, so renaming one is a visible change.
- `K` is a secret or private key type. A public key belongs in `LocalStorage` or on the server.
- **Client-only,** like `Files`: a service that injects it is client-only ([10-services.md](10-services.md#serverclient-boundary)). The one implementation is `BrowserKeyStore`. Server code holds keys in configuration, or wrapped in the database.
- **Eviction is the real risk.** A browser under storage pressure may delete IndexedDB, and a lost device key can't be recovered. `requestPersistence()` asks for persistent storage (`navigator.storage.persist()`), which Chromium and WebKit grant or refuse silently and Firefox asks the user about. Because it can prompt, it follows the rule in [45-web-platform.md](45-web-platform.md#permissions) that only a `request…` member prompts, and it is `@UserAction` (the "Turn on the vault" click). It asks for the origin's whole storage, not only the key store, and moves to `k.storage` when IndexedDB collections are designed. The answer is typed, and apps using `KeyStore` for anything unrecoverable are expected to keep a recovery path, such as a passphrase-wrapped copy on the server, as in the example below.
- Signing out doesn't clear the store by itself. The app calls `delete`, as the example does.

## Example: private notes in the notes app

The notes example gets an optional **private vault.** A private note's text is encrypted in the browser. The server stores and syncs it but can't read it, and neither can a database dump or an operator.

```k
// src/vault/PrivateText.k

/// The part of a private note that only the account's browsers can read.
public struct PrivateText implements Codable {
    var body: String
}
```

```k
// src/vault/VaultProtocol.k

public protocol VaultProtocol requires Authenticated {
    /// The account's vault, or `nil` before one is set up.
    rpc get() throws VaultError -> StoredVault?

    /// Stores a new vault. The server sees only the salt and the wrapped key.
    rpc create(vault: StoredVault) throws VaultError
}

public struct StoredVault implements Codable, Equatable {
    let salt: PassphraseSalt
    let wrappedKey: WrappedKey<AesGcmKey>
}
```

```k
// src/vault/VaultState.k and src/vault/VaultError.k

public enum VaultState {
    case unknown                        // restore() hasn't finished
    case locked
    case unlocked(AesGcmKey)            // never Codable, so never sent or saved
}

public enum VaultError implements Error, Equatable {
    case noVault
    case wrongPassphrase
    case locked
    case tampered                       // the server returned a ciphertext that isn't this note's
    case rpcFailure(RpcFailure)         // also the vault rpcs' error enum
}
```

```k
// src/vault/VaultService.k

/// Unlocks the vault in this browser and seals and opens private notes.
///
/// Client-only, because it injects `KeyStore`. The vault key is generated
/// here, never exported in the clear, and never sent to the server: its type
/// isn't `Codable`, so no rpc could carry it.
public service VaultService {
    @Inject let api: VaultProtocol
    @Inject let keys: KeyStore

    private static let vaultKey = KeyName<AesGcmKey>(name: "notes-vault")

    @State var state: VaultState = .unknown

    /// Restores the key this browser kept, if any. The app layout calls it
    /// from an effect, because `init()` can't await.
    func restore() async {
        let kept = try await keys.get(VaultService.vaultKey) else { nil }
        if let kept {
            state = .unlocked(kept)
        } else {
            state = .locked
        }
    }

    /// Sets up a vault the first time, with a passphrase that isn't the
    /// sign-in password.
    func create(passphrase: String) async throws VaultError {
        let salt = PassphraseSalt.generate()
        let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: salt)
        let vault = await AesGcmKey.generateExportable()

        try await api.create(vault: StoredVault(salt: salt, wrappedKey: await wrapping.wrap(vault)))
        await remember(vault.key)
    }

    /// Unlocks on a new browser.
    func unlock(passphrase: String) async throws VaultError {
        guard let stored = try await api.get() else {
            throw VaultError.noVault
        }

        let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: stored.salt)

        let key = try await wrapping.unwrap(stored.wrappedKey) else {
            throw VaultError.wrongPassphrase
        }
        await remember(key)
    }

    func seal(_ text: PrivateText, note id: Uuid) async throws VaultError -> Sealed<PrivateText> {
        let key = try unlockedKey()
        return await key.seal(text, context: "note:{id}".utf8)
    }

    func open(_ sealed: Sealed<PrivateText>, note id: Uuid) async throws VaultError -> PrivateText {
        let key = try unlockedKey()
        do {
            return try await key.open(sealed, context: "note:{id}".utf8)
        } catch .authenticationFailed, .undecodable {
            throw VaultError.tampered
        }
    }

    /// Forgets the key in this browser, for example at sign-out.
    func lock() async {
        state = .locked
        try await keys.delete(VaultService.vaultKey) else { return }
    }

    private func unlockedKey() throws VaultError -> AesGcmKey {
        return match state {
            case .unlocked(let key) { key }
            case .locked, .unknown { throw VaultError.locked }
        }
    }

    private func remember(_ key: AesGcmKey) async {
        do {
            try await keys.put(key, as: VaultService.vaultKey)
        } catch .unavailable {
            // Keep it for this tab only; the person unlocks again next visit.
        }
        state = .unlocked(key)
    }
}
```

- **The entity holds `Sealed<PrivateText>?`** next to the plain title, so the list still shows titles and search keeps working on titles and tags. The note's text is the only private part, and the book says so plainly.
- **The context binds each ciphertext to its note id,** so a server that moves ciphertexts between notes produces `VaultError.tampered`, not a wrong note.
- **What the types guarantee:** `AesGcmKey` isn't `Codable`, so no rpc, `@Load` field or log can carry the vault key. `vault.key` is non-exportable once `create` returns, and the only exportable handle is a local value that goes out of scope. Opening needs the key, which exists only on the client. None of these needs a review checklist.
- **What it doesn't protect against:** the server serves the JavaScript, so a malicious server could ship code that reads the passphrase. That is true of every web end-to-end design, and the book's chapter says so.
- **Sharing a private note** with another account uses that account's `X25519PublicKey`, published through an rpc: the note gets its own `AesGcmKey`, and `recipient.wrap(noteKey)` gives the `EnvelopedKey<AesGcmKey>` stored for that account. It is in the book as an extension exercise, not in the example's first cut.

## Which side

| API | Server | Browser | Notes |
|---|---|---|---|
| `Hmac`, `Hash`, `SecureRandom`, `Uuid`, `ConstantTime` | yes | yes | |
| `Password` | yes | no | Argon2id, deliberately slow |
| key types, `Sealed`, `Envelope`, `WrappedKey`, signatures, HKDF, PBKDF2, `Jwt`, `Jws`, `JwkSet` | yes | yes | Web Crypto in the browser, Rust ops on the server |
| `Secret<K>` keys from a `config` | yes | no | configs are server-only ([25-config-and-secrets.md](25-config-and-secrets.md#injection)) |
| `KeyStore` | no | yes | IndexedDB, client-only platform interface |

No API here asks the user for permission except `KeyStore.requestPersistence()`, whose answer is the typed `StoragePersistence`. No operation reaches the network.

## Implementation

- **Browser:** Web Crypto (`crypto.subtle`), which provides AES-GCM, HKDF, PBKDF2, ECDSA on P-256, RSA verification, Ed25519 and X25519 in the browsers K targets. HPKE and JWS are composed from those calls in the stdlib's own JavaScript. No cryptographic primitive is implemented in JavaScript.
- **Server:** the server is plain `deno_core` with K's own ops, not Deno's Web Crypto extension. The new primitives are Rust ops next to `op_k_hmac` and the Argon2 ops. They use `ring` for AES-GCM, HKDF, PBKDF2, Ed25519, ECDSA P-256 and RSA verification. `ring` is already compiled in as rustls's crypto provider, so this adds no new code to the binary. Static X25519 agreement keys use `x25519-dalek`, from the RustCrypto family `sha2` and `argon2` already come from, because `ring`'s agreement keys are ephemeral only. Web Push's ephemeral P-256 ECDH and AES-128-GCM ([45](45-web-platform.md#push-tier-2)) are `ring` calls inside `WebPush`, not public API, like coturn's HMAC-SHA1 ([46](46-media-and-rtc.md#stun-and-turn-without-third-party-accounts)).
- **The `k test` containers use the Rust ops for both sides.** The browser container there has no Web Crypto ([21-testing.md](21-testing.md#which-side-a-test-runs-on)). The stdlib's crypto module has two backends behind one internal interface, `subtle` and `ops`, and a differential test ties them together (below).

## Testing

Cryptography is computation, so the real algorithms run in `k test`. What tests need is determinism, speed and a double for the one piece of I/O.

- **Deterministic randomness.** In `.test`, `SecureRandom`, nonces, salts and `generate()` draw from a ChaCha20 stream seeded from the test's full name. Every run of a test makes the same keys and the same `Sealed` text, so failures reproduce and a snapshot of a sealed value is stable. This extends the existing rule for `Uuid.random()` ([21-testing.md](21-testing.md#running-tests)). The seeded generator exists only in the test runtime, so a production build can't contain it.
- **Fast passphrases.** In `.test`, `PassphraseSalt.generate()` records 1,000 iterations instead of 600,000, and such salts parse only in `.test`. A test that reads a salt from production is unaffected, because the salt carries its own count.
- **`MemoryKeyStore`** (from `k.test`) is the hermetic default for `KeyStore` ([21-testing.md](21-testing.md#test-doubles-and-defaults)):

  | Member | Meaning |
  |---|---|
  | `names: List<String>` | the names stored now |
  | `failAll(error: KeyStoreError)` | every following call throws `error` |
  | `persistence: StoragePersistence` | what `requestPersistence()` answers; `.persistent` unless set |

- **`TestKeys`** (from `k.test`) gives fixture keys by number, such as `TestKeys.ed25519(1)` and `TestKeys.x25519(2)`, the same in every test. They can be bound into configs: `use ExportSigningConfig(signingKey: Secret(TestKeys.ed25519(1)), billingKey: TestKeys.ed25519(2).publicKey) for ExportSigningConfig`.

```k
test VaultService {
    @Inject let vault: VaultService
    @Inject let keys: MemoryKeyStore

    test "a wrong passphrase doesn't unlock" {
        try await vault.create(passphrase: "correct horse")
        await vault.lock()

        expect throws VaultError.wrongPassphrase {
            try await vault.unlock(passphrase: "battery staple")
        }
        expect keys.names == []
    }

    test "the server can't move a ciphertext to another note" {
        try await vault.create(passphrase: "correct horse")
        let sealed = try await vault.seal(PrivateText(body: "hi"), note: Uuid("00000000-0000-4000-8000-000000000001"))

        expect throws VaultError.tampered {
            try await vault.open(sealed, note: Uuid("00000000-0000-4000-8000-000000000002"))
        }
    }
}
```

**The stdlib's own tests** check correctness:

- Published vectors for each primitive on the `ops` backend: NIST CAVP for AES-GCM, RFC 5869 for HKDF, RFC 8032 for Ed25519, RFC 7748 for X25519, RFC 9180 for HPKE, RFC 7515 and 8037 for JWS, and Project Wycheproof's malformed and edge cases for every algorithm it covers.
- The same vectors through the `subtle` backend in a real browser, in the planned real-browser suite ([docs/compiler/testing.md §8](../compiler/testing.md#8-end-to-end-tests): Chromium driven over the DevTools protocol, skipped where no browser is installed, with Firefox over WebDriver BiDi after it), plus a differential test where each side opens and verifies what the other sealed and signed, on thousands of random inputs.
- Golden files for the `Sealed`, `Envelope` and `WrappedKey` wire formats, so a format change is a reviewed diff.
- Corpus tests for the type rules: `==` on secret types, `Codable` positions, export on non-`Exportable` keys, mismatched signature and key types, bare private keys in configs, and `SharedSecret` used as a key.

## Decided while implementing

**Round 1: the crypto core.** The key types, `Sealed`, HKDF and PBKDF2, key wrapping, Ed25519 and ECDSA P-256 signatures, X25519 agreement, `Envelope` and `EnvelopedKey`, `ConstantTime`, raw import and export, `Secret<K>`'s forwarded operations, both backends and the test support. JWK and PEM (`crypto-key-formats`, with `RsaPublicKey` and `k keys new`), key types as `ConfigValue`s (`crypto-config-keys`), JWS and JWT (`jwt`), `KeyStore` and `MemoryKeyStore` (`key-store`), the notes vault (`notes-vault`) and `WebPush` (`web-push`) are the next round, marked **Planned** in the book under those feature keys.

**Runtime values** (`crates/k_runtime/js/runtime/crypto.js`)

- **A secret or private key is an opaque `Key` object** with non-enumerable fields (the raw secret on `ops`, a `CryptoKey` on `subtle`, and the raw public key). It prints and serializes as `AesGcmKey(•••)`. `Exportable<K>` is the same object generated or unwrapped as extractable: whether a key can leave is enforced by the type, and the runtime only refuses to wrap a key that wasn't made exportable.
- **Everything `Codable` is held as its text:** public keys and signatures as base64url of the raw form, `Sealed`, `Envelope`, `WrappedKey` and `EnvelopedKey` as their prefixed text, `PassphraseSalt` as `pbkdf2-sha256$<iterations>$<base64url salt>`. `==`, hashing, `Set`/`Map` keys, codecs and interpolation then need nothing special. Each type's codec (`Sealed$codec`, …) is a native that accepts only well-formed text, with a decoding error at the path otherwise. Base64url is unpadded and canonical (nonzero spare bits are refused), so every value has one text.
- **Every operation is a `@Native` member named `Owner$member`** (codegen.md §16), like `http.js`: the generic path passes the receiver, `$c` for async members and the `Codable` codec of `T` for `seal` and `open`. Properties of these types (`sealed.bytes`, `key.publicKey`, `publicKey.raw`, `exportable.key`) lower to `Owner$name(value)` (`native.rs`, `is_crypto_value`). `unwrap` and `unwrapExportable` get the expected key type's name from the call's result type, so the runtime builds the type the program asked for, never one named in the bytes.
- **`OpenError` and `DecodingError` cases are built by the runtime** as the generated enums build them (`{$: 0}`, `{$: 1, _0: …}`), as `files.js` does for its errors. `DecodingError` became `Equatable`, so `OpenError` can be.

**Wire formats** (golden files in `crates/k_server/tests/crypto/wire.golden`)

- `Sealed`: `0x01 ‖ nonce(12) ‖ ciphertext ‖ tag(16)`, text `k1.` + base64url of all of it. `WrappedKey`: the same layout, text `k1w.`. **The associated data is a format tag byte and then the context:** `0x01 ‖ context` for `Sealed` and `0x02 ‖ UTF-8(key type name)` for `WrappedKey`, so a sealed value can never be unwrapped as a key, whatever context an app chooses.
- **What is wrapped is the raw form, except for P-256:** a wrapped `EcdsaP256PrivateKey` is `d ‖ x ‖ y` (96 bytes), because Web Crypto imports a P-256 private key only with its point (PKCS #8 without the public key is refused by Chrome, and a JWK needs `x` and `y`). Unwrapping checks that the point is the scalar's on `ops`. `exportRaw()` is still the 32-byte scalar.
- `Envelope`: `0x01 ‖ enc(32) ‖ ciphertext ‖ tag`, text `k1e.`, HPKE `info` `"k.crypto.Envelope"` and the context as AAD. `EnvelopedKey`: the same with `info` `"k.crypto.EnvelopedKey"`, the key type's name as AAD, text `k1k.`.
- Only sequence number 0 of an HPKE context is used: one message per envelope.

**Backends**

- **`ops`** is fifteen Rust ops in `k_server`'s `crypto.rs`: `ring` for AES-GCM, HKDF, PBKDF2 (off the worker's thread), Ed25519 and ECDSA P-256, `x25519-dalek` (with `static_secrets`) for X25519, and a constant-time loop for `ConstantTime.equals` and `mac.matches`. `ring` builds a P-256 key pair only from PKCS #8 or from both halves, so the RustCrypto `p256` crate (arithmetic only) checks a raw scalar and computes its public point, and `ring` builds the pair from both halves. `app.js` hands the ops over with `useOps`; the client part of the runtime never names `Deno.core`.
- **`subtle`** is Web Crypto. A raw Ed25519 or X25519 private key is imported as PKCS #8 (a fixed prefix and the 32 bytes), and its public key comes from an extractable copy's JWK before the non-extractable key is imported; a P-256 key is imported as a JWK from its wrapped form. HMAC with an empty key (HPKE's empty salts) uses 32 zero bytes, which HMAC treats the same and Web Crypto accepts. HKDF's expand step for HPKE's labeled expand is composed from HMAC, since Web Crypto's HKDF takes no PRK.
- **Selection:** Web Crypto where `crypto.subtle` exists (browsers), the ops otherwise: the server, and both containers of `k test`, whose `crypto` stand-in has no `subtle`. Members that can't be async (`parse(raw:)` of a secret key, `TestKeys`) use `ops` and panic in a browser; they take a `Secret`, which is server-only, so client code can't reach them.
- `Hash`, `Hmac`, `SecureRandom`, `ConstantTime` and `Mac.matches` moved into `crypto.js` too, so they work in the browser for the first time (`Hash` and `SecureRandom` had no implementation on either side before). `Password` and `Uuid` stay where they were.
- **The browser chunk is deferred:** `crypto.js` is a feature chunk (`FEATURE_CHUNKS`, part `Crypto`, `CRYPTO_SIZE_BUDGET` of 8 KB gzip for all of it; a release page that seals and opens ships about 3.5 KB), imported by the page chunks that use one of its exports, so the shipped runtime doesn't grow.

**Language and checks**

- **`Secret<K>` forwarding is a member-lookup rule** (`secret_key_forwarding` in `k_sema`'s `cx.rs`): a member missing on `Secret<K>` is looked up on `K` when `K` is `AesGcmKey` or a private key type and the name is one of `seal`, `open`, `wrap`, `unwrap`, `unwrapExportable`, `sign`, `agree` or `publicKey`. Never an export member. The runtime's natives accept a key or a `Secret` of one.
- **No new diagnostic codes.** `==` on a secret key or `SharedSecret` is K2013 with a help naming what to compare instead (and `Mac`'s help names `matches`). An export member on a plain key is K1203 with the message "`AesGcmKey` can't be exported; generate it with `generateExportable()`". Keys in a `Codable` type, `Sealed<AesGcmKey>`, mismatched key and signature types and a `SharedSecret` used as a key are the existing conformance, constraint, mismatch and unknown-member errors (corpus `tests/corpus/sema/types/crypto/`). A bare private key in a config is today's non-`ConfigValue` field error; its dedicated message and fix come with `crypto-config-keys`.

**API as built, where it differs from the text above**

- **`context` defaults to `"".utf8`,** not `Bytes()`, which doesn't exist.
- **`publicKey.fingerprint()` is an `async` method,** not a property: Web Crypto's SHA-256 is asynchronous, and K ships no JavaScript SHA-256.
- **`EcdsaP256PublicKey.parse(raw:)` checks the length and the `0x04` prefix only,** synchronously; a point off the curve is refused at `verify`, which returns `false`. Checking the curve equation synchronously would need the curve's arithmetic in JavaScript in the browser.
- **ECDSA signatures aren't deterministic in tests:** the signing nonce comes from the operating system on `ops`, as from the platform in browsers. Everything else `generate()`, `seal` and salts draw is seeded from the test's name, through the kit's existing ChaCha20 stream (`crypto.getRandomValues` in `k test`).
- **`TestKeys` key `n` of a kind** is derived from the SHA-256 of `k.test.TestKeys/<kind>/<n>`. It is a `@Native class` of `k.test` with `aesGcm`, `ed25519`, `ecdsaP256` and `x25519`.
- **`WrappedKey` and `EnvelopedKey` have `parse(_ text:)` but no `bytes`,** and `Exportable<K>.parse` waits for the key formats round. `Sealed` and `Envelope` have both forms.
- **`RsaPublicKey` waits for `crypto-key-formats`:** RSA keys arrive as JWK or PEM, so there was nothing to import it from in this round.
- **A static member of a generic type is called with the type inferred:** `let s: Sealed<Note>? = Sealed.parse(text)`; K has no `Sealed<Note>.parse(…)` syntax.
- **`Exportable.exportRaw()` returns a `Secret<Bytes>`, so it is server-only** (as `Secret` is). In the browser a key leaves only wrapped.

**Tests**

- Rust unit tests of each primitive in `crypto.rs` (NIST/GCM spec AES-256-GCM, RFC 5869, RFC 7914's PBKDF2-SHA256, RFC 8032, RFC 7748, RFC 6979's P-256 key and signature, FIPS 180 digests, edge cases).
- `crypto-vectors.json`, run by `crypto-vectors.js` on `ops` (`crypto.test.js`, `runtime_js`) and on `subtle` in one real-Chrome test (`k_cli` `chrome::crypto_subtle_matches_the_vectors_and_the_ops_backend`): AES-256-GCM, HKDF, Ed25519, X25519, P-256, and RFC 9180 base mode from the CFRG's `test-vectors.json` for DHKEM(X25519, HKDF-SHA256) and HKDF-SHA256 with AES-128-GCM and AES-256-GCM (the RFC's own appendix has no AES-256 vector for X25519; AES-128 is supported by the internal HPKE functions only for that vector). **Wycheproof subset:** AES-GCM with 256-bit keys and 96-bit nonces (every valid and invalid case), X25519 (40 valid cases and 20 low-order ones, whose all-zero result is refused), Ed25519 (every valid and invalid case), ECDSA P-256 SHA-256 in P1363 form (40 valid cases and every invalid one) and HKDF-SHA256 (40). **Left out:** Wycheproof's "acceptable" cases (behaviour the libraries may differ on), other nonce and key sizes K never uses, the DER ECDSA file, PBKDF2 (Wycheproof has none), and the RFC 9180 cases for other modes and suites.
- The same Chrome test runs the differential: 400 random inputs each way, sealed, signed with Ed25519 and ECDSA, and enveloped by one side and opened and verified by the other (Web Crypto against an in-process isolate on `ops`).
- Golden files for the wire formats (`crates/k_server/tests/crypto_wire.rs`), which also open each golden value again; a run corpus case (`tests/corpus/run/stdlib/crypto-key-types.k`) for RFC vectors through the K API; sema corpus cases for the type rules; and `k test` on `crates/k_cli/tests/fixtures/sealing` for seeded randomness, fast salts, `TestKeys`, `OpenError` and `Secret<K>`.

## Why

- A constant-time comparison is only useful if it is the obvious way. A separate `Mac` type makes it the only way, enforced by the type checker. The new types apply the same idea to every misuse the Web Crypto API leaves to runtime checks or documentation: wrong key for the operation, reused nonces, raw DH output used as a key, extractable keys by default, and the algorithm taken from a token.
- Async digests match the platform API they are built on, instead of hiding a blocking call or a thread pool in the runtime.
- A UUID literal form that is checked at compile time keeps fixtures and seed data readable without a failable call.
- **Few algorithms, each with one reason.** AES-256-GCM for symmetric encryption, Ed25519 for signatures, X25519 and HPKE for public-key encryption. P-256 and RSA verification are for interop. Every one of them is native in browsers and in `ring`, so K ships no cryptographic code of its own on either side.
- **Exportability in the type** turns the browser's best defence against key theft by injected script (non-extractable keys) into the default, and makes the exception visible in review.
- Reusing `Secret` for exported material and configured keys means the leak rules, the redaction and the `_FILE` convention already in place cover keys with no new mechanism.

## Alternatives rejected

- **Exposing `crypto.subtle` as it is.** Algorithm names are strings, key usages are arrays and misuse fails at run time. That is the API this layer exists to replace.
- **XChaCha20-Poly1305.** Its 192-bit nonce removes the random-nonce limit, but Web Crypto doesn't have it. The browser would need a JavaScript or WebAssembly implementation, with no constant-time guarantee and a bundle cost, plus a second implementation to keep in step. AES-GCM's 2³² limit, with HKDF-derived subkeys beyond it, is the documented trade.
- **Caller-supplied nonces,** even as an "advanced" overload. One reuse under GCM reveals the authentication key. Protocols that fix their nonces are built inside the stdlib (HPKE is one).
- **Argon2id or scrypt in the browser** through WebAssembly. They are better passphrase KDFs, but K ships no third-party code to the browser ([17-javascript-interop.md](17-javascript-interop.md)). PBKDF2 with a stored work factor can be upgraded once the platform offers more.
- **A generic `Key<Algorithm, Role>`.** It expresses the same rules, but its error messages read worse, and flat names are searchable and match the platform's documentation.
- **An `extractable: Bool` parameter,** as in Web Crypto. Export of a non-extractable key would fail at run time. `Exportable<K>` makes it a compile error.
- **Deno's Web Crypto extension on the server.** It would bring a second JavaScript Web Crypto layer and a larger dependency set into a runtime that otherwise exposes only K's ops. It also wouldn't help the `k test` browser container, which needs the ops anyway.
- **RSA signing and encryption, AES-CBC and CTR, SHA-1.** They are legacy choices. Verifying RSA signatures is the one RSA operation interop still needs.

## Open questions

- **Streaming encryption** of large files (`Stream<Bytes>`), for example as chunked AES-GCM with the STREAM construction, together with streaming in [42-files.md](42-files.md).
- **Key commitment.** AES-GCM isn't key-committing, which matters for messages encrypted to several recipients ("invisible salamanders"). A committing variant could become `Sealed` format version 2.
- **Passkeys and WebAuthn** will reuse `EcdsaP256PublicKey` and `Ed25519PublicKey` for credential public keys, when [26-auth-and-sessions.md](26-auth-and-sessions.md) takes them up.
- **A caching `JwksClient`** for identity providers, with OAuth.
- Resolved while reconciling with 45 and 46: `persist()` became the `@UserAction` `requestPersistence()`, because Firefox prompts for it; the public ECDH P-256 types were cut (their one use, Web Push, is internal), which also drops the `p256` crate; and the browser-side vector and differential tests run in the planned real-browser suite rather than `browser.rs`, which drives a fake DOM.
- **Older browsers without Ed25519 or X25519 in Web Crypto:** K either refuses to run there, with a clear message, or gives these keys no fallback. This follows the browser baseline in [38-deployment.md](38-deployment.md).
