import k.crypto.AesGcmKey

/// What this browser knows about the account's vault.
public enum VaultState {
    /// `restore()` hasn't finished.
    case unknown

    /// This browser holds no vault key.
    case locked

    /// The vault key, never `Codable`, so never sent or saved.
    case unlocked(AesGcmKey)
}
