test AuthApi {
    // The implementation itself, called directly. That skips the rpc layer,
    // so validation and the email's `@Check` don't run first, which is how a
    // race between two sign-ups looks from inside the handler.
    @Inject let api: AuthApi

    test "a second sign-up with the same email is refused even past validation" {
        try await api.signUp(request: signUpRequest(email: "ada@example.com"))

        expect throws SignUpError.emailTaken {
            try await api.signUp(request: signUpRequest(email: "ADA@example.com"))
        }
    }
}
