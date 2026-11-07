/// Validation failures with field paths and messages. Thrown by a `@Form`'s
/// action ([`Form.handle`]) to show server-side failures under their
/// fields, and carried by [`k.rpc.RpcFailure.invalid`].
public struct ValidationError implements Error, Codable, Equatable {
    /// The failures ([`FieldError`]), in field order.
    let failures: List<FieldError>
}
