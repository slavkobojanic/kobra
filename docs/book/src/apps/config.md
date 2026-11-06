# Configuration and secrets

Configuration and secrets come from the **process environment**, through typed **`config`**
declarations. A config is server-only, validated before the app serves anything, redacted when printed,
and injected like a service. Tests never read the environment.

```k
// src/payments/StripeWebhookConfig.k

/// Settings for verifying Stripe webhook signatures.
public config StripeWebhookConfig {
    /// The endpoint's signing secret (`whsec_…`).
    @Env("STRIPE_WEBHOOK_SECRET") let signingSecret: Secret<String>

    /// How far a signature's timestamp may be from the current time.
    @Env("STRIPE_WEBHOOK_TOLERANCE") let tolerance: Duration = 5.minutes
}
```

```k
public service StripeWebhookVerifier {
    @Inject let config: StripeWebhookConfig

    …
}
```

One built executable therefore runs anywhere: values are typed, checked at startup, never reach the
browser, and arrive the way Docker and Kubernetes deliver them.

## `config` declarations

- **`config` is a declaration keyword,** because a config has its own compiler semantics: the runtime
  constructs it from the environment, validates it at startup, and keeps it server-only.
- **Every field names its variable with `@Env("NAME")`.** Names are never derived from field names. An
  environment variable is a contract with something outside the codebase (a compose file, a Kubernetes
  manifest, a hosting dashboard), like a named URL. Renaming the field never renames the variable, and a
  search for `STRIPE_WEBHOOK_SECRET` finds the code.
- **Fields are `let`,** and like every member of a public type they are public without a modifier.
- **A field with a default is optional; one without is required.** `T?` fields are optional with `nil`.
- Two configs naming the same variable is a compile error, and so is a field without `@Env`.
- **Field types implement `ConfigValue`:** `String`, `Int`, `Double`, `Decimal`, `Bool` (`true`/`false`),
  `Duration` (`30s`, `5m`, `2h`, `10.5s`), `Url`, `Uuid`, `EmailAddress`, enums without associated values
  (by case name: `SMTP_SECURITY=startTls`), `List<T>` (comma-separated), and `Secret<T>`. Any other type is
  a compile error at the field.
- **Constraint annotations apply:** `@Range(1...65535) @Env("PORT") let port: Int`.
- **A config is data.** It holds only `@Env` fields: no functions, no `@Inject`, no state.

```k
/// The one staff account that can open the admin pages.
public config AdminConfig {
    /// The address staff sign in with.
    @Env("SHOP_ADMIN_EMAIL") let email: EmailAddress

    /// The Argon2id hash of the staff password, as a PHC string.
    @Env("SHOP_ADMIN_PASSWORD_HASH") let passwordHash: Secret<String>
}
```

## `Secret<T>`

`Secret<T>` wraps a value that must not leak:

| Member | Meaning |
|---|---|
| `Secret(value)` | wraps a value; used by the runtime, and by tests |
| `secret.reveal() -> T` | the value, at the one point it is used |
| `secret.matches(_ candidate: T) -> Bool` | compares in constant time, so timing doesn't reveal how much of a guess was right |
| `"{secret}"` | interpolates as `Secret(•••)`, so logging a config can't print it |

- **`Secret` doesn't implement `Codable`, `Equatable` or `Hashable`.** It can't be put in an rpc
  response, a `@Load` field, a job payload or an entity, and it can't be compared with `==`.
- **Revealed values keep that rule.** Within a function, the compiler tracks values computed from
  `reveal()`, directly or through interpolation, concatenation and local bindings. Comparing one with `==`
  is a compile error with a quick fix to `secret.matches(candidate)`.
- **Most uses need no `reveal()`.** Outgoing HTTP requests take secrets directly
  (`.bearerToken(config.apiKey)`, see [Calling HTTP APIs](http.md)), and so do `Hmac` and `Password`
  ([Bytes and cryptography](crypto.md)).
- `reveal()` is greppable on purpose: a review of secret handling is a search for `.reveal()`.

## Where values come from

At startup, for each field:

1. **`NAME_FILE`**, if it is set and the field is a `Secret`: the file's contents, with one trailing line
   ending removed. This is how Docker and Kubernetes secrets arrive (`/run/secrets/<name>`), and the
   convention of the official Docker images (`POSTGRES_PASSWORD_FILE`). Setting `NAME_FILE` for a field
   that isn't a `Secret` is a startup error.
2. **`NAME`**, if it is set.
3. **The field's default.**
4. Otherwise the value is missing.

Setting both `NAME` and `NAME_FILE` is a startup error, so it is never ambiguous which one won.

### Per environment

