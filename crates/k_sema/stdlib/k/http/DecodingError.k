/// Why decoding JSON into a [`Codable`] type failed.
public enum DecodingError implements Error, Codable {
    /// The text isn't JSON; `message` says where parsing stopped.
    case malformed(message: String)
    /// The JSON doesn't have the type's shape: `path` locates the value
    /// that doesn't fit.
    case mismatch(path: String, message: String)
}
