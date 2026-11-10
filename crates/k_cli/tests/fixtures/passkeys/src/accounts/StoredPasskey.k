import k.auth.PasskeyCredential

/// A passkey and the account it signs in.
public struct StoredPasskey implements Equatable, Codable {
    let owner: String
    let credential: PasskeyCredential
}
