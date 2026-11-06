# Proposal: Configuration and Secrets

> **Status: proposal.** Not yet accepted into the design. First used by `examples/shop`.

## Problem

[10-services.md](../10-services.md) says server-side services may inject "server-only resources (secrets)" and that secrets can't end up in the client bundle, but nothing defines what a secret or a configuration value is, where values come from, when they are checked, or how tests supply them. [k-toml.md](k-toml.md) deliberately keeps environment variables and secrets out of the committed manifest, and [app-structure.md](app-structure.md), [auth-and-sessions.md](auth-and-sessions.md) and [repository-writes.md](repository-writes.md) all defer to "the secrets and configuration design". This is it.

The requirements:

- **Type-safe:** a port is an `Int`, a timeout a `Duration`, an origin a `Url`. A malformed value is caught before the app serves anything.
- **Server-only:** neither the values nor the code that reads them can reach the browser.
- **Twelve-factor:** values come from the process environment, so one built executable runs anywhere. Container secrets (Docker and Kubernetes secret files) work without glue code.
- **Hermetic tests:** tests never read the machine's environment.

## Proposal

### `config` declarations

```k
// src/payments/StripeWebhookConfig.k

/// Settings for verifying Stripe webhook signatures.
public config StripeWebhookConfig {
    /// The endpoint's signing secret (`whsec_…`).
    @Env("STRIPE_WEBHOOK_SECRET") public let signingSecret: Secret<String>

    /// How far a signature's timestamp may be from the current time.
    @Env("STRIPE_WEBHOOK_TOLERANCE") public let tolerance: Duration = 5.minutes
}
```

- **`config` is a declaration keyword**, because a config has its own compiler semantics: it is constructed by the runtime from the environment, validated at startup, and server-only. That follows "keywords declare kinds" ([01-principles.md](../01-principles.md)).
- **Every field names its variable with `@Env("NAME")`.** Names are never derived from field names: an environment variable is a contract with something outside the codebase (a compose file, a Kubernetes manifest, a hosting dashboard), like a named URL ([12-routing.md](../12-routing.md)). Renaming the field never renames the variable, and a search for `STRIPE_WEBHOOK_SECRET` finds the code. `@Env` takes `let`; config fields are immutable.
- **Two configs naming the same variable is a compile error**, as are fields without `@Env`.
- **A field with a default is optional; one without is required.** `T?` fields are optional with `nil`.
- **Field types implement `ConfigValue`**, which the stdlib provides for `String`, `Int`, `Double`, `Decimal`, `Bool` (`true`/`false`), `Duration` (`30s`, `5m`, `2h`, `10.5s`), `Url`, enums without associated values (by case name: `SMTP_SECURITY=startTls`), `List<T>` (comma-separated), and `Secret<T>`. Any other type is a compile error at the field.
- **Constraint annotations from [15-forms-and-validation.md](../15-forms-and-validation.md) apply:** `@Email @Env("SHOP_ADMIN_EMAIL") let email: String`, `@Range(1...65535) @Env("PORT") let port: Int`.
- Configs may hold only `@Env` fields: no functions, no `@Inject`, no state. A config is data.

### `Secret<T>`

`Secret<T>` wraps a value that must not leak:

| Member | Meaning |
|---|---|
| `Secret(value)` | wraps a value; used by the runtime, and by tests |
| `secret.reveal() -> T` | the value, at the one point it is used (an `Authorization` header, an HMAC key) |
| `"{secret}"` | interpolates as `Secret(•••)`, so logging a config can't print it |

- **`Secret` doesn't implement `Codable`, `Equatable` or `Hashable`.** It can't be put in an rpc response, a `@Load` field, a job payload or an entity, and it can't be compared with `==`, which would leak timing. Each of those is a compile error.
- `reveal()` is greppable on purpose: a review of secret handling is a search for `.reveal()`.

### Where values come from

At startup, for each field:

1. **`NAME_FILE`**, if it is set and the field is a `Secret`: the file's contents, with one trailing newline removed. This is the convention of the official Docker images (`POSTGRES_PASSWORD_FILE`), and it is how Docker and Kubernetes secrets arrive (`/run/secrets/<name>`). Setting `NAME_FILE` for a field that isn't a `Secret` is a startup error, because non-secret values belong in plain variables.
2. **`NAME`**, if it is set.
3. **The field's default.**
4. Otherwise the value is missing.

Setting both `NAME` and `NAME_FILE` is a startup error, so it is never ambiguous which one won.

### Validated at startup, all at once

