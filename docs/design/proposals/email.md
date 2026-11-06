# Proposal: Sending Email (`k.server`)

> **Status: proposal.** Not yet accepted into the design. First used by `examples/shop`.

## Problem

[20-standard-library.md](../20-standard-library.md) lists "email (SMTP provider)" in `k.server` and says vendor integrations build on stdlib provider interfaces "such as `EmailProvider`", each with "a sensible built-in default". The interface, the message type, the errors and the SMTP configuration aren't defined.

## Proposal

### `EmailProvider`

```k
public interface EmailProvider {
    func send(email: Email) async throws EmailError
}

public enum EmailError implements Error {
    case unavailable                  // can't connect, timed out, or a 4xx (temporary) SMTP reply
    case rejected(reason: String)     // a 5xx (permanent) SMTP reply, with the server's text
}
```

- The interface is server-only.
- **The built-in default is `SmtpEmailProvider`.** It is the only implementation outside tests, so `@Inject let mailer: EmailProvider` resolves to it with no `use` rule. Vendor packages (an HTTP API provider, for example) add implementations, and the app then chooses with `use … for EmailProvider` in `App.k`.
- The two error cases match the retry decision a caller has to make: `unavailable` may work later, `rejected` won't. Background jobs map them to `JobError.retryable` and `.permanent` ([background-jobs.md](background-jobs.md)).

### `Email` is a `@Builder` struct

```k
let email = Email(to: order.email, subject: "Your order is confirmed", text: body)
    .html(htmlBody)
    .replyTo("support@shop.example")
```

| Field | Kind | Default |
|---|---|---|
| `to: String` | required | |
| `subject: String` | required | |
| `text: String` | required; every message has a plain-text part | |
| `html: String?` | chained | `nil` |
| `from: String?` | chained | `MAIL_FROM` |
| `replyTo: String?`, `cc: List<String>`, `bcc: List<String>` | chained | none |

Addresses are validated like `@Email` fields; an invalid address panics, since it should have been validated where it was entered.

### SMTP configuration

`SmtpEmailProvider` injects the stdlib `SmtpConfig` ([config-and-secrets.md](config-and-secrets.md)):

| Variable | Type | Default |
|---|---|---|
| `SMTP_HOST` | `String` | required |
| `SMTP_PORT` | `Int` | `587` |
| `SMTP_SECURITY` | `.startTls`, `.tls` or `.none` | `.startTls` |
| `SMTP_USERNAME` | `String?` | `nil` (no authentication) |
| `SMTP_PASSWORD` | `Secret<String>?` | `nil`; usually `SMTP_PASSWORD_FILE` |
| `MAIL_FROM` | `String` | required, for example `K Shop <orders@shop.example>` |

Since the config is only required when `SmtpEmailProvider` is reachable, an app that never sends email needs none of these. `SMTP_SECURITY=none` is a startup warning outside private networks, and is meant for local catchers such as Mailpit.

### Tests

Tests replace the provider with a fake implementing `EmailProvider` that records messages (`use RecordingMailer for EmailProvider`), and inspect it with `@Inject let mailer: RecordingMailer` ([testing-support.md](testing-support.md)).

## Why

- A two-case error enum is the smallest set that still lets callers decide correctly whether to retry.
- A builder struct keeps the common case (to, subject, text) short and makes optional parts explicit, following the named-or-chained rule.
- Configuring SMTP through standard variables means an app sends mail with no code beyond `send`.
