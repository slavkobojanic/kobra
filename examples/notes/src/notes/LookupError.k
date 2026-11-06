import k.auth.GuardRejection

/// Why a call about one existing note failed.
public enum LookupError implements Error {
    /// There is no note with this id, or it belongs to another account. The
    /// two look identical on purpose, so ids can't be probed.
    case notFound

    /// The `Authenticated` guard refused the call: nobody is signed in.
    case rejected(GuardRejection)
}
