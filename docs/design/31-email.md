# Email

## Decision

Email is sent through the server-only **`EmailProvider`** interface in `k.server`, with a built-in SMTP implementation configured from the environment. Messages are a `@Builder` struct, and addresses are a validated **`EmailAddress`** type, so an invalid address can't reach `send`.

```k
let email = Email(to: order.email, subject: "Your order is confirmed", text: body)
    .html(htmlBody)
    .replyTo(EmailAddress("support@shop.example"))

try await mailer.send(email: email)
```

## `EmailAddress`

`EmailAddress` (in `k.text`) is an email address, optionally with a display name (`K Shop <orders@shop.example>`). It follows the literal-and-parse convention ([20-standard-library.md](20-standard-library.md#literals-and-parsing)):

| Member | Meaning |
|---|---|
| `EmailAddress("orders@shop.example")` | a literal, validated at compile time |
| `EmailAddress.parse(text) -> EmailAddress?` | parses runtime text; `nil` if it isn't an address, or contains a line break. Normalizes as described below. |
| `address.address`, `address.displayName` | the parts |
| `"{address}"` | the address in message-header form |

- **`parse` trims surrounding whitespace and lowercases the domain part only.** `"  Ana@Example.COM "` parses to `Ana@example.com`. The local part (before the `@`) keeps its case, because mail servers may treat it as case-sensitive. A literal is normalized the same way at compile time.
- **A display name precedes the address in angle brackets** (`Ana Doe <ana@example.com>`). `parse` collapses runs of spaces in the name. A name holding an RFC 5322 special other than `.` (a comma, a colon, brackets, `@`, a quote or a backslash) must be a quoted string, with `\"` and `\\` escapes, and keeps its quotes; a quoted name that needs none is unquoted, so equal addresses have one text. An unquoted name with a special (`Doe, Ana <ana@example.com>`), an empty name and bare angle brackets (`<ana@example.com>`) are `nil`. `displayName` is the name unquoted; equality compares the whole normalized text, name included.
- It implements `Codable` (as a string), `Equatable`, `Hashable` and `ConfigValue`, so it works in rpc messages, entities and configs. Decoding and config values go through `parse`, so they are normalized too, and equality compares normalized addresses.
- **A form field of type `EmailAddress`** binds to a `textField` as text and is validated as an email address when the form validates, so it needs no `@Email` and the form's value holds the parsed address ([15-forms-and-validation.md](15-forms-and-validation.md#the-forms-initial-value-and-its-value)).
- **`@Email` on an `EmailAddress` field is redundant:** a warning, with a quick fix that removes it. `@Email` remains for `String` fields.
- **The message for text that doesn't parse** is set with `@Format(message: "Enter a valid email address")` on the field ([15-forms-and-validation.md](15-forms-and-validation.md#typed-fields-and-format)).

## `EmailProvider`

```k
public interface EmailProvider {
    func send(email: Email) async throws EmailError
}

public enum EmailError implements Error {
    case unavailable                       // can't connect, timed out, or a 4xx (temporary) SMTP reply
    case rejected(reason: String)          // a 5xx (permanent) SMTP reply, with the server's text
    case invalidMessage(reason: String)    // the message can't be sent as written
}
```

- The interface is a server-only stdlib platform interface ([10-services.md](10-services.md#layering)).
- **The built-in default is `SmtpEmailProvider`.** It is the only implementation outside tests, so `@Inject let mailer: EmailProvider` resolves to it with no `use` rule. Vendor packages (an HTTP API provider, for example) add implementations, and the app then chooses with `use … for EmailProvider` in `App.k`.
- **The error cases match the decision a caller has to make.** `unavailable` may work later; `rejected` and `invalidMessage` won't. Background jobs map them to `JobError.retryable` and `.permanent` ([30-background-jobs.md](30-background-jobs.md)).
- **`send` never panics on message content.** A carriage return or line feed in the subject, which would allow header injection when the subject comes from user input, makes `send` throw `invalidMessage`. Addresses can't contain one, by type.

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

`SmtpEmailProvider` injects the stdlib `SmtpConfig` ([25-config-and-secrets.md](25-config-and-secrets.md#stdlib-configs)):

| Variable | Type | Default |
|---|---|---|
| `SMTP_HOST` | `String` | required |
| `SMTP_PORT` | `Int` | `587` |
| `SMTP_SECURITY` | `.startTls`, `.tls` or `.none` | `.startTls` |
| `SMTP_USERNAME` | `String?` | `nil` (no authentication) |
| `SMTP_PASSWORD` | `Secret<String>?` | `nil`; usually `SMTP_PASSWORD_FILE` |
| `MAIL_FROM` | `EmailAddress` | required, for example `K Shop <orders@shop.example>` |

- The config is required only when `SmtpEmailProvider` is reachable, so an app that never sends email needs none of these.
- `SMTP_SECURITY=none` is a startup warning outside private networks, and is meant for local catchers such as Mailpit.

## Tests

**`k test` installs `RecordingEmailProvider`** (from `k.test`) for `EmailProvider` by default ([21-testing.md](21-testing.md#test-doubles-and-defaults)), so no test can send real mail:

| Member | Meaning |
|---|---|
| `sent: List<Email>` | every message sent so far, oldest first. Read-only to tests. |
| `simulateFailure(error: EmailError)` | makes every later `send` throw `error` |

A test reaches it with `@Inject let mailer: RecordingEmailProvider`.

## Why

- A typed address moves validation to where the text enters the system, so `send` can't fail on a malformed address.
- A three-case error enum is the smallest set that still lets callers decide correctly whether to retry.
- A builder struct keeps the common case (to, subject, text) short and makes optional parts explicit, following the named-or-chained rule.
- Configuring SMTP through standard variables means an app sends mail with no code beyond `send`.
