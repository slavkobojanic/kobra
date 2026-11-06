# K Shop

A small, production-style shop written entirely in K. Customers browse
products, check out as guests, and pay through Stripe Checkout. A Stripe
webhook confirms the payment, and a background job emails a confirmation over
SMTP. Staff sign in to an admin page that lists orders. The whole app ships
as one executable, `dist/app`, in a minimal container next to Postgres.

> The K compiler doesn't exist yet. This example was written by hand against
> the design in [`docs/design`](../../docs/design) and the proposals it relies
> on (see [Design proposals used](#design-proposals-used)). The commands below
> describe how it will run.

## What it demonstrates

| Feature | Where |
|---|---|
| `Dockerfile` that runs `k check` and `k build`, then copies only `dist/app` into `distroless/cc` | [`Dockerfile`](Dockerfile) |
| Compose with Postgres, health checks, a named volume and Docker secrets | [`docker-compose.yml`](docker-compose.yml), [`secrets/`](secrets) |
| Type-safe configuration and secrets, injected only into server-only services | [`StripeWebhookConfig.k`](src/payments/StripeWebhookConfig.k), [`StripeApiConfig.k`](src/payments/StripeApiConfig.k), [`AdminConfig.k`](src/admin/AdminConfig.k) |
| Entities, repositories, conditional updates and transactions on Postgres | [`src/store`](src/store) |
| Migrations, including seed data | [`migrations/`](migrations) |
| A webhook at a fixed URL, `page StripeWebhook at "/webhooks/stripe"`, verified with HMAC-SHA256 from `k.crypto` (no vendor SDK) | [`StripeWebhook.k`](src/webhooks/StripeWebhook.k), [`StripeSignature.k`](src/payments/StripeSignature.k), [`StripeWebhookVerifier.k`](src/payments/StripeWebhookVerifier.k) |
| `requires Authenticated, Role(.admin)` on a page and on an rpc | [`Admin.k`](src/pages/Admin.k), [`AdminProtocol.k`](src/admin/AdminProtocol.k) |
| `RateLimit` on rpcs (placing orders, signing in) | [`StoreProtocol.k`](src/store/StoreProtocol.k), [`AdminProtocol.k`](src/admin/AdminProtocol.k) |
| A background job sending email through `k.server`'s SMTP provider | [`OrderConfirmationJob.k`](src/store/OrderConfirmationJob.k) |
| SEO metadata: `canonical`, `structuredData`, `socialPreview` | [`ProductDetail.k`](src/pages/ProductDetail.k), [`Home.k`](src/pages/Home.k) |
| `use X for Y in .environment` | [`App.k`](App.k) |
| Unit, integration and HTTP-level tests next to the source | `*.test.k` |
| An end-to-end test with Docker Compose and `curl` | [`scripts/e2e.sh`](scripts/e2e.sh) |

## How a purchase flows

1. `ProductDetail` submits an `@Form` of `OrderRequest` to `StoreService`,
   which calls `StoreProtocol.placeOrder`. The rpc is rate limited to 10 a
   minute per client, and the request is validated on the server too.
2. `StoreApi` reserves stock and records the order in one transaction, using
   a conditional `UPDATE` so the last unit can't be sold twice. It then asks
   the `PaymentGateway` for a checkout URL. If the gateway fails, the order
   is cancelled and the stock released.
3. In production, `StripeCheckout` creates a Stripe Checkout Session over
   HTTP, using the order id as `client_reference_id` and as the idempotency
   key. The customer pays on Stripe's page.
4. Stripe calls `/webhooks/stripe`. `StripeWebhookVerifier` checks the
   `Stripe-Signature` header (HMAC-SHA256 over `"{timestamp}.{raw body}"`,
   constant-time comparison, five-minute replay window) before reading the
   JSON.
5. `PaymentRecorder` checks that the amount matches the order total, marks
   the order paid with a conditional update, and enqueues
   `OrderConfirmationJob` in the same transaction. A repeated webhook delivery
   changes nothing and sends no second email.
6. The worker inside `dist/app` runs the job, which emails the customer
   through SMTP, retrying with backoff when the server is unavailable.
7. The customer's `Receipt` page polls with `reload()` until the payment is
   confirmed.

## Structure

```
App.k                         the app: per-environment payment gateway, layout, sign-in page
k.toml                        manifest
migrations/                   0001 creates the tables, 0002 seeds three demo products
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
                              AdminService, StaffRole, AdminConfig
  webhooks/StripeWebhook.k    the endpoint page Stripe calls
  ops/HealthCheck.k           GET /health for Docker health checks
  pages/                      Storefront layout, Home, ProductDetail, Receipt, SignIn,
                              Admin, and the ProductCard component
  testing/                    test-only module: RecordingMailer, Stripe event builder,
                              shared samples
scripts/e2e.sh                end-to-end test
secrets/*.example             example Docker secrets; real ones are git-ignored
```

Every file holds one public declaration named after the file; helpers in the
same file are module-private. Each directory is a module.

## Running it

### Development

```bash
cp .env.example .env
for f in secrets/*.example; do cp -n "$f" "${f%.example}"; done
docker compose up --detach mail        # optional: Mailpit catches emails at http://localhost:8025
k dev
```

`k dev` uses SQLite under `.k/` and `LocalCheckout`, which skips Stripe and
sends the customer straight to the receipt. To confirm an order locally, send
a signed webhook as `scripts/e2e.sh` does. Sign in at `/sign-in` as
`admin@shop.example` with the password `change-me-admin-password`.

### Tests

```bash
k test           # every *.test.k file, against in-memory SQLite
k test --watch
```

`k check` runs the tests too, together with type checking, the formatter
check, and breaking-change detection on `protocol.lock`, `routes.lock` and
`schema.lock`. The compiler generates those lockfiles on the first build, and
they are committed.

### Production build

```bash
k build                                  # dist/app
docker compose up --build --wait         # the whole stack, as in production
scripts/e2e.sh                           # build, start, exercise over HTTP, tear down
```

The compose stack uses `stripe/stripe-mock` instead of the real Stripe API,
so the production build's `StripeCheckout` runs end to end without an
account. For a real deployment, put real values in `secrets/*.txt` (keep
them out of version control), set `K_PUBLIC_ORIGIN` to the public `https://`
origin, point the `SMTP_*` variables at a real mail server, and remove
`STRIPE_API_URL`.

`dist/app config` lists every variable the build reads.

## Configuration

Values come from environment variables. Secrets can also come from a file
named by `NAME_FILE`, which is how Docker secrets arrive.

| Variable | Required | Read by |
|---|---|---|
| `K_PUBLIC_ORIGIN` | in production | stdlib `ServerConfig`: canonical links, Stripe return URLs, email links |
| `PORT` | no (`8080`) | stdlib `ServerConfig` |
| `DATABASE_URL`, `DATABASE_PASSWORD[_FILE]` | yes | stdlib `DatabaseConfig` |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_SECURITY`, `MAIL_FROM` | yes | stdlib `SmtpConfig` |
| `STRIPE_WEBHOOK_SECRET[_FILE]` (secret), `STRIPE_WEBHOOK_TOLERANCE` | yes | `payments.StripeWebhookConfig` |
| `STRIPE_API_KEY[_FILE]` (secret), `STRIPE_API_URL`, `STRIPE_API_TIMEOUT` | in production only | `payments.StripeApiConfig` (only `StripeCheckout` injects it) |
| `SHOP_ADMIN_EMAIL`, `SHOP_ADMIN_PASSWORD_HASH[_FILE]` (secret) | yes | `admin.AdminConfig` |

`dist/app` validates all of them at startup and refuses to start, listing
every problem, if any are missing or malformed. Tests never read the
environment: each test group binds the configs it needs with
`use AdminConfig.sample for AdminConfig`.

To make an admin password hash, hash it with Argon2id and store the PHC
string (`$argon2id$v=19$…`) in `secrets/admin_password_hash.txt`. The example
file holds the hash of `change-me-admin-password`.

## Language features by file

| File | Shows |
|---|---|
| `App.k` | `app`, `use … for … in .production`, `@Layout`, `@SignInPage`, app `metadata` |
| `store/Product.k`, `store/Order.k` | `entity`, `@Id`, `@Unique`, `@Relation`, defaults, a method returning a wire struct |
| `store/ProductRepository.k`, `store/OrderRepository.k` | `repository … for`, lambda queries, `.including`, `.sortedByDescending`, conditional `filter … update` |
| `store/OrderStatus.k` | an enum with a computed property using `match` |
| `store/OrderRequest.k` | `Validatable` with `@NotBlank`, `@Range`, `@Email` and custom messages |
| `store/StoreProtocol.k` | `protocol`, `rpc`, `requires RateLimit(10.perMinute)`, typed `throws` |
| `store/StoreApi.k` | `service … implements Protocol`, `guard let`, `database.transaction`, exhaustive `catch`, compensation on failure |
| `store/CheckoutError.k`, `admin/AdminError.k` | error enums with `case rejected(GuardRejection)` |
| `store/PaymentRecorder.k` | idempotent state changes, enqueueing a job inside a transaction |
| `store/OrderConfirmationJob.k` | `Job<Payload>`, `@Retry`, `JobError`, `EmailProvider`, `Page.receipt(id:).absoluteUrl` |
| `payments/PaymentGateway.k` | an `interface` with two implementations chosen per environment |
| `payments/StripeCheckout.k` | `HttpClient`, `HttpRequest` builder, form bodies, `catch … where`, `catch let` |
| `payments/StripeSignature.k` | a pure struct, `static func`, `match` on strings, `Hmac`, constant-time comparison |
| `payments/StripeWebhookVerifier.k` | a server-only service injecting a config, `Secret.reveal()`, `try … else`, `@JsonKey` |
| `*Config.k` | `config`, `@Env`, `Secret<String>`, defaults, constraints on config fields |
| `admin/AdminApi.k` | `Session.signIn`, `Password.verify`, timing-safe sign-in |
| `webhooks/StripeWebhook.k`, `ops/HealthCheck.k` | endpoint pages with `@Post` / `@Get` handlers and named URLs |
| `pages/ProductDetail.k` | `@Param`, `@Load`, `@Form`, `bind`, `if` expressions, `loading`, `failed`, `metadata` with `canonical` and `structuredData` |
| `pages/Admin.k` | `requires Authenticated, Role(.admin)`, a table, conditional classes with `when`, `reload()` |
| `pages/Receipt.k` | `match` in a view, `@Effect` with `Timer` and `onCleanup` |
| `pages/SignIn.k` | a typed `@Param let returnTo: Page` |
| `pages/Storefront.k`, `pages/ProductCard.k` | a `layout` with `@Slot`, a `component` with `@Prop`, scoped styles and theme tokens |
| `testing/*` | a test-only module: fakes, fixtures as static extension members |
| `*.test.k` | `test` groups, `use` overrides (fakes and config values), `expect`, `expect throws` with nested patterns, `TestClock`, `InlineJobQueue`, `render` with accessibility queries, `TestClient` |

## Design proposals used

The design leaves several topics open. This example uses these proposals,
all marked *Status: proposal*:

- Written for this example: [config and secrets](../../docs/design/proposals/config-and-secrets.md),
  [endpoint pages](../../docs/design/proposals/server-endpoints.md),
  [background jobs](../../docs/design/proposals/background-jobs.md),
  [email](../../docs/design/proposals/email.md),
  [crypto](../../docs/design/proposals/crypto.md),
  [money](../../docs/design/proposals/money.md),
  [RPC wire format](../../docs/design/proposals/rpc-wire-format.md),
  [deployment](../../docs/design/proposals/deployment.md).
- Shared with other examples, and extended here: [App.k](../../docs/design/proposals/app-structure.md),
  [k.toml](../../docs/design/proposals/k-toml.md),
  [HttpClient](../../docs/design/proposals/http-client.md),
  [testing support](../../docs/design/proposals/testing-support.md).
- Shared with other examples: [auth and sessions](../../docs/design/proposals/auth-and-sessions.md),
  [migrations](../../docs/design/proposals/migrations.md),
  [repository writes](../../docs/design/proposals/repository-writes.md),
  [doc comments](../../docs/design/proposals/doc-comments.md),
  [catch binding](../../docs/design/proposals/catch-binding.md),
  [timers and clock](../../docs/design/proposals/timers-and-clock.md),
  [reloading loads](../../docs/design/proposals/load-reload.md),
  [element accessibility](../../docs/design/proposals/element-accessibility.md).
