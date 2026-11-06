import k.http.DecodingError

/// JSON encoding of `Codable` values.
@Native
public class Json {
    /// `value` as JSON text.
    static func encode<T>(_ value: T) -> String where T implements Codable
    /// `text` decoded into a `Codable` type, given by the binding.
    static func decode<T>(_ text: String) throws DecodingError -> T where T implements Codable
    /// UTF-8 `bytes` decoded into a `Codable` type, given by the binding.
    static func decode<T>(_ bytes: Bytes) throws DecodingError -> T where T implements Codable
}
