/// The name a key is kept under in a [`KeyStore`], paired with its type:
/// `KeyName<AesGcmKey>(name: "notes-vault")`. The name is a string
/// literal, declared once per app, and fixes the key type, so `get`
/// returns the type `put` stored.
@Native
public struct KeyName<K> {
    /// Creates the name `name`.
    ///
    /// - Parameter name: a string literal, unique in the app.
    @Native
    @Literal
    init(name: String) {}

    /// The name.
    var name: String
}
