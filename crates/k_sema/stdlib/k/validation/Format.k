/// Sets the message for a typed field whose text doesn't parse:
/// `@Format(message: "Enter a valid email address")`. A compile error on a
/// `String` field.
@Annotation
public struct Format {
    /// The message shown when the text doesn't parse.
    let message: String
}
