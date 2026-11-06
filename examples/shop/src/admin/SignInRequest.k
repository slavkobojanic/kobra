import k.text.EmailAddress
import k.validation.NotBlank
import k.validation.Validatable

/// The sign-in form for staff.
public struct SignInRequest implements Codable, Validatable {
    /// The email address of the account. Its type validates it.
    var email: EmailAddress

    /// The password, in plain text. It is only ever compared with
    /// `AdminConfig.passwordHash` and never stored or logged.
    @NotBlank var password: String
}
