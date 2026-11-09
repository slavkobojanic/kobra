# Configuration and Secrets

## Decision

Configuration and secrets come from the **process environment**, through typed **`config`** declarations. A config is server-only, validated before the app serves anything, redacted when printed, and injected like a service. Tests never read the environment.

```k
// src/payments/StripeWebhookConfig.k

/// Settings for verifying Stripe webhook signatures.
public config StripeWebhookConfig {
    /// The endpoint's signing secret (`whsec_…`).
    @Env("STRIPE_WEBHOOK_SECRET") let signingSecret: Secret<String>

    /// How far a signature's timestamp may be from the current time.
    @Env("STRIPE_WEBHOOK_TOLERANCE") let tolerance: Duration = 5.minutes
}

public service StripeWebhookVerifier {
    @Inject let config: StripeWebhookConfig
}
```

The requirements this meets:

- **Type-safe:** a port is an `Int`, a timeout a `Duration`, an origin a `Url`. A malformed value is caught before the app serves anything.
- **Server-only:** neither the values nor the code that reads them can reach the browser.
- **Twelve-factor:** values come from the process environment, so one built executable runs anywhere. Container secrets (Docker and Kubernetes secret files) work without glue code.
- **Hermetic tests.**

## `config` declarations

- **`config` is a declaration keyword**, because a config has its own compiler semantics: the runtime constructs it from the environment, validates it at startup, and keeps it server-only.
- **Every field names its variable with `@Env("NAME")`.** Names are never derived from field names: an environment variable is a contract with something outside the codebase (a compose file, a Kubernetes manifest, a hosting dashboard), like a named URL ([12-routing.md](12-routing.md)). Renaming the field never renames the variable, and a search for `STRIPE_WEBHOOK_SECRET` finds the code.
- **Fields are `let`,** and like every member of a public type they are public without a modifier ([16-modules.md](16-modules.md)).
- **Two configs naming the same variable is a compile error**, as are fields without `@Env`.
- **A field with a default is optional; one without is required.** `T?` fields are optional with `nil`.
- **Field types implement `ConfigValue`**, which the stdlib provides for `String`, `Int`, `Double`, `Decimal`, `Bool` (`true`/`false`), `Duration` (`30s`, `5m`, `2h`, `10.5s`), `Url`, `Uuid`, `EmailAddress`, enums without associated values (by case name: `SMTP_SECURITY=startTls`), `List<T>` (comma-separated), and `Secret<T>`. Any other type is a compile error at the field.
- **Constraint annotations from [15-forms-and-validation.md](15-forms-and-validation.md) apply,** in `k dev`, `k run` and `dist/app` alike: `@Range(1...65535) @Env("PORT") let port: Int`. A failed constraint is a startup problem like a malformed value.
- **Constraints on a `Secret<T>` field are allowed** and check the revealed value at startup: `@Matches("^whsec_[A-Za-z0-9]+$") @Env("STRIPE_WEBHOOK_SECRET") let signingSecret: Secret<String>`. The failure message names the variable and the constraint, never the value.
- **`@Matches` takes a string literal regular expression,** validated at compile time: an invalid pattern is a compile error, not a startup error. The build manifest carries the pattern and the annotation's `message:`, and startup reports `Secret(•••) <message>` (or `doesn't match <pattern>`) for a value that fails it, so an empty or placeholder password hash stops the app. The runtime checks the pattern with Rust's `regex` syntax; one that only JavaScript understands (a look-around, a back-reference) isn't checked at startup.
- **A config is data.** It holds only `@Env` fields: no functions, no `@Inject`, no state. Static members may be added to a config only by extensions in a test-only module, for shared test values (`StripeWebhookConfig.sample`).
- **The runtime reads no environment variable that isn't a `config` field,** the stdlib's own included. So `dist/app config` lists everything a deployment can set.

## `Secret<T>`

`Secret<T>` wraps a value that must not leak:

| Member | Meaning |
|---|---|
| `Secret(value)` | wraps a value; used by the runtime, and by tests |
| `secret.reveal() -> T` | the value, at the one point it is used |
| `secret.matches(_ candidate: T) -> Bool` | compares in constant time, so timing doesn't reveal how much of a guess was right (`Secret<String>` and `Secret<Bytes>`) |
| `"{secret}"` | interpolates as `Secret(•••)`, so logging a config can't print it |

- **`Secret` doesn't implement `Codable`, `Equatable` or `Hashable`.** It can't be put in an rpc response, a `@Load` field, a job payload or an entity, and it can't be compared with `==`. Each of those is a compile error.
- **Revealed values keep that rule.** Within a function, the compiler tracks values computed from `reveal()` (directly, through interpolation or concatenation, or through local bindings). `==` or `!=` with such a value is a compile error, with a quick fix to `secret.matches(candidate)`. So `request.headers["authorization"] == "Bearer {config.token.reveal()}"` doesn't compile.
- **Most uses need no `reveal()`.** Outgoing requests take secrets directly (`.bearerToken(config.apiKey)`, [32-http-client.md](32-http-client.md)), and so does `Hmac` ([33-crypto.md](33-crypto.md)). Logging redacts them.
- `reveal()` is greppable on purpose: a review of secret handling is a search for `.reveal()`.
- **An optional secret is still a secret.** A field of type `Secret<T>?` may come from `NAME_FILE`, is never printed, and is `nil` when unset, so a secret with a fallback (`config.token ?? service.token`) needs no sentinel value.

## Where values come from

At startup, for each field:

1. **`NAME_FILE`**, if it is set and the field is a `Secret`: the file's contents, with one trailing line ending (`\r\n` or `\n`) removed. This is the convention of the official Docker images (`POSTGRES_PASSWORD_FILE`), and it is how Docker and Kubernetes secrets arrive (`/run/secrets/<name>`). Setting `NAME_FILE` for a field that isn't a `Secret` is a startup error, because non-secret values belong in plain variables.
2. **`NAME`**, if it is set.
3. **The field's default.**
4. Otherwise the value is missing.

Setting both `NAME` and `NAME_FILE` is a startup error, so it is never ambiguous which one won.

## Validated at startup, all at once

- **When `dist/app` starts, it builds every config reachable from the active dependency graph** and validates them before it opens a port or a database connection. With any problem, it prints them all and exits with status 78 (`EX_CONFIG`):

  ```
  error: configuration is invalid (2 problems)
    STRIPE_WEBHOOK_SECRET     missing (required by payments.StripeWebhookConfig.signingSecret;
                              set STRIPE_WEBHOOK_SECRET or STRIPE_WEBHOOK_SECRET_FILE)
    STRIPE_WEBHOOK_TOLERANCE  "5 minutes" is not a Duration (expected e.g. 300s, 5m)
  ```

  Secret values are never printed, even when they are malformed or fail a constraint:

  ```
    STRIPE_WEBHOOK_SECRET     Secret(•••) doesn't match ^whsec_[A-Za-z0-9]+$
  ```
- **"Reachable" is computed at compile time from the `use` rules in `App.k`** ([23-app-structure.md](23-app-structure.md#environments)). In a shop where only `StripeCheckout` injects `StripeApiConfig`, and only production uses `StripeCheckout`, `STRIPE_API_KEY` is required by a production build and never by `k dev`. Adding an injection is what makes a variable required; there is no list to keep in sync.
- **`dist/app config`** prints the table of every variable the build reads: its type, whether it is required, whether it is a secret, its default, and whether it is currently set. It never prints secret values.

## Injection

- **A config is injected like a service, but only where secrets may go:** into server-side services and into repositories ([10-services.md](10-services.md#layering)). Injecting one into a component, page, layout or client-side service, or importing a config type into client code, is a compile error that names the path by which it would reach the browser.
- A service that injects a config is therefore server-only, like one that injects a repository.
- Configs are process-wide values, built once at startup, and can be injected in every scope. Changing a value means restarting the process.

## Stdlib configs

The stdlib's own settings are ordinary `config` declarations, with documented variable names:

| Config (module) | Variable | Field | Default |
|---|---|---|---|
| `ServerConfig` (`k.server`) | `PORT` | `port: Int` | `8080` |
| | `K_PUBLIC_ORIGIN` | `publicOrigin: Url` | required in production; `http://localhost:{PORT}` in development; `http://localhost` in tests |
| | `K_SHUTDOWN_GRACE` | `shutdownGrace: Duration` | `25.seconds` |
| | `K_TRUSTED_PROXIES` | `trustedProxies: List<String>` (CIDR ranges) | empty |
| `DatabaseConfig` (`k.db`) | `DATABASE_URL` | `url: Url` (`postgres://user@host:5432/db` or `sqlite:path`) | required in production; `sqlite:.k/<package>.db` in development |
| | `DATABASE_PASSWORD` | `password: Secret<String>?` | `nil` |
| `JobConfig` (`k.server`) | `K_JOB_CONCURRENCY` | `@Range(1...256) concurrency: Int` | `4` |
| `SessionConfig` (`k.auth`) | `K_SESSION_MAX_AGE` | `maxAge: Duration` | `30.days` |
| | `K_SESSION_IDLE` | `idleTimeout: Duration` | `7.days` |
| `SmtpConfig` (`k.server`) | `SMTP_HOST`, `SMTP_PORT`, `SMTP_SECURITY`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `MAIL_FROM` | see [31-email.md](31-email.md) | |

