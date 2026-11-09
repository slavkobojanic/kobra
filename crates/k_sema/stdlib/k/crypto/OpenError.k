import k.http.DecodingError

/// Why opening a [`Sealed`], an [`Envelope`] or a wrapped key failed.
public enum OpenError implements Error, Equatable {
    /// The wrong key, the wrong context, or the bytes were changed.
    case authenticationFailed
    /// Authentic, but not a value of the type: written by another version
    /// of the app.
    case undecodable(DecodingError)
}
