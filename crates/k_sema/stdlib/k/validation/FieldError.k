/// One validation failure: the path of the field and the message, an
/// element of [`ValidationError.failures`] and [`Form.errors`].
public struct FieldError implements Equatable, Codable {
    /// The field path, such as `"email"` or `"address.city"`.
    let path: String
    /// The message shown under the field.
    let message: String
}
