# Bytes and cryptography

`Bytes` is a core type, and `k.crypto` provides HMAC, hashing, secure random values, UUIDs and password
hashing, and on top of them **authenticated encryption, key derivation, key wrapping, signatures, key
agreement, sealing to a public key, JWK and PEM key formats, keys in configuration, JWS and JWT, and a
browser key store**. **Safety comes from types:** a MAC is a distinct `Mac` type that
can't be compared with `==`, so the constant-time comparison is the only one that compiles, and every key
is a type of its own, so the wrong key for an operation, a reused nonce or raw key-agreement output used as
a key doesn't compile.

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

## Keys

Every key is a value of a type named after its algorithm and role. None of them share a supertype that
operations accept, so passing the wrong kind of key never compiles.

| Type | Algorithm | Role | `Codable`, `Equatable` |
|---|---|---|---|
| `AesGcmKey` | AES-256-GCM | `seal`, `open`, `wrap`, `unwrap` | no: secret |
| `Ed25519PrivateKey`, `Ed25519PublicKey` | Ed25519 | sign, verify | the public key only |
| `EcdsaP256PrivateKey`, `EcdsaP256PublicKey` | ECDSA P-256 with SHA-256 | sign, verify, for interop | the public key only |
| `X25519PrivateKey`, `X25519PublicKey` | X25519 | key agreement, `Envelope` | the public key only |
| `RsaPublicKey` | RSASSA-PKCS1-v1_5 and PSS with SHA-256 | verify only, for interop | yes |
| `SharedSecret` | the output of `agree` | input to a key derivation only | no: secret |

- **Generating:** `AesGcmKey.generate()`, `Ed25519PrivateKey.generate()` and so on, each `async`. A private
  key has `publicKey`, so there is no key-pair type.
- **Secret keys are opaque.** They aren't `Codable`, `Equatable` or printable: `"{key}"` is
  `AesGcmKey(•••)`, and putting one in an rpc, a `@Load` field, a job payload, an entity or `LocalStorage`
  is a compile error, because each requires `Codable`. `==` on one is an error whose help names what to
  compare instead.
- **Public keys are ordinary data:** `Codable` as base64url text of the raw form, `Equatable` and
  `Hashable`, so they travel in rpcs and are stored in entities. `publicKey.raw` is the raw form,
  `Ed25519PublicKey.parse(raw:)` reads it (`nil` for a wrong length; an X25519 key that is a known
  small-order point is refused too), and `await publicKey.fingerprint()` is the SHA-256 of the raw form as
  hex, for people to compare on two devices.
- **Exporting is opt-in, and the type records it.** By default a key can't be exported at all: in the
  browser it is a non-extractable Web Crypto key, which script injected into the page can use while the
  page is open but can't copy. `generateExportable()` returns an `Exportable<K>`, whose `key` is the
  plain key and whose `exportRaw()` returns a `Secret<Bytes>` (so on the server). A plain key has no
  export member:

```k,fails=K1203
~import k.crypto.AesGcmKey
~
~func backUp(vault: AesGcmKey) -> Secret<Bytes> {
return vault.exportRaw()        // `AesGcmKey` can't be exported; generate it with `generateExportable()`
~}
```

- Server code imports a raw secret key with `AesGcmKey.parse(raw:)` and the private keys' `parse(raw:)`,
  which take a `Secret<Bytes>`, so key material can't be a plain value by accident. JWK and PEM come
  next.

## Key formats: JWK and PEM

Keys travel between systems as JWKs (RFC 7517) and PEM files. Every key type reads them, and the parsers
follow the stdlib's `Type.parse(form: …) -> Type?` convention, with the label naming the form:

