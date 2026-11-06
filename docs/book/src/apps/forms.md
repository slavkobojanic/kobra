# Forms and validation

Validation uses **constraint annotations on struct fields**, in the style of Spring's Bean Validation.
**One struct is the single source of truth** for the form, for the RPC request, and for the checks on the
server.

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

            passwordField(value: bind signup.confirmPassword)
                .label("Confirm password")

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

## Constraints

**Field constraints are annotations:** `@Email`, `@Length(min:max:)`, `@Range(…)`, `@NotBlank`,
`@Matches("^[a-z0-9-]+$")` and more, from `k.validation`. Every constraint accepts a custom `message:`;
otherwise the message comes from the standard library.

```k
@NotBlank(message: "Give your note a title")
@Length(max: 200)
var title: String
```

**Typed values validate themselves.** A field of type `EmailAddress` binds to a `textField` as text and
is validated as an email address, so it needs no `@Email`, and the form's `value` holds the parsed
address. `@Email` on an `EmailAddress` field is a warning, with a quick fix that removes it. `@Email` is
for the rare field that must stay a `String`.

**Cross-field rules** go in a `validate(rules:)` function, which reports each problem against a field.

### Typed fields and `@Format`

A typed field's parse failure has its own message, set with `@Format(message:)`. Without it, the
standard library's default message for the type is used:

```k
@Format(message: "Enter a valid email address") var email: EmailAddress
```

- `@Format` applies only to fields whose type is parsed from text: `EmailAddress`, `Url`, `Money`, `Int`,
  `Uuid` and the other types with a `parse`. On a `String` field it is a compile error, because a string
  can't fail to parse.
- **Parsing runs first.** A field that doesn't parse shows only its `@Format` message; its other
  constraints run once it parses.

### `@Matches`

`@Matches` takes a **string literal** regular expression, which the compiler checks: an invalid pattern
is a compile error at the annotation. Passing anything other than a literal is a compile error too, so
every pattern is known at compile time.

```k
@Matches("^[a-z0-9-]+$", message: "Use lowercase letters, digits and dashes")
var slug: String
```

### Configs

The same constraints apply to `config` fields and are checked at startup. On a `Secret<T>` field they
check the revealed value, and the failure message names the variable and the constraint but never
includes the value. See [Configuration and secrets](config.md).

## `@Form`

`@Form` wraps a struct with per-field state. `@Form` fields are always `var`.

| Member | Meaning |
|---|---|
| `form.value` | the parsed struct, ready to send; readable only inside the action passed to `handle` |
| `form.errors` | the current error messages, per field |
| `form.isTouched` | whether the person has interacted with a field |
| `form.isValid` | whether every rule passes |
| `form.isSubmitting` | whether a submission is in flight |
| `form.handle(action)` | a submit handler: validates first, sets `isSubmitting`, prevents double submits, then runs `action` |

**The action passed to `handle` either doesn't throw or throws only `ValidationError`.** A
`ValidationError` it throws is shown under the fields it names. That is how failures from the server
reach the form: the action passes them on with `catch .rpcFailure(.invalid(let failure)) { throw failure
}`, as `submit` does above. The handler `handle` returns doesn't throw, as every event handler must.

### The initial value

```k
page Checkout {
    @Param let slug: String

    @Form var order: OrderRequest = .empty(productSlug: slug, quantity: 1)
}
```

- **`.empty` is generated for every `Validatable` struct used with `@Form`.** It is valid only as a
  `@Form` field's initial value. `@Form var signup: SignupRequest = .empty` starts every field blank.
- **A form keeps the raw input of each field,** not a half-built struct. Text fields hold the text as
  typed. Typed fields (`EmailAddress`, `Url`, `Money`, `Int`, …) start blank and are parsed when the form
  validates, so a half-typed address is a normal state of the form, not an invalid value.
- **Prefilled fields:** `.empty(productSlug: slug, quantity: 1)` takes optional named arguments for any
  subset of the struct's fields, typed as the fields are. Fields not named start blank.
- **`form.value` is the parsed struct,** available only inside the action passed to `handle` (`submit`
  above), which runs after validation passes. Reading `value` anywhere else is a compile error, because
  outside `handle` the fields may not parse.
- **A struct's initializer can't leave fields out.** `OrderRequest(productSlug: slug)` without the other
  fields is a compile error. A partly filled struct never exists as a value; partial input exists only
  inside a form.

## Accessible by default

**Binding a form field to an input wires up error display automatically:** the error message,
`aria-invalid` and `aria-describedby`. A form written the obvious way is already accessible, and
`.label("…")` gives every field a visible, connected label.

## The server can't be bypassed

- **The same rules run on the client and on the server.**
- **A `Validatable` RPC parameter is validated before the handler runs.** An invalid request fails with
  `rpcFailure(.invalid(ValidationError))`, carrying field paths and messages, and the handler never sees
  it. See [Protocols and RPC](protocols.md#failures-outside-the-handler).
- **Server errors map back to fields automatically**, because both sides use the same struct and the same
  field paths.

## Async rules

Some rules need the server, such as "this email isn't registered yet". They live in services and are
referenced with `@Check`:

```k
@Check(UserService.emailAvailable, message: "An account with this email already exists")
var email: EmailAddress
```

In the browser, the check runs debounced when the field loses focus. On the server, it always runs in
full. A server-side `@Check` that fails appears under its field.

## The last line of defence

Two sign-ups can both pass `@Check` before either is stored. So **the database insert itself reports a
duplicate** of a `@Unique` field as a typed `UniqueViolation`, which the server maps to its own error case
(`emailTaken` above):

```k
do {
    try await accounts.insert(account)
} catch .email {
    throw SignUpError.emailTaken
}
```

See [Persistence](persistence.md#writes).
