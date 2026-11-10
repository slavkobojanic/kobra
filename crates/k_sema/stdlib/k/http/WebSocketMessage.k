/// One WebSocket message: text or binary.
public enum WebSocketMessage implements Equatable {
    /// A text message.
    case text(String)
    /// A binary message.
    case binary(Bytes)

    /// Decodes the message, text or UTF-8 bytes, as JSON into a
    /// [`Codable`] type, written out or given by the binding:
    /// `let tick = try message.json<Tick>()`.
    ///
    /// - Returns: the decoded value.
    /// - Throws: [`DecodingError.malformed`] when it isn't JSON, and
    ///   [`DecodingError.mismatch`] when it doesn't have the type's shape.
    @Native
    func json<T>() throws DecodingError -> T where T implements Codable
}