- **When `dist/app` starts, it builds every config reachable from the active dependency graph** and validates them before it opens a port or a database connection. With any problem, it prints them all and exits with status 78 (`EX_CONFIG`):

  ```
  error: configuration is invalid (2 problems)
    STRIPE_WEBHOOK_SECRET   missing (required by payments.StripeWebhookConfig.signingSecret;
                            set STRIPE_WEBHOOK_SECRET or STRIPE_WEBHOOK_SECRET_FILE)
    STRIPE_WEBHOOK_TOLERANCE  "5 minutes" is not a Duration (expected e.g. 300s, 5m)
  ```

  Secret values are never printed, even when they are malformed.
- **"Reachable" is computed at compile time from the `use` rules in `App.k`.** In the shop, only `StripeCheckout` injects `StripeApiConfig`, and only production uses `StripeCheckout`, so `STRIPE_API_KEY` is required by a production build and never by `k dev`. Adding an injection is what makes a variable required; there is no list to keep in sync.
- **`dist/app config`** prints the table of every variable the build reads: its type, whether it is required, whether it is a secret, its default, and whether it is currently set. It never prints secret values. Deployment tooling and reviewers read this instead of the source.

### Injection

```k
public service StripeWebhookVerifier {
    @Inject let config: StripeWebhookConfig
}
```

- **A config is injected like a service, but only where secrets may go:** into server-side services and repositories ([10-services.md](../10-services.md)). Injecting one into a component, page, layout or client-side service, or importing a config type into client code, is a compile error that names the path by which it would reach the browser.
- A service that injects a config is therefore server-only, like one that injects a repository. That is what keeps `StripeWebhookVerifier` out of the client bundle.
- Configs are app-scoped singletons, built once at startup. Changing a value means restarting the process.

### Stdlib configs

The stdlib's providers use the same mechanism, with documented variable names, so an app wires nothing by hand:

| Config (module) | Variables |
|---|---|
| `ServerConfig` (`k.server`) | `PORT` (default `8080`), `K_PUBLIC_ORIGIN` (the origin used for absolute URLs; see [deployment.md](deployment.md)) |
| `DatabaseConfig` (`k.db`) | `DATABASE_URL` (`postgres://user@host:5432/db` or `sqlite:path`), `DATABASE_PASSWORD` (`Secret`, optional) |
| `SmtpConfig` (`k.server`) | `SMTP_HOST`, `SMTP_PORT`, `SMTP_SECURITY`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `MAIL_FROM` ([email.md](email.md)) |

Keeping the database password out of `DATABASE_URL` lets one secret file serve both Postgres (`POSTGRES_PASSWORD_FILE`) and the app (`DATABASE_PASSWORD_FILE`).

### Per environment

| Environment | Values come from |
|---|---|
| `.production` (`dist/app`) | the process environment only |
| `.development` (`k dev`) | the process environment, after loading an optional `.env` file at the project root. `.env` is for the developer's machine: it is git-ignored, and a project commits a `.env.example` instead. `_FILE` variables may point at paths relative to the project root. |
| `.test` (`k test`) | **never the environment.** Each test group that reaches a config binds it, below. |

Stdlib configs have built-in values in `.test` (an in-memory SQLite database, origin `http://localhost`), so only the app's own configs need binding.

### Tests

A test group binds a config to a value with the `use … for …` it already uses for fakes ([21-testing.md](../21-testing.md)):

```k
test StripeWebhookVerifier {
    use StripeWebhookConfig(signingSecret: Secret("whsec_test")) for StripeWebhookConfig

    @Inject let verifier: StripeWebhookVerifier
}
```

- **In test code, a config has a memberwise initializer** with its defaults. Outside test code, constructing a config is a compile error: production values come only from the environment.
- `use <value> for <Config>` is allowed only in `test` groups. `App.k` can't bind a config to a value, so a secret can't be committed by putting it there.
- A test group whose graph reaches an unbound config is a compile error, with a quick fix that adds the `use` line. Shared samples go in a test-only module as static members (`use StripeWebhookConfig.sample for StripeWebhookConfig`; see [testing-support.md](testing-support.md)).

## Why

- A dedicated kind gives the compiler what it needs to enforce the important rules (server-only, validated at startup, unconstructible in production code) without conventions.
- Explicit `@Env` names treat variables as the external contract they are, the same reasoning as named URLs and lockfiles.
- `NAME_FILE` follows an existing ecosystem convention, so Docker and Kubernetes secrets need no entrypoint scripts.
- Computing required variables from the DI graph means a development machine never needs production credentials, and a production deploy can't forget one: the process refuses to start.
- Tests bind configs with existing syntax and never depend on the developer's shell.

## Open questions

- Reloading configuration without a restart (rotating a secret) is left out. Secret rotation today means a rolling restart.
- Whether `dist/app config --check` should run in CI against a deployment's environment description (a compose file or a Kubernetes manifest).
- Session lifetime and cookie settings ([auth-and-sessions.md](auth-and-sessions.md)) could become a stdlib `SessionConfig`, or stay annotations on the `app`, since they describe behavior rather than environment.