| Environment | Values come from |
|---|---|
| `.production` (`dist/app`) | the process environment only |
| `.development` (`k dev`) | the process environment, plus an optional `.env` file at the project root. The process environment wins on a conflict, and `k dev` prints which `.env` keys were shadowed. `_FILE` variables may point at paths relative to the project root. |
| `.test` (`k test`) | never the environment; see [Tests](#tests) |

### Example files hold placeholders

`.env` is git-ignored; a project commits `.env.example` instead. **Committed example files hold
placeholders, never working values.** A `.env.example` or `secrets/*.example` file that someone copies
into place must fail validation until a real value is set, for example a password hash that isn't a PHC
string. A documented sample password can then never become a deployed one. Scripts that need real values,
such as an end-to-end test, generate them.

```text
# .env.example
DATABASE_URL=sqlite:.k/shop.db
SHOP_ADMIN_EMAIL=admin@shop.example
# Generate a real hash into this file; the committed .example holds a placeholder.
SHOP_ADMIN_PASSWORD_HASH_FILE=secrets/admin_password_hash.txt
```

## Validated at startup, all at once

When `dist/app` starts, it builds every config reachable from the active dependency graph, and validates
them before it opens a port or a database connection. With any problem, it prints them all and exits with
status `78` (`EX_CONFIG`):

```text
error: configuration is invalid (2 problems)
  STRIPE_WEBHOOK_SECRET     missing (required by payments.StripeWebhookConfig.signingSecret;
                            set STRIPE_WEBHOOK_SECRET or STRIPE_WEBHOOK_SECRET_FILE)
  STRIPE_WEBHOOK_TOLERANCE  "5 minutes" is not a Duration (expected e.g. 300s, 5m)
```

Secret values are never printed, even when they are malformed.

**"Reachable" is computed at compile time from the `use` rules in `App.k`.** In a shop where only
`StripeCheckout` injects `StripeApiConfig`, and only production uses `StripeCheckout`, `STRIPE_API_KEY`
is required by a production build and never by `k dev`. Adding an injection is what makes a variable
required; there is no list to keep in sync.

`dist/app config` prints the table of every variable the build reads: its type, whether it is required,
whether it is a secret, its default, and whether it is currently set. It never prints secret values.

## Injection

- **A config is injected like a service, but only where secrets may go:** into server-side services and
  into repositories. Injecting one into a component, page, layout or client-side service, or importing a
  config type into client code, is a compile error that names the path by which it would reach the
  browser.
- A service that injects a config is therefore server-only.
- Configs are process-wide values, built once at startup, and can be injected in every scope. Changing a
  value means restarting the process.

## Standard configs

The standard library's own settings are ordinary `config` declarations, with documented variable names.
**The runtime reads no environment variable that isn't a `config` field,** so `dist/app config` lists
everything a deployment can set:

| Config (module) | Variable | Field | Default |
|---|---|---|---|
| `ServerConfig` (`k.server`) | `PORT` | `port: Int` | `8080` |
| | `K_PUBLIC_ORIGIN` | `publicOrigin: Url` | required in production; `http://localhost:{PORT}` in development; `http://localhost` in tests |
| | `K_SHUTDOWN_GRACE` | `shutdownGrace: Duration` | `25.seconds` |
| | `K_TRUSTED_PROXIES` | `trustedProxies: List<String>` (CIDR ranges) | empty |
| `DatabaseConfig` (`k.db`) | `DATABASE_URL` | `url: Url` (`postgres://…` or `sqlite:path`) | required |
| | `DATABASE_PASSWORD` | `password: Secret<String>?` | `nil` |
| `JobConfig` (`k.server`) | `K_JOB_CONCURRENCY` | `concurrency: Int`, 1 to 256 | `4` |
| `SessionConfig` (`k.auth`) | `K_SESSION_MAX_AGE` | `maxAge: Duration` | `30.days` |
| | `K_SESSION_IDLE` | `idleTimeout: Duration` | `7.days` |
| `SmtpConfig` (`k.server`) | `SMTP_HOST`, `SMTP_PORT`, `SMTP_SECURITY`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `MAIL_FROM` | see [Email](email.md#smtp-configuration) | |

Keeping the database password out of `DATABASE_URL` lets one secret file serve both Postgres
(`POSTGRES_PASSWORD_FILE`) and the app (`DATABASE_PASSWORD_FILE`).

## Tests

A test group binds a config to a value with the `use … for …` it already uses for fakes:

```k
test StripeWebhookVerifier {
    use StripeWebhookConfig.sample for StripeWebhookConfig

    @Inject let verifier: StripeWebhookVerifier
}
```

- **Inside `test` bodies, `use` takes a value for a config.** The value may be a shared static member,
  such as `StripeWebhookConfig.sample`, or the config's memberwise initializer,
  `StripeWebhookConfig(signingSecret: Secret("whsec_test"))`, which exists only in test code.
- **Static members may be added to a config only by extensions in a test-only module,** for shared test
  values:

  ```k
  // src/testing/Samples.k
  public extend StripeWebhookConfig {
      static let sample = StripeWebhookConfig(signingSecret: Secret("whsec_test"))
  }
  ```

- **Outside test code, constructing a config is a compile error:** production values come only from the
  environment.
- A test group whose graph reaches an unbound app config is a compile error, with a quick fix that adds
  the `use` line. Standard configs have built-in test values, so they never need one.

> **Open question.** Reloading configuration without a restart, for example to rotate a secret, isn't
> designed yet. Today secret rotation means a rolling restart.
