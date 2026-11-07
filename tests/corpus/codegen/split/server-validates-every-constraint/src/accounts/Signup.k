import k.validation.Check
import k.validation.Email
import k.validation.Length
import k.validation.Matches
import k.validation.Validatable

/// A sign-up request with one of each constraint the server must enforce.
public struct Signup implements Codable, Validatable {
    /// An address, checked for availability on the server.
    @Email(message: "Enter an email address")
    @Check(Directory.isFree, message: "Taken")
    var contact: String

    /// Lowercase letters only.
    @Matches("^[a-z]+$", message: "Lowercase letters only")
    @Length(min: 2, max: 8)
    var handle: String
}
