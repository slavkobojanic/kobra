/// The sign-in form for staff.
public struct SignInRequest implements Codable, Validatable {
    /// The email address of the account.
    @Email public var email: String

    /// The password, in plain text. It is only ever compared with
    /// `AdminConfig.passwordHash` and never stored or logged.
    @NotBlank public var password: String
}
