import k.validation.Validatable

/// The text the compose form sends.
public struct Draft implements Codable, Validatable {
    var text: String
}
