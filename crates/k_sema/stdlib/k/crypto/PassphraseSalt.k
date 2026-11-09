/// The salt and work factor of a passphrase-derived key
/// (`AesGcmKey.derive(fromPassphrase:salt:)`). It isn't secret: store it
/// next to what the key protects. It carries its work factor, so raising
/// the default never breaks existing vaults.
@Native
public struct PassphraseSalt implements Codable, Equatable {
    /// The PBKDF2-HMAC-SHA256 iterations this salt records.
    var iterations: Int

    /// A fresh salt: 16 random bytes at the current work factor,
    /// 600,000 iterations (1,000 in `k test`).
    ///
    /// - Returns: the salt.
    static func generate() -> PassphraseSalt
}
