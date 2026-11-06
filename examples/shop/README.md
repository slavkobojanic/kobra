# K Shop

A small, production-style shop written entirely in K. Customers browse
products, check out as guests, and pay through Stripe Checkout. A Stripe
webhook confirms the payment, and a background job emails a confirmation over
SMTP. Staff sign in to an admin page that lists orders. The whole app ships
as one executable, `dist/app`, in a minimal container next to Postgres.

> The K compiler doesn't exist yet. This example was written by hand against
> the design in [`docs/design`](../../docs/design) (see
> [Design documents used](#design-documents-used)) and the grammar in
> [`docs/compiler/grammar.md`](../../docs/compiler/grammar.md). The commands
> below describe how it will run.

## What it demonstrates

| Feature | Where |
|---|---|
| `Dockerfile` that runs `k check` and `k build`, then copies only `dist/app` into `distroless/cc` | [`Dockerfile`](Dockerfile) |
| Compose with Postgres, health checks, a named volume and Docker secrets | [`docker-compose.yml`](docker-compose.yml), [`secrets/`](secrets) |
| Type-safe configuration and secrets, injected only into server-only services | [`StripeWebhookConfig.k`](src/payments/StripeWebhookConfig.k), [`StripeApiConfig.k`](src/payments/StripeApiConfig.k), [`AdminConfig.k`](src/admin/AdminConfig.k) |
| Entities, repositories, conditional updates and transactions on Postgres | [`src/store`](src/store) |
| A migration and its committed `schema.lock` | [`migrations/`](migrations), [`schema.lock`](schema.lock) |
| A webhook at a fixed URL, `page StripeWebhook at "/webhooks/stripe"`, verified with HMAC-SHA256 from `k.crypto` (no vendor SDK) | [`StripeWebhook.k`](src/webhooks/StripeWebhook.k), [`StripeSignature.k`](src/payments/StripeSignature.k), [`StripeWebhookVerifier.k`](src/payments/StripeWebhookVerifier.k) |
| `requires Authenticated, Role(.admin)` on a page and on an rpc, with a typed user id | [`Admin.k`](src/pages/Admin.k), [`AdminProtocol.k`](src/admin/AdminProtocol.k), [`StaffId.k`](src/admin/StaffId.k) |
| `RateLimit` on rpcs (placing orders, signing in) | [`StoreProtocol.k`](src/store/StoreProtocol.k), [`AdminProtocol.k`](src/admin/AdminProtocol.k) |
| A background job sending email through `k.server`'s SMTP provider | [`OrderConfirmationJob.k`](src/store/OrderConfirmationJob.k) |
| SEO metadata: `canonical`, `structuredData`, `socialPreview`, and a real `404` for missing products | [`ProductDetail.k`](src/pages/ProductDetail.k), [`Home.k`](src/pages/Home.k) |
| `use X for Y in .environment` | [`App.k`](App.k) |
| Unit, integration and HTTP-level tests next to the source | `*.test.k` |
| An end-to-end test with Docker Compose and `curl`, with generated secrets | [`scripts/e2e.sh`](scripts/e2e.sh) |

## How a purchase flows

1. `ProductDetail` submits an `@Form` of `OrderRequest` to `StoreService`,
   which calls `StoreProtocol.placeOrder`. The rpc is rate limited to 10 a
   minute per client, and the request is validated on the server too; a
   server-side validation failure comes back as `rpcFailure(.invalid(…))`
   and is shown under the form's fields.
2. `StoreApi` reserves stock and records the order in one transaction, using
   a conditional `UPDATE` so the last unit can't be sold twice. It then asks
   the `PaymentGateway` for a checkout URL, with return and cancel URLs from
   `PublicUrls`. If the gateway fails, the order is cancelled and the stock
   released.
3. In production, `StripeCheckout` creates a Stripe Checkout Session over
   HTTP, using the order id as `client_reference_id` and as the idempotency
   key. The customer pays on Stripe's page.
4. Stripe calls `/webhooks/stripe`. `StripeWebhookVerifier` checks the
   `Stripe-Signature` header (HMAC-SHA256 over `"{timestamp}.{raw body}"`,
   constant-time comparison through `Mac.matches`, five-minute replay
   window) before reading the JSON.
5. `PaymentRecorder` checks that the amount matches the order total, marks
   the order paid with a conditional update, and enqueues
   `OrderConfirmationJob` through `JobQueue` in the same transaction. A
   repeated webhook delivery changes nothing and sends no second email.
6. The worker inside `dist/app` runs the job, which emails the customer
   through SMTP, retrying with backoff when the server is unavailable.
7. The customer's `Receipt` page polls with `reload()` until the payment is
   confirmed.

## Structure

```
App.k                         the app: per-environment payment gateway, layout, sign-in page
k.toml                        manifest
schema.lock                   the schema the migrations produce, checked by `k check`
migrations/                   0001 creates the tables
src/
  theme/ShopTheme.k           design tokens
  store/                      catalog and orders: entities, repositories, StoreProtocol,
                              its server implementation (StoreApi) and client service
                              (StoreService), PaymentRecorder and OrderConfirmationJob
  payments/                   PaymentGateway and its two implementations, Stripe
                              signature verification, Stripe configs. Knows nothing
                              about orders, so `store` imports `payments` and never
                              the reverse.
  admin/                      staff sign-in and the order list: AdminProtocol, AdminApi,
                              AdminService, StaffId, StaffRole, AdminConfig
  webhooks/StripeWebhook.k    the endpoint page Stripe calls
  ops/HealthCheck.k           GET /health for Docker health checks
  pages/                      Storefront layout, Home, ProductDetail, Receipt, SignIn,
                              Admin, and the ProductCard component
  testing/                    test-only module: Stripe event builder, shared samples
scripts/e2e.sh                end-to-end test
scripts/demo-catalog.sql      the three demo products, for `k dev` and the end-to-end test
secrets/*.example             placeholder Docker secrets; real ones are git-ignored
```

Every file holds at most one public declaration, named after the file;
helpers in the same file are module-private. Members of a public type are
public unless marked `private`. Each directory is a module.

## Running it

### Development

```bash
cp .env.example .env
for f in secrets/*.example; do cp -n "$f" "${f%.example}"; done
# Replace the placeholder hash with a real one (see Configuration):
printf '%s' 'your-admin-password' | argon2 "$(openssl rand -hex 16)" -id -t 3 -m 16 -p 4 -e > secrets/admin_password_hash.txt
docker compose up --detach mail        # optional: Mailpit catches emails at http://localhost:8025
k dev                                  # applies migrations to .k/dev.sqlite, then serves
```

Once `k dev` has created the tables, load the demo catalog from another
terminal: `sqlite3 .k/dev.sqlite < scripts/demo-catalog.sql`.

`k dev` uses SQLite under `.k/` and `LocalCheckout`, which skips Stripe and
sends the customer straight to the receipt. To confirm an order locally, send
a signed webhook as `scripts/e2e.sh` does. Sign in at `/sign-in` as
`admin@shop.example` with the password you hashed.

### Tests

```bash
k test           # every *.test.k file, against in-memory SQLite
k test --watch
```

Tests never touch the network, a mail server or the environment: `k test`
installs hermetic defaults for `Clock`, `JobQueue`, `EmailProvider` and
`HttpClient`, and each test group binds the app's own configs, for example
`use AdminConfig.sample for AdminConfig`. A group writes `use` only to choose
something else, such as `use InlineJobQueue for JobQueue` to run jobs.

`k check` runs the tests too, together with type checking, the formatter
check, and breaking-change detection on `protocol.lock`, `routes.lock` and
`schema.lock`. The compiler generates those lockfiles, and they are
committed.

### Production build

```bash
k build                                  # dist/app
docker compose up --build --wait         # the whole stack, as in production
docker compose exec -T db psql --username=shop --dbname=shop < scripts/demo-catalog.sql
scripts/e2e.sh                           # build, start, exercise over HTTP, tear down
```

The compose stack uses `stripe/stripe-mock` instead of the real Stripe API,
so the production build's `StripeCheckout` runs end to end without an
account. For a real deployment, put real values in `secrets/*.txt` (keep
them out of version control), set `K_PUBLIC_ORIGIN` to the public `https://`
origin, point the `SMTP_*` variables at a real mail server, and remove
`STRIPE_API_URL`.

`scripts/e2e.sh` doesn't read `secrets/`: it generates every secret for its
run, including a random admin password and its hash, and points Compose at
them with an override file.

`dist/app config` lists every variable the build reads.

## Configuration

Values come from environment variables. Secrets can also come from a file
named by `NAME_FILE`, which is how Docker secrets arrive.

| Variable | Required | Read by |
|---|---|---|
| `K_PUBLIC_ORIGIN` | in production | stdlib `ServerConfig`: canonical links and `PublicUrls` (Stripe return URLs, email links) |
| `PORT` | no (`8080`) | stdlib `ServerConfig` |
| `DATABASE_URL`, `DATABASE_PASSWORD[_FILE]` | yes | stdlib `DatabaseConfig` |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_SECURITY`, `MAIL_FROM` | yes | stdlib `SmtpConfig` |
| `STRIPE_WEBHOOK_SECRET[_FILE]` (secret), `STRIPE_WEBHOOK_TOLERANCE` | yes | `payments.StripeWebhookConfig` |
| `STRIPE_API_KEY[_FILE]` (secret), `STRIPE_API_URL`, `STRIPE_API_TIMEOUT` | in production only | `payments.StripeApiConfig` (only `StripeCheckout` injects it) |
| `SHOP_ADMIN_EMAIL`, `SHOP_ADMIN_PASSWORD_HASH[_FILE]` (secret) | yes | `admin.AdminConfig` |

`dist/app` validates all of them at startup and refuses to start, listing
every problem, if any are missing or malformed. Tests never read the
environment.

The committed `secrets/*.example` files hold placeholders, never working
values. In particular, `admin_password_hash.txt.example` isn't a hash at all,
so the app refuses to start until a real one is in place. To make one, hash
the password with Argon2id and store the PHC string (`$argon2id$v=19$…`) in
`secrets/admin_password_hash.txt`, for example with the `argon2` command-line
tool:

```bash
printf '%s' 'your-admin-password' | argon2 "$(openssl rand -hex 16)" -id -t 3 -m 16 -p 4 -e > secrets/admin_password_hash.txt
```

## Language features by file

| File | Shows |
|---|---|
| `App.k` | `app`, `use … for … in .production`, `@Layout`, `@SignInPage(.signIn)`, app `metadata` |
| `store/Product.k`, `store/Order.k` | `entity`, `@Id`, `@Unique`, `@Index`, `@Relation`, an `EmailAddress` field, defaults, a method returning a wire struct |
| `store/ProductRepository.k`, `store/OrderRepository.k` | `repository … for`, awaited lambda queries, `.including`, `.sortedByDescending`, conditional `filter … update` with `and` |
| `store/OrderStatus.k` | an enum stored as its case names, with a computed property using `match` |
| `store/OrderRequest.k` | `Validatable` with `@NotBlank`, `@Range`, a self-validating `EmailAddress` field and custom messages |
| `store/StoreProtocol.k` | `protocol`, `rpc`, typed `throws`, then `requires RateLimit(10.perMinute)` |
| `store/StoreApi.k` | `service … implements Protocol`, `guard let`, `database.transaction`, `PublicUrls`, exhaustive `catch`, compensation on failure |
| `store/CheckoutError.k`, `admin/SignInError.k`, `admin/AdminError.k` | rpc error enums with `case rpcFailure(RpcFailure)` |
| `store/PaymentRecorder.k` | idempotent state changes, `JobQueue.enqueue(OrderConfirmationJob.kind, …)` inside a transaction |
| `store/OrderConfirmationJob.k` | `Job<Payload>`, `@Retry`, `@JobName`, `JobError`, `EmailProvider`, `PublicUrls` |
| `payments/PaymentGateway.k` | an `interface` with two implementations chosen per environment |
| `payments/StripeCheckout.k` | `HttpClient`, `HttpRequest` builder with `.bearerToken(secret)`, form bodies, `catch … where status.isClientError`, `catch let` |
| `payments/StripeSignature.k` | a struct with a `static func`, `match` on strings, `Bytes.parse(hex:)`, async `Hmac` keyed by a `Secret`, constant-time `Mac.matches` |
| `payments/StripeWebhookVerifier.k` | a server-only service injecting a config, `try await … else`, `Currency.parse(code:)`, `@JsonKey` |
| `*Config.k` | `config`, `@Env`, `Secret<String>`, `EmailAddress`, defaults, a constraint on a config field |
| `admin/AdminApi.k`, `admin/StaffId.k` | `SessionManager.signIn` with a typed `AuthUserId`, `Password.verify` against a `Secret` hash, timing-safe sign-in |
| `webhooks/StripeWebhook.k`, `ops/HealthCheck.k` | endpoint pages with fixed URLs and `@Post` / `@Get` handlers taking `IncomingRequest`, typed `HttpStatus` responses |
| `pages/ProductDetail.k` | `@Param`, `@Load`, `@Form`, `bind`, `if` expressions, `loading`, `failed` with `status(.notFound)`, rethrowing `.rpcFailure(.invalid)` to the form, `metadata` with `canonical` and `structuredData` |
| `pages/Admin.k` | `requires Authenticated, Role(.admin)`, a table, conditional classes with `when`, `reload()` |
| `pages/Receipt.k` | `match` in a view, `@Effect` with `Timer` and `onCleanup`, `status(.notFound)` |
| `pages/SignIn.k` | a typed `@Param let returnTo: Page` |
| `pages/Storefront.k`, `pages/ProductCard.k` | a `layout` with `@Slot`, a `component` with `@Prop`, scoped styles and theme tokens |
| `testing/*` | a test-only module: fixtures as static members of public extensions, a Stripe event builder |
| `*.test.k` | `test` groups, `use` overrides (fakes and config values), `expect` and `expect not`, `expect throws` with nested patterns, the `TestClock`, `RecordingEmailProvider` and `RecordingJobQueue` defaults, `InlineJobQueue`, `SessionManager` in tests, `render` with accessibility queries, `TestClient` with `PublicUrls` |

## Design documents used

Besides the core design, this example relies on:

- [App structure](../../docs/design/23-app-structure.md),
  [projects and `k.toml`](../../docs/design/24-projects.md),
  [config and secrets](../../docs/design/25-config-and-secrets.md),
  [auth and sessions](../../docs/design/26-auth-and-sessions.md),
  [endpoint pages](../../docs/design/27-endpoint-pages.md),
  [RPC wire format](../../docs/design/28-rpc-wire-format.md).
- [Background jobs](../../docs/design/30-background-jobs.md),
  [email](../../docs/design/31-email.md),
  [HTTP client](../../docs/design/32-http-client.md),
  [bytes and cryptography](../../docs/design/33-crypto.md),
  [money](../../docs/design/34-money.md),
  [timers and the clock](../../docs/design/35-time.md),
  [deployment](../../docs/design/38-deployment.md),
  [doc comments](../../docs/design/39-doc-comments.md).
- [Persistence](../../docs/design/19-persistence.md) (migrations, writes,
  `schema.lock`), [testing](../../docs/design/21-testing.md) (test doubles and
  defaults), [errors](../../docs/design/09-errors.md) (`catch let`),
  [async data](../../docs/design/14-async-data.md) (`reload()`,
  `status(_:)`), [forms](../../docs/design/15-forms-and-validation.md) and
  [views](../../docs/design/05-views.md) (element accessibility).
