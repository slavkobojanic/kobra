import k.validation.Check
import k.validation.Email
import k.validation.Length
import k.validation.Rules
import k.validation.Validatable

/// Everything needed to create an account.
///
/// The same struct backs the sign-up form and the `signUp` rpc, so these
/// rules run in the browser as the person types, and again on the server
/// before `AuthApi` sees the request.
public struct SignUpRequest implements Codable, Validatable {
    /// The new account's email address. The availability check runs on blur
    /// in the browser, and in full on the server.
    @Email(message: "Enter a valid email address")
    @Check(AuthService.emailAvailable, message: "An account with this email already exists")
    public var email: String

    /// The new password. At least 8 characters.
    @Length(min: 8, message: "Use at least 8 characters")
    public var password: String

    /// The password again, to catch typos.
    public var confirmPassword: String

    /// Checks that both password fields match.
    public func validate(rules: Rules) {
        rules.check(confirmPassword == password, field: .confirmPassword, message: "Passwords must match")
    }
}