| Form | Public keys | Secret and private keys |
|---|---|---|
| raw | `parse(raw: Bytes)` | `parse(raw: Secret<Bytes>)` |
| JWK | `parse(jwk: String)` | `parse(jwk: Secret<String>)` |
| PEM | `parse(pem: String)`, a `PUBLIC KEY` (SPKI) | `parse(pem: Secret<String>)`, a `PRIVATE KEY` (PKCS #8) |

- **Parsing checks what it can, at once.** A JWK's `kty` and `crv` must be the type's, and so must its
  `alg`, `use` and `key_ops` when it has them, so an X25519 JWK passed to `Ed25519PublicKey.parse(jwk:)`
  is `nil`. A JWK with a private part (`d`) isn't a public key. A private JWK or PEM that carries a public
  key must carry the key's own. An RSA key needs 2,048 to 8,192 bits and an odd exponent of at least 3.
- **Public keys export as they are:** `publicKey.jwk` and `publicKey.pem`.
- **A secret key exports only from an `Exportable`:** `exportJwk()` and `exportPem()` return a
  `Secret<String>`, like `exportRaw()`. `Exportable.parse(raw:)`, `(jwk:)` and `(pem:)` import a key as
  exportable, with the key type taken from the expected type, since a static member of a generic type is
  called with its type inferred:

```k
~import k.crypto.Ed25519PrivateKey
~import k.crypto.Exportable
~
/// Moves a signing key to a new server: the JWK of a key from the old one's PEM.
func migrate(pem: Secret<String>) -> Secret<String>? {
    let key: Exportable<Ed25519PrivateKey>? = Exportable.parse(pem: pem)
    return key?.exportJwk()
}
```

- **There are no literals for key types.** A key in source code is a key in git. Public keys come from
  configuration, from rpcs or from a JWK set.
- **`RsaPublicKey` verifies only:** `verify(_:of:)` for RSASSA-PKCS1-v1_5 (JWT `RS256`) and
  `verifyPss(_:of:)` for RSASSA-PSS with a 32-byte salt (`PS256`), of an `RsaSignature`. It exists for
  identity providers that still sign with RSA; K never signs with RSA.

## Authenticated encryption: `Sealed`

```k
// src/vault/PrivateText.k

/// The part of a private note that only the account's browsers can read.
public struct PrivateText implements Codable, Equatable {
    var body: String
}
```

```k
~import k.crypto.AesGcmKey
~import k.crypto.OpenError
~import k.crypto.Sealed
~
func roundTrip(key: AesGcmKey, id: Int, draft: String) async throws OpenError -> PrivateText {
    let sealed: Sealed<PrivateText> = await key.seal(PrivateText(body: draft), context: "note:{id}".utf8)

    return try await key.open(sealed, context: "note:{id}".utf8)
}
```

| Member | Meaning |
|---|---|
| `key.seal(_ value: T, context: Bytes) async -> Sealed<T>` | encrypts `value` with a fresh random nonce. `T` is `Bytes`, encrypted as is, or any `Codable`, encrypted as its JSON. `context` may be left out. |
| `key.open(_ sealed: Sealed<T>, context: Bytes) async throws OpenError -> T` | checks and decrypts |
| `Sealed<T>` | the ciphertext: `Codable` (as text), `Equatable` and `Hashable` |
| `sealed.bytes`, `Sealed.parse(bytes:)`, `"{sealed}"`, `Sealed.parse(_:)` | the wire forms, for storing outside K |

`OpenError` has two cases: `.authenticationFailed` for the wrong key, the wrong context or changed bytes,
and `.undecodable(DecodingError)` for a value that is authentic but not a `T`, written by another version
of the app.

- **There is no nonce parameter.** `seal` draws a 96-bit nonce from the secure random source every time.
  Random nonces are safe for 2³² seals under one key (NIST SP 800-38D); an app that seals more than that
  with one key derives per-purpose keys with HKDF (below). The compiler doesn't count seals.
- **`context` is authenticated but not encrypted.** Binding a ciphertext to where it belongs, such as
  `"note:{id}".utf8`, means a server that swaps two notes' ciphertexts produces `.authenticationFailed`,
  not the wrong note.
- **The type parameter documents the content,** and sealed values of different `T` don't convert into
  each other. A server can store a `Sealed<PrivateText>` it has no key to open.
- **Wire format:** a version byte `0x01`, the 12-byte nonce, the ciphertext and the 16-byte tag. The text
  form is `k1.` followed by base64url, so a format change can be detected and rejected.
- **Whole values, not streams:** `seal` works on values that fit in memory.

## Deriving keys

`AesGcmKey.derive(from:salt:info:)` derives a key from high-entropy input with HKDF-SHA256 (RFC 5869). The
input is a `SharedSecret`, a `Secret<Bytes>`, a `Secret<String>` or a `Secret<AesGcmKey>`, never plain
`Bytes`, because HKDF input is key material. `info` is required and names the purpose
(`"notes/attachments/v1"`), so two derivations from one secret can't collide. `Hkdf.sha256(from:salt:info:count:)`
gives raw output, as a `Secret<Bytes>`, for implementing a protocol that specifies HKDF.

A passphrase is low-entropy, so it needs a deliberately slow derivation, PBKDF2-HMAC-SHA256, the only one
browsers provide:

```k
~import k.crypto.AesGcmKey
~import k.crypto.PassphraseSalt
~
~func unlockKey(typed: String) async -> AesGcmKey {
let salt = PassphraseSalt.generate()                                   // 16 random bytes and the work factor
let wrappingKey = await AesGcmKey.derive(fromPassphrase: typed, salt: salt)
~    return wrappingKey
~}
```

- **A `PassphraseSalt` carries its work factor**, 600,000 iterations today (OWASP's 2023 figure), so
  raising the default later doesn't break existing vaults. It is `Codable` and `Equatable`, and isn't
  secret: store it next to what it protects. A salt below 100,000 iterations doesn't decode, except in
  tests.
- **This isn't `Password`.** `Password` (Argon2id) is how the server checks a sign-in password and is never
  available in the browser; `derive(fromPassphrase:)` makes a key on whichever side holds the passphrase,
  usually the browser. They must not share a passphrase: if the sign-in password also unlocked a vault, the
  server that receives it at sign-in could derive the vault key.

## Wrapping keys

Wrapping seals a key with another key, so the key can be stored or sent without being exported in the
clear. Only an `Exportable<K>` can be wrapped.

```k
~import k.crypto.AesGcmKey
~import k.crypto.OpenError
~import k.crypto.PassphraseSalt
~import k.crypto.WrappedKey
~
/// A vault as the server stores it: it can't read the key inside.
public struct StoredVault implements Codable, Equatable {
    let salt: PassphraseSalt
    let wrappedKey: WrappedKey<AesGcmKey>
}

func createVault(passphrase: String) async -> StoredVault {
    let salt = PassphraseSalt.generate()
    let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: salt)
    let vault = await AesGcmKey.generateExportable()

    return StoredVault(salt: salt, wrappedKey: await wrapping.wrap(vault))
}

func unlockVault(_ stored: StoredVault, passphrase: String) async throws OpenError -> AesGcmKey {
    let wrapping = await AesGcmKey.derive(fromPassphrase: passphrase, salt: stored.salt)

    return try await wrapping.unwrap(stored.wrappedKey)
}
```

- `key.wrap(_:)` takes an `Exportable<K>` of `AesGcmKey` or a private key type; `key.unwrap(_:)` returns
  the key, non-exportable, and `key.unwrapExportable(_:)` an exportable one, for wrapping it again for a
  new device.
- **The key type's name is authenticated,** so a wrapped X25519 key can't be unwrapped as an `AesGcmKey`.
  A `WrappedKey<K>` is `Codable` (text `k1w.` and base64url) and `Equatable`.
- **A wrong passphrase is `.authenticationFailed` from `unwrap`,** which is how an app tells the person
  their passphrase was wrong. The app never stores a password verifier for it.

## Signatures

```k
~import k.crypto.Ed25519PrivateKey
~import k.crypto.Ed25519PublicKey
~import k.crypto.Ed25519Signature
~
/// An export manifest with its signature.
public struct SignedManifest implements Codable {
    let body: Bytes
    let signature: Ed25519Signature
}

func sign(_ body: Bytes, with key: Secret<Ed25519PrivateKey>) async -> SignedManifest {
    return SignedManifest(body: body, signature: await key.sign(body))
}

func accept(_ manifest: SignedManifest, from signer: Ed25519PublicKey) async -> Bytes? {
    guard await signer.verify(manifest.signature, of: manifest.body) else {
        return nil
    }
    return manifest.body
}
```

- `privateKey.sign(_:)` returns an `Ed25519Signature` or an `EcdsaP256Signature`. Ed25519 is deterministic;
  ECDSA uses the platform's random nonces.
- **`verify(_:of:)` returns `Bool`,** `false` for any invalid signature, never an error, and is written
  with `guard`, so code that uses the message sits after the check.
- **A signature type matches its key type:** verifying an `Ed25519Signature` with an
  `EcdsaP256PublicKey` is a type error.
- Signatures are `Codable` (base64url), with `bytes` and `parse(bytes:)`. ECDSA signatures are the 64-byte
  `r‖s` form Web Crypto and JWS use, and `EcdsaP256Signature.parse(der:)` reads the DER form.
- **Ed25519 is the default.** ECDSA P-256 is for interop with platforms that only speak P-256.

## Key agreement and `Envelope`

```k
~import k.crypto.AesGcmKey
~import k.crypto.X25519PrivateKey
~import k.crypto.X25519PublicKey
~
~func chatKey(mine: X25519PrivateKey, theirs: X25519PublicKey, sessionSalt: Bytes) async -> AesGcmKey {
let shared = await mine.agree(with: theirs)                 // SharedSecret
let key = await AesGcmKey.derive(from: shared, salt: sessionSalt, info: "chat/v1")
~    return key
~}
```

**A `SharedSecret` has no members besides being HKDF input.** Raw Diffie–Hellman output isn't uniformly
random and isn't bound to a purpose, so using it as a key directly is the classic mistake, and here it
doesn't compile.

Most apps that use key agreement want "encrypt this so only the holder of that private key can read it".
That is `Envelope`, built as HPKE base mode (RFC 9180) with DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and
AES-256-GCM, so other HPKE implementations can open it:

```k
~import k.crypto.Envelope
~import k.crypto.OpenError
~import k.crypto.X25519PrivateKey
~import k.crypto.X25519PublicKey
~
~func share(recipient: X25519PublicKey, mine: X25519PrivateKey, id: Int) async throws OpenError -> String {
let envelope: Envelope<String> = await recipient.seal("the note", context: "share:{id}".utf8)
let text = try await mine.open(envelope, context: "share:{id}".utf8)
~    return text
~}
```

- A fresh ephemeral key is used for every envelope. An `Envelope<T>` is `Codable` (text `k1e.` and
  base64url) and `Equatable`, and an envelope and a `Sealed` are different types.
- **A key isn't `Codable`, so `Envelope<AesGcmKey>` doesn't compile.** `recipient.wrap(_:)` seals an
  `Exportable<K>` to a public key as an `EnvelopedKey<K>`, and `privateKey.unwrap(_:)` opens it.
- **It isn't authenticated to a sender.** Anyone can seal to a public key; when the sender matters, sign
  the envelope's bytes as well.

## Comparing in constant time

Types that hold secrets aren't `Equatable`: `Mac`, `Secret`, `AesGcmKey`, every private key and
`SharedSecret`. `ConstantTime.equals(_:_:)` compares plain `Bytes`, such as a token's hash read from a
database, in time that depends only on their lengths.

## Keys in configuration

Key types are `ConfigValue`s, so a server reads them from the environment like any other setting, checked
at startup. The value is a JWK, a PEM, or base64url of the raw key:

```k
~import k.crypto.Ed25519PrivateKey
~import k.crypto.Ed25519PublicKey
~
public config ExportSigningConfig {
    /// Signs export manifests. A PKCS #8 PEM, a JWK or a base64url seed.
    @Env("EXPORT_SIGNING_KEY") let signingKey: Secret<Ed25519PrivateKey>

    /// Verifies tokens from the billing service.
    @Env("BILLING_TOKEN_KEY") let billingKey: Ed25519PublicKey
}
```

- **A secret or private key field is a `Secret<K>`.** Then the key may come from `NAME_FILE`, such as a
  Docker or Kubernetes secret file, and the startup report never prints it. A bare one is K4032, whose
  quick fix wraps the type:

```k,fails=K4032
~import k.crypto.Ed25519PrivateKey
~
config SigningConfig {
    @Env("SIGNING_KEY") let signingKey: Ed25519PrivateKey     // must be `Secret<Ed25519PrivateKey>`
}
```

- **A `Secret<K>` of a key type has the key's operations** (`seal`, `open`, `wrap`, `unwrap`, `sign`,
  `agree`, `publicKey`), the way `Hmac` accepts a `Secret<String>`, so a configured key needs no
  `reveal()`: `await config.signingKey.sign(manifest)`. It forwards no export member.
- Public-key fields are plain values. They aren't secret, but like every config they are server-only.
- **`k keys new ed25519`** (also `x25519`, `p256-ecdsa` and `aes-gcm`) prints a new private key as a JWK
  for the environment, and its public JWK, so nobody needs another tool to make a key.
- In tests, bind keys with `use`: `use ExportSigningConfig(signingKey: Secret(TestKeys.ed25519(1)),
  billingKey: TestKeys.ed25519(1).publicKey) for ExportSigningConfig`.

## JWS and JWT, for interop

K's own sessions are cookies, and K services call each other with rpcs. JWTs are for **talking to systems
outside K**: an identity provider's ID token, a webhook signed with JWS, or an API that wants a signed
assertion.

```k
~import k.crypto.Ed25519PublicKey
~import k.crypto.Jwt
~import k.crypto.JwtError
~
public struct BillingClaims implements Codable {
    let iss: String
    let aud: String
    let sub: String
    let plan: String
}

func claims(of token: String, key: Ed25519PublicKey, now: Date) async throws JwtError -> BillingClaims {
    return try await Jwt.verify(token, key: key, issuer: "https://billing.internal", audience: "notes", now: now)
}
```

| Member | Meaning |
|---|---|
| `Jwt.verify(_:key:issuer:audience:now:leeway:)` | checks the signature, `exp` (required), `nbf`, `iss` and `aud`, with a minute's leeway unless given, then decodes the payload as the expected `Codable` type |
| `Jwt.sign(_:key:keyId:expiresIn:now:)` | a compact JWT of the claims, with `iat` and `exp` set |
| `Jws.verify(_:key:)`, `Jws.sign(_:key:)` | the signature layer alone, for JWS that isn't a JWT: the payload is `Bytes` |
| `JwkSet.parse(_:)` | a JWKS document, such as an identity provider's `jwks_uri`; it keeps only the public signature keys K supports |

**The algorithm comes from the key's type, never from the token:**

| `key:` | Accepted `alg` |
|---|---|
| `Ed25519PublicKey`, and to sign an `Ed25519PrivateKey` or a `Secret` of one | `EdDSA` |
| `EcdsaP256PublicKey`, and to sign an `EcdsaP256PrivateKey` or a `Secret` of one | `ES256` |
| `RsaPublicKey` (verify only) | `RS256`, `PS256` |
| `Secret<String>` or `Secret<Bytes>` of at least 32 bytes | `HS256` |
| `JwkSet` (verify only) | the key whose `kid` matches the header, at that key's own algorithm |

- **`alg: none` is never accepted,** and a header `alg` the key's type doesn't verify is
  `.wrongAlgorithm`. The classic attack, an RS256 public key reused as an HS256 secret, can't happen: a
  `JwkSet` holds no symmetric keys, and HMAC needs a `Secret`.
- **`issuer`, `audience` and `now` are required.** `now` comes from the injected `Clock`, so tests control
  expiry with `TestClock`. Since `Jwt.sign` adds only `iat` and `exp`, the claims type carries `iss` and
  `aud` itself.
- The errors are `JwtError`: `.malformed`, `.wrongAlgorithm(alg)`, `.unknownKeyId(kid)`, `.badSignature`,
  `.expired`, `.notYetValid`, `.wrongIssuer`, `.wrongAudience`, `.missingExpiry` and `.claims(DecodingError)`.
- Fetching and caching a JWKS is the app's own `HttpClient` call. JWE (encrypted JWTs) isn't provided;
  `Envelope` encrypts to a recipient.

## Keeping keys in the browser: `KeyStore`

A browser key has to survive a reload without being written down as bytes. `KeyStore` keeps keys in the
browser's IndexedDB as non-extractable Web Crypto keys:

```k
~import k.crypto.AesGcmKey
~import k.crypto.KeyName
~import k.crypto.KeyStore
~import k.crypto.KeyStoreError
~
/// Keeps one device key in this browser.
public service DeviceKeys {
    @Inject let keys: KeyStore

    private static let deviceKey: KeyName<AesGcmKey> = KeyName(name: "device")

    /// The kept key, or a new one, kept for next time.
    func key() async throws KeyStoreError -> AesGcmKey {
        if let kept = try await keys.get(DeviceKeys.deviceKey) {
            return kept
        }
        let fresh = await AesGcmKey.generate()
        try await keys.put(fresh, as: DeviceKeys.deviceKey)
        return fresh
    }
}
```

- **`KeyName(name:)` takes a string literal** and fixes the key type, so `get` returns the type `put`
  stored. `put` of an exportable key stores it non-exportable.
- **It is client-only,** like `Files`: a service that injects it is client-only. `BrowserKeyStore` is the
  implementation, namespaced per app. Server code keeps keys in configuration, or wrapped in the database.
- `KeyStoreError.unavailable` means IndexedDB is blocked, as in some private modes. An app can keep a key
  for the tab only then, and ask for the passphrase again next visit.
- **Eviction is the real risk.** A browser under storage pressure may delete the store, and a lost key
  can't be recovered. `requestPersistence()` asks the browser to keep it and answers a
  `StoragePersistence`, `.persistent` or `.bestEffort`; it is `@UserAction`, because Firefox asks the
  person. Keep a recovery path, such as a passphrase-wrapped copy on the server, as the notes example
  does.
- Signing out doesn't clear the store by itself: the app calls `delete`.

## A worked example: the notes vault

The [notes example](../examples/notes.md#the-private-vault) puts these together. A private note's text is
sealed in the browser with a vault key, which is wrapped with a key derived from a passphrase (never the
sign-in password) and stored on the server, and kept in this browser's `KeyStore`. The server stores and
syncs the sealed text but can't read it, and each ciphertext is bound to its note's id, so a server that
moves one to another note produces an error, not the wrong note.

## Testing

Cryptography is computation, so the real algorithms run in `k test`, on the server's implementation for
both sides.

- **Randomness repeats.** In a test, `SecureRandom`, nonces, salts and `generate()` draw from a ChaCha20
  stream seeded from the test's full name, so every run of a test makes the same keys and the same `Sealed`
  text, and a failure reproduces.
- **Passphrases are fast.** In a test, `PassphraseSalt.generate()` records 1,000 iterations instead of
  600,000, and such salts decode only in tests.
- **`TestKeys`** (from `k.test`) gives fixture keys by number, the same in every test:
  `TestKeys.ed25519(1)`, `TestKeys.x25519(2)`, `TestKeys.ecdsaP256(1)`, `TestKeys.aesGcm(1)`.
- **`MemoryKeyStore`** (from `k.test`) replaces `KeyStore`: keys in memory, empty for every test. `names`
  lists what is stored, `failAll(error:)` makes every call throw, and `setPersistence(_:)` sets what
  `requestPersistence()` answers (`.persistent` unless set).

```k
~import k.crypto.OpenError
~import k.crypto.Sealed
~import k.test.TestKeys
~
test PrivateText {
    test "a sealed note opens with its own id only" {
        let key = TestKeys.aesGcm(1)
        let sealed: Sealed<PrivateText> = await key.seal(PrivateText(body: "hi"), context: "note:1".utf8)

        expect throws OpenError.authenticationFailed {
            try await key.open(sealed, context: "note:2".utf8)
        }
    }
}
```

## Which side

Everything in this chapter except `Password` works in the browser, on Web Crypto, and on the server, on
the runtime's own implementation, except that `KeyStore` is the browser's only. The code that uses a
`Secret` stays on the server, because a `Secret` does: that includes configured keys, `parse` of a
secret key, and export. The browser loads the cryptography code only on pages that use it. Ed25519 and X25519 need a browser
whose Web Crypto has them, as current Chrome, Firefox and Safari do.

## Push messages

`WebPush`, which sends Web Push notifications signed with the app's own VAPID key (an
`EcdsaP256PrivateKey` from `k keys new p256-ecdsa`) and encrypted to each browser, is in
[Notifications](notifications.md#push-messages-from-the-server). Its encryption is internal: `k.crypto` adds no
API for it.
