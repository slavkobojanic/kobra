/// One validation failure: the path of the field and the message.
public struct FieldError implements Equatable, Codable {
    /// The field path, such as `"email"` or `"address.city"`.
    let path: String
    /// The message shown under the field.
    let message: String
}
