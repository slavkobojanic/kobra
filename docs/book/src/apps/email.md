# Email

Email is sent through the server-only **`EmailProvider`** interface in `k.server`, with a built-in SMTP
implementation configured from the environment. Messages are a `@Builder` struct, and addresses are a
validated **`EmailAddress`** type, so an invalid address can't reach `send`.

```k
let email = Email(to: order.email, subject: "Your order is confirmed", text: body)
    .html(htmlBody)
    .replyTo(EmailAddress("support@shop.example"))

try await mailer.send(email: email)
```

## `EmailAddress`

`EmailAddress` (in `k.text`) is an email address, optionally with a display name
(`K Shop <orders@shop.example>`). It follows the literal-and-parse convention:

| Member | Meaning |
|---|---|
| `EmailAddress("orders@shop.example")` | a literal, validated at compile time |
| `EmailAddress.parse(text) -> EmailAddress?` | parses runtime text; `nil` if it isn't an address, or contains a line break |
| `address.address`, `address.displayName` | the parts |
| `"{address}"` | the address in message-header form |

- It implements `Codable` (as a string), `Equatable`, `Hashable` and `ConfigValue`, so it works in rpc
  messages, entities and configs.
- **A form field of type `EmailAddress`** binds to a `textField` as text and is validated as an email
  address, so it needs no `@Email`, and the form's value holds the parsed address
  ([Forms and validation](forms.md)).
- So storing addresses as `EmailAddress` all the way through, in the request, the entity and the config,
  means `Email(to: order.email, …)` simply type-checks.

## `EmailProvider`

```k
public interface EmailProvider {
    func send(email: Email) async throws EmailError
}

public enum EmailError implements Error {
    case unavailable                       // can't connect, timed out, or a temporary (4xx) SMTP reply
    case rejected(reason: String)          // a permanent (5xx) SMTP reply, with the server's text
    case invalidMessage(reason: String)    // the message can't be sent as written
}
```

- `EmailProvider` is a server-only platform interface, injectable only into server-side services.
- **The built-in default is `SmtpEmailProvider`,** the only implementation outside tests, so
  `@Inject let mailer: EmailProvider` resolves to it with no `use` rule. Vendor packages, such as an HTTP
  API provider, add implementations, and the app then chooses with `use … for EmailProvider` in `App.k`.
- **The error cases match the decision a caller has to make.** `unavailable` may work later; `rejected`
  and `invalidMessage` won't. [Background jobs](jobs.md) map them to `JobError.retryable` and
  `.permanent`.
- **`send` never panics on message content.** A line break in the subject, which would allow header
  injection when the subject comes from user input, makes `send` throw `invalidMessage`. Addresses can't
  contain one, by type.

## `Email` is a `@Builder` struct

| Field | Kind | Default |
|---|---|---|
| `to: EmailAddress` | required | |
| `subject: String` | required | |
| `text: String` | required; every message has a plain-text part | |
| `html: String?` | chained | `nil` |
| `from: EmailAddress?` | chained | `MAIL_FROM` |
| `replyTo: EmailAddress?` | chained | none |
| `cc: List<EmailAddress>`, `bcc: List<EmailAddress>` | chained | empty |

## SMTP configuration

`SmtpEmailProvider` injects the standard `SmtpConfig`:

| Variable | Type | Default |
|---|---|---|
| `SMTP_HOST` | `String` | required |
| `SMTP_PORT` | `Int` | `587` |
| `SMTP_SECURITY` | `.startTls`, `.tls` or `.none` | `.startTls` |
| `SMTP_USERNAME` | `String?` | `nil` (no authentication) |
| `SMTP_PASSWORD` | `Secret<String>?` | `nil`; usually `SMTP_PASSWORD_FILE` |
| `MAIL_FROM` | `EmailAddress` | required, for example `K Shop <orders@shop.example>` |

- The config is required only when `SmtpEmailProvider` is reachable, so an app that never sends email
  needs none of these.
- `SMTP_SECURITY=none` is a startup warning outside private networks, and is meant for local catchers
  such as Mailpit.

## Tests

**`k test` installs `RecordingEmailProvider`** (from `k.test`) for `EmailProvider` by default, so no test
can send real mail. A test reaches it with `@Inject let mailer: RecordingEmailProvider`:

| Member | Meaning |
|---|---|
| `sent: List<Email>` | every message sent so far, oldest first; read-only to tests |
| `simulateFailure(error: EmailError)` | makes every later `send` throw `error` |

```k
test OrderConfirmationJob {
    @Inject let job: OrderConfirmationJob
    @Inject let mailer: RecordingEmailProvider

    test "a temporary mail outage is retried" {
        mailer.simulateFailure(error: .unavailable)

        expect throws JobError.retryable {
            try await job.run(payload: OrderConfirmation(orderId: Order.paid.id))
        }
    }
}
```