- Keeping the database password out of `DATABASE_URL` lets one secret file serve both Postgres (`POSTGRES_PASSWORD_FILE`) and the app (`DATABASE_PASSWORD_FILE`).
- How `K_PUBLIC_ORIGIN`, `K_SHUTDOWN_GRACE` and `K_TRUSTED_PROXIES` are used is in [38-deployment.md](38-deployment.md); the session settings are in [26-auth-and-sessions.md](26-auth-and-sessions.md).

## Per environment

| Environment | Values come from |
|---|---|
| `.production` (`dist/app`) | the process environment only |
| `.development` (`k dev`, `k run`, `k migrate`) | the process environment, plus an optional `.env` file at the project root, then the project's development settings and the development defaults (below). **The process environment wins on a conflict,** and `k dev` prints which `.env` keys were shadowed: before each start, one note naming the keys set in both places to different values, never a value. `.env` is git-ignored; a project commits `.env.example` instead. `_FILE` variables may point at paths relative to the project root. |
| `.test` (`k test`) | **never the environment.** Stdlib configs have built-in test values (an in-memory SQLite database, the origin `http://localhost`); the app's own configs are bound per test group (below). |

### Development needs no setup

A development run starts with one command and no `.env`. What the process environment and `.env` leave unset comes, in this order, from:

