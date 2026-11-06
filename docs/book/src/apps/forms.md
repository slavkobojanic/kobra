# Forms and validation

Validation uses **constraint annotations on struct fields**, in the style of Spring's Bean Validation.
**One struct is the single source of truth** for the form, for the RPC request, and for the checks on the
server.

```k
struct SignupRequest implements Codable, Validatable {
    @Email var email: String
    @Length(min: 8) var password: String
    var confirmPassword: String
    @Range(13...120) var age: Int

    static let empty = SignupRequest(email: "", password: "", confirmPassword: "", age: 18)

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

            passwordField(value: bind signup.confirmPassword)
                .label("Confirm password")

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

## Constraints

**Field constraints are annotations:** `@Email`, `@Length(min:max:)`, `@Range(…)`, `@NotBlank`,
`@Matches(pattern)` and more, from `k.validation`. Every constraint accepts a custom `message:`;
otherwise the message comes from the standard library.

```k
@Email(message: "Enter a valid email address")
@Length(max: 200)
var email: String
```

**Cross-field rules** go in a `validate(rules:)` function, which reports each problem against a field.

## `@Form`

`@Form` wraps a struct with per-field state. `@Form` fields are always `var`.

| Member | Meaning |
|---|---|
| `form.value` | the struct itself, ready to send |
| `form.errors` | the current error messages, per field |
| `form.isTouched` | whether the person has interacted with a field |
| `form.isValid` | whether every rule passes |
| `form.isSubmitting` | whether a submission is in flight |
| `form.handle(action)` | a submit handler: validates first, sets `isSubmitting`, prevents double submits, then runs `action` |

## Accessible by default

**Binding a form field to an input wires up error display automatically:** the error message,
`aria-invalid` and `aria-describedby`. A form written the obvious way is already accessible, and
`.label("…")` gives every field a visible, connected label.

## The server can't be bypassed

- **The same rules run on the client and on the server.**
- **A `Validatable` RPC parameter is validated before the handler runs.** An invalid request gets a typed
  `ValidationError` with field paths, and the handler never sees it.
- **Server errors map back to fields automatically**, because both sides use the same struct. An "email
  already taken" error appears under the email field without extra code.

## Async rules

Some rules need the server, such as "this email isn't registered yet". They live in services and are
referenced with `@Check`:

```k
@Email
@Check(UserService.emailAvailable, message: "An account with this email already exists")
var email: String
```

In the browser, the check runs debounced when the field loses focus. On the server, it always runs in
full.
