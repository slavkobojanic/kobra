# Forms and Validation

## Decision

Validation uses **constraint annotations on struct fields**, in the style of Spring's Bean Validation. **One struct is the single source of truth** for the form, the RPC request and the server-side checks.

```k
struct SignupRequest implements Codable, Validatable {
    var email: EmailAddress
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
    @State var problem: String? = nil

    func submit() async throws ValidationError {
        do {
            try await auth.signup(request: signup.value)

            Router.navigate(to: .welcome)
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .emailTaken {
            problem = "That email is already registered."
        } catch .rpcFailure {
            problem = "We couldn't create your account. Try again."
        }
    }

    view {
        form {
            textField(value: bind signup.email)
                .label("Email")

            passwordField(value: bind signup.password)
                .label("Password")

            numberField(value: bind signup.age)
                .label("Age")

            if let problem {
                paragraph { problem }
                    .role(.alert)
            }

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
- **Typed values validate themselves.** A field of type `EmailAddress` ([31-email.md](31-email.md)) binds to a `textField` as text and is validated as an email address, so it needs no `@Email`, and the form's `value` holds the parsed address.
- **The same rules run on the client and on the server.**
- **`@Form` wraps a struct with per-field state:** `errors`, `isTouched`, `isValid`, `isSubmitting`, `value`.
- **Binding a form field to an input wires up error display automatically:** the error message, `aria-invalid` and `aria-describedby`. Forms are accessible by default.
- **The server can't be bypassed.** A `Validatable` RPC parameter is validated before the handler runs. An invalid request fails with `rpcFailure(.invalid(ValidationError))`, a `ValidationError` carrying field paths and messages ([28-rpc-wire-format.md](28-rpc-wire-format.md#failures-outside-the-handler)).
- **`form.handle(action)`** validates first, sets `isSubmitting`, and prevents double submits. Its action either doesn't throw or throws only `ValidationError`. **A `ValidationError` the action throws is shown under the fields it names,** which is how server-side failures reach the form: the action passes them on with `catch .rpcFailure(.invalid(let failure)) { throw failure }`. The handler `handle` returns doesn't throw, as every event handler must ([04-components.md](04-components.md#handlers)).
- **Server errors map back to fields automatically,** because both sides use the same struct and the same field paths. A server-side `@Check` that fails ("email already taken") appears under the email field.
- **Async rules live in services:** `@Check(UserService.emailAvailable)`. On the client the check runs debounced on blur. On the server it always runs in full.
- **A unique constraint is the last line of defence.** Two sign-ups can both pass `@Check` before either is stored, so the insert itself reports a duplicate as a typed `UniqueViolation` ([19-persistence.md](19-persistence.md#writes)), which the server maps to its own error case (`emailTaken` above).
- Custom messages use `message:` on any constraint. Default messages come from the stdlib.
