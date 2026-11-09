/// The output of [`X25519PrivateKey.agree(with:)`]: input to a key
/// derivation only (`AesGcmKey.derive(from:salt:info:)`,
/// `Hkdf.sha256(from:salt:info:count:)`). Raw Diffie–Hellman output isn't
/// uniformly random and isn't bound to a purpose, so it has no other
/// members, and it isn't [`Codable`], [`Equatable`] or printable.
@Native
public struct SharedSecret {}
