/// Validation failures with field paths and messages. Thrown by a `@Form`'s
/// action to show server-side failures under their fields, and carried by
/// `RpcFailure.invalid`.
public struct ValidationError implements Error, Codable, Equatable {
    /// The failures, in field order.
    let failures: List<FieldError>
}
