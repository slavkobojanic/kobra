import k.http.DecodingError

/// JSON encoding and decoding of [`Codable`] values.
@Native
public class Json {
    /// Encodes `value` as JSON text.
    ///
    /// - Parameter value: the value to encode.
    /// - Returns: compact JSON text.
    static func encode<T>(_ value: T) -> String where T implements Codable
    /// Decodes `text` into a [`Codable`] type, written out
    /// (`Json.decode<Board>(text)`) or given by the binding.
    ///
    /// - Parameter text: JSON text.
    /// - Returns: the decoded value.
    /// - Throws: [`DecodingError.malformed`] when `text` isn't JSON, and
    ///   [`DecodingError.mismatch`] when it doesn't have the type's shape.
    static func decode<T>(_ text: String) throws DecodingError -> T where T implements Codable
    /// Decodes UTF-8 `bytes` into a [`Codable`] type, written out or given
    /// by the binding.
    ///
    /// - Parameter bytes: UTF-8 encoded JSON.
    /// - Returns: the decoded value.
    /// - Throws: [`DecodingError.malformed`] when `bytes` aren't JSON, and
    ///   [`DecodingError.mismatch`] when they don't have the type's shape.
    static func decode<T>(_ bytes: Bytes) throws DecodingError -> T where T implements Codable
}
