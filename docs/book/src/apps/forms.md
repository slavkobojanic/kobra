# Forms and validation

Validation uses **constraint annotations on struct fields**, in the style of Spring's Bean Validation.
**One struct is the single source of truth** for the form, for the RPC request, and for the checks on the
server.

```k
~import k.text.EmailAddress
~import k.validation.Form
~import k.validation.Length
~import k.validation.Range
~import k.ui.Router
~import k.validation.Rules
~import k.validation.Validatable
~import k.validation.ValidationError
~import k.rpc.RpcFailure
~
~enum SignupError implements Error {
~    case emailTaken
~    case rpcFailure(RpcFailure)
~}
~
~service AuthService {
~    func signup(request: SignupRequest) async throws SignupError {
~        throw .emailTaken
~    }
~}
~
~page Welcome {
~    view {
~        text { "Welcome" }
~    }
~}
~
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
~import k.validation.Length
~import k.validation.NotBlank
~
~import k.validation.Validatable
~
~struct NoteDraft implements Codable, Validatable {
@NotBlank(message: "Give your note a title")
@Length(max: 200)
var title: String
~}
```

**Typed values validate themselves.** A field of type `EmailAddress` binds to a `textField` as text and
is validated as an email address, so it needs no `@Email`, and the form's `value` holds the parsed
address. `@Email` on an `EmailAddress` field is a warning (`K7004`), with a quick fix that removes it. `@Email` is
for the rare field that must stay a `String`.

**Cross-field rules** go in a `validate(rules:)` function, which reports each problem against a field.

### Typed fields and `@Format`

A typed field's parse failure has its own message, set with `@Format(message:)`. Without it, the
standard library's default message for the type is used:

```k
~import k.text.EmailAddress
~import k.validation.Format
~
~import k.validation.Validatable
~
~struct Contact implements Codable, Validatable {
@Format(message: "Enter a valid email address") var email: EmailAddress
~}
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
~import k.validation.Matches
~
~import k.validation.Validatable
~
~struct Product implements Codable, Validatable {
@Matches("^[a-z0-9-]+$", message: "Use lowercase letters, digits and dashes")
var slug: String
~}
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
| `form.reset()` | puts every field back to its initial value and clears the errors and `isTouched` |

**The action passed to `handle` either doesn't throw or throws only `ValidationError`.** A
`ValidationError` it throws is shown under the fields it names. That is how failures from the server
reach the form: the action passes them on with `catch .rpcFailure(.invalid(let failure)) { throw failure
}`, as `submit` does above. The handler `handle` returns doesn't throw, as every event handler must.

### The initial value

```k
~import k.validation.Form
~import k.validation.Validatable
~
~struct OrderRequest implements Codable, Validatable {
~    var productSlug: String
~    var quantity: Int
~}
~
page Checkout {
    @Param let slug: String

    @Form var order: OrderRequest = .empty(productSlug: slug, quantity: 1)
~
~    view {
~        text { "{order.isSubmitting}" }
~    }
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
- **`form.reset()` starts the form over:** every field goes back to the text `.empty(…)` gave it (blank
  for a field it doesn't name), the errors and the touched flags are cleared, and a pending
  [async rule](#async-rules) is cancelled. A form that adds one item after another calls it in the action,
  after `value` has been used, so the fields are empty for the next entry.
- **A struct's initializer can't leave fields out.** `OrderRequest(productSlug: slug)` without the other
  fields is a compile error. A partly filled struct never exists as a value; partial input exists only
  inside a form.

## Accessible by default

**Binding a form field to an input wires up error display automatically:** the error message,
`aria-invalid` and `aria-describedby`. A form written the obvious way is already accessible, and
`.label("…")` gives every field a visible, connected label.

**A submit that fails puts the focus in the first invalid field,** in the form's field order, whether the
browser's validation refused it or the action threw a `ValidationError`, so a keyboard or screen reader
user lands on the field to fix and hears its error.

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
~import k.validation.Check
~import k.text.EmailAddress
~
~import k.rpc.RpcOnlyError
~import k.validation.Validatable
~
~protocol UserProtocol {
~    rpc emailAvailable(email: EmailAddress) throws RpcOnlyError -> Bool
~}
~
~service UserService {
~    @Inject let api: UserProtocol
~
~    func emailAvailable(email: EmailAddress) async -> Bool {
~        do {
~            return try await api.emailAvailable(email: email)
~        } catch .rpcFailure {
~            return true
~        }
~    }
~}
~
~struct SignupRequest implements Codable, Validatable {
@Check(UserService.emailAvailable, message: "An account with this email already exists")
var email: EmailAddress
~}
```

The function takes the field's parsed value and answers `true` when it is acceptable. On the server it
always runs in full, before the rpc's handler, and a failure appears under its field. In the browser it
runs as the person leaves the field:

- **Debounced on blur:** 300 ms after the field loses focus, on the app's `Clock`, so leaving and
  re-entering the field quickly runs it once. Typing alone never runs it.
- **After the field's other rules:** only text that parses and passes the field's constraints is sent.
- **The newest answer wins.** An answer for text the field no longer holds is dropped, and the earlier
  call's cancellation signal fires.
- **A refusal shows under the field** until a later check of new text passes, and submitting the same
  text again is refused without a round trip. A check that throws, or a service the browser doesn't
  have (a server-only one), flags nothing there: the server's check decides.

In a test, leave the field with `blur()` and move the `TestClock` past the debounce:

```k,ignore
let email = screen.textField(labeled: "Email")

email.fill("ada@example.com")
email.blur()
clock.advance(by: 300.milliseconds)
await screen.settle()

expect screen.text("An account with this email already exists").isVisible
```

## The last line of defence

Two sign-ups can both pass `@Check` before either is stored. So **the database insert itself reports a
duplicate** of a `@Unique` field as a typed `UniqueViolation`, which the server maps to its own error case
(`emailTaken` above):

```k
// examples/notes/src/accounts/AuthApi.k
do {
    try await accounts.insert(account)
} catch .email {
    throw SignUpError.emailTaken
}
```

See [Persistence](persistence.md#writes).
