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

- **Field constraints are annotations:** `@Email`, `@Length`, `@Range`, `@NotBlank`, `@Matches("^[a-z0-9-]+$")`, and so on. **Cross-field rules** go in a `validate(rules:)` function.
- **Typed values validate themselves.** A field of type `EmailAddress` ([31-email.md](31-email.md)) binds to a `textField` as text and is validated as an email address, so it needs no `@Email`, and the form's `value` holds the parsed address. `@Email` on an `EmailAddress` field is a warning, with a quick fix that removes it.
- **The same rules run on the client and on the server.**
- **`@Form` wraps a struct with per-field state:** `errors`, `isTouched`, `isValid`, `isSubmitting`, `value`. Its initial value and `value` are described [below](#the-forms-initial-value-and-its-value).
- **Binding a form field to an input wires up error display automatically:** the error message, `aria-invalid` and `aria-describedby`. Forms are accessible by default.
- **The server can't be bypassed.** A `Validatable` RPC parameter is validated before the handler runs. An invalid request fails with `rpcFailure(.invalid(ValidationError))`, a `ValidationError` carrying field paths and messages ([28-rpc-wire-format.md](28-rpc-wire-format.md#failures-outside-the-handler)).
- **`form.handle(action)`** validates first, sets `isSubmitting`, and prevents double submits. Its action either doesn't throw or throws only `ValidationError`. **A `ValidationError` the action throws is shown under the fields it names,** which is how server-side failures reach the form: the action passes them on with `catch .rpcFailure(.invalid(let failure)) { throw failure }`. The handler `handle` returns doesn't throw, as every event handler must ([04-components.md](04-components.md#handlers)).
- **Server errors map back to fields automatically,** because both sides use the same struct and the same field paths. A server-side `@Check` that fails ("email already taken") appears under the email field.
- **Async rules live in services:** `@Check(UserService.emailAvailable)`. On the client the check runs debounced on blur. On the server it always runs in full.
- **A unique constraint is the last line of defence.** Two sign-ups can both pass `@Check` before either is stored, so the insert itself reports a duplicate as a typed `UniqueViolation` ([19-persistence.md](19-persistence.md#writes)), which the server maps to its own error case (`emailTaken` above).
- Custom messages use `message:` on any constraint. Default messages come from the stdlib.

### Typed fields and `@Format`

- **A typed field's parse failure has its own message,** set with `@Format(message:)`. Without it, the stdlib's default message for the type is used:

  ```k
  @Format(message: "Enter a valid email address") var email: EmailAddress
  ```

- `@Format` applies only to fields whose type is parsed from text (`EmailAddress`, `Url`, `Money`, `Int`, `Uuid`, and the other types with a `parse` ([20-standard-library.md](20-standard-library.md#literals-and-parsing))). On a `String` field it is a compile error, because a string can't fail to parse.
- Parsing runs first. A field that doesn't parse shows only its `@Format` message; its other constraints run once it parses.

### `@Matches`

- **`@Matches` takes a string literal regular expression,** which the compiler checks: an invalid pattern is a compile error at the annotation. Passing anything other than a literal is a compile error too, so every pattern is known at compile time.

### Configs

The same constraints apply to `config` fields, and are checked at startup ([25-config-and-secrets.md](25-config-and-secrets.md#config-declarations)). On a `Secret<T>` field they check the revealed value, and the failure message names the variable and the constraint but never includes the value.

## The form's initial value and its value

```k
page Checkout {
    @Param let slug: String

    @Form var order: OrderRequest = .empty(productSlug: slug, quantity: 1)
}
```

- **`.empty` is generated for every `Validatable` struct used with `@Form`.** It is valid only as a `@Form` field's initial value. `@Form var signup: SignupRequest = .empty` starts every field blank.
- **A form keeps the raw input of each field,** not a half-built struct. Text fields hold the text as typed. Typed fields (`EmailAddress`, `Url`, `Money`, `Int`, …) start blank and are parsed when the form validates, so a half-typed address is a normal state of the form, not an invalid value.
- **Prefilled fields:** `.empty(productSlug: slug, quantity: 1)` takes optional named arguments for any subset of the struct's fields, typed as the fields are. Fields not named start blank.
- **`form.value` is the parsed struct,** available only inside the action passed to `handle` (`submit` in the sign-up example), which runs after validation passes. Reading `value` anywhere else is a compile error, because outside `handle` the fields may not parse.
- **A struct's memberwise initializer may omit only fields that have a default value.** A field declared with a default (`var quantity: Int = 1`, `var tags: List<String> = []`, `var note: String? = nil`) takes it when omitted. Omitting a field without a default is a compile error: `OrderRequest(productSlug: slug)` doesn't compile while `email` has no default. A partly filled struct never exists as a value; partial input exists only inside a form ([08-types.md](08-types.md#structs-and-classes)).
