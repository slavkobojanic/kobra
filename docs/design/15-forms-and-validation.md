# Forms and Validation

## Decision

Validation uses **constraint annotations on struct fields**, in the style of Spring's Bean Validation. **One struct is the single source of truth** for the form, the RPC request and the server-side checks.

```k
struct SignupRequest implements Codable, Validatable {
    @Email var email: String
    @Length(min: 8) var password: String
    var confirmPassword: String
    @Range(13...120) var age: Int

    func validate(rules: Rules) {
        rules.check(confirmPassword == password, field: .confirmPassword, message: "Passwords must match")
    }
}

component Signup {
    @Inject let auth: AuthService
    @Form var signup: SignupRequest = .empty

    func submit() async throws AuthError {
        try await auth.signup(request: signup.value)
    }

    view {
        form {
            textField(value: bind signup.email)
                .label("Email")

            passwordField(value: bind signup.password)
                .label("Password")

            numberField(value: bind signup.age)
                .label("Age")

            button { "Create account" }
                .type(.submit)
                .disabled(signup.isSubmitting)
        }
        .on(submit: signup.handle(submit))
    }
}
```

## Rules

- **Field constraints are annotations:** `@Email`, `@Length`, `@Range`, `@NotBlank`, `@Matches(pattern)`, and so on. **Cross-field rules** go in a `validate(rules:)` function.
- **The same rules run on the client and on the server.**
- **`@Form` wraps a struct with per-field state:** `errors`, `isTouched`, `isValid`, `isSubmitting`, `value`.
- **Binding a form field to an input wires up error display automatically:** the error message, `aria-invalid` and `aria-describedby`. Forms are accessible by default.
- **The server can't be bypassed.** A `Validatable` RPC parameter is validated before the handler runs. An invalid request returns a typed `ValidationError` with field paths.
- **Server errors map back to fields automatically,** because both sides use the same struct. An "email already taken" error appears under the email field without extra code.
- **`form.handle(action)`** validates first, sets `isSubmitting`, and prevents double submits.
- **Async rules live in services:** `@Check(UserService.emailAvailable)`. On the client the check runs debounced on blur. On the server it always runs in full.
- Custom messages use `message:` on any constraint. Default messages come from the stdlib.