1. **`[dev.env]` and `[dev.secrets]` in `k.toml`** ([24-projects.md](24-projects.md#development-settings)): committed development values (never for a `Secret` field: that is refused at startup), and secrets generated on first use into the git-ignored `.k/secrets/`.
2. **The development defaults**, for the stdlib's own configs:
   - `DATABASE_URL` is `sqlite:.k/<package>.db`, a SQLite file in the project's `.k/`, created and migrated at startup.
   - An app that sends email over SMTP (`SmtpConfig` is reachable) and sets no `SMTP_HOST` gets a **local mail catcher**: `SMTP_HOST`, `SMTP_PORT` and `SMTP_SECURITY=none` point at an SMTP server inside `k dev` that accepts every message, prints its sender, recipients, subject and links, and keeps it as an `.eml` file in `.k/mail/` ([31-email.md](31-email.md#in-development)). `MAIL_FROM` defaults to `noreply@example.com` then. Setting `SMTP_HOST` (in `.env`, to a local Mailpit for example) turns the catcher off.
   - `K_PUBLIC_ORIGIN` is `http://localhost:<port>`, as before.

**Production builds still require explicit configuration**: `dist/app` reads only the process environment, so a missing `DATABASE_URL` is still exit 78 there.

**Committed example files hold placeholders, never working values.** A `.env.example` or `secrets/*.example` file that someone copies into place must fail validation until a real value is set (for example a password hash that isn't a PHC string), so a documented sample password can never become a deployed one. Scripts that need real values (an end-to-end test) generate them.

## Tests

A test group binds a config to a value with the `use … for …` it already uses for fakes ([21-testing.md](21-testing.md)):

```k
test StripeWebhookVerifier {
    use StripeWebhookConfig(signingSecret: Secret("whsec_test")) for StripeWebhookConfig

    @Inject let verifier: StripeWebhookVerifier
}
```

- **Inside `test` bodies, `use` takes a value for a config:** `use <expression> for <Config>`. The expression may be the config's memberwise initializer, which exists only in test code, or a shared static member such as `StripeWebhookConfig.sample`. Outside test code, constructing a config is a compile error: production values come only from the environment.
- `App.k` can't bind a config to a value, so a secret can't be committed by putting it there.
- A test group whose graph reaches an unbound app config is a compile error, with a quick fix that adds the `use` line.

## Why

- A dedicated kind gives the compiler what it needs to enforce the important rules (server-only, validated at startup, unconstructible in production code) without conventions.
- Explicit `@Env` names treat variables as the external contract they are, the same reasoning as named URLs and lockfiles.
- `NAME_FILE` follows an existing ecosystem convention, so Docker and Kubernetes secrets need no entrypoint scripts.
- Computing required variables from the DI graph means a development machine never needs production credentials, and a production deploy can't forget one: the process refuses to start.

## Open questions

- Reloading configuration without a restart (rotating a secret). Secret rotation today means a rolling restart.
- Whether `dist/app config --check` should run in CI against a deployment's environment description (a compose file or a Kubernetes manifest).
