/// Why decoding JSON into a `Codable` type failed.
public enum DecodingError implements Error, Codable {
    /// The text isn't JSON.
    case malformed(message: String)
    /// The JSON doesn't have the type's shape.
    case mismatch(path: String, message: String)
}
