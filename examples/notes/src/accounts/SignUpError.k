import k.auth.GuardRejection

/// Why creating an account failed after its request passed validation.
public enum SignUpError implements Error {
    /// Another account registered the same email between validation and
    /// saving. The `@Check` on `SignUpRequest.email` catches the common case
    /// earlier, under the email field.
    case emailTaken

    /// The rate limit refused the call.
    case rejected(GuardRejection)
}
