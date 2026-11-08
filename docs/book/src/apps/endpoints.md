# Endpoint pages

**An endpoint page is a page with HTTP handlers instead of a `view`.** It is K's one deliberate escape
hatch to raw HTTP, and it exists **only for external callers** that can't speak K's RPC: webhooks, OAuth
callbacks and health checks. The app's own client always uses [RPC](protocols.md), so you otherwise never
see HTTP.

**Endpoint pages are allowed only with a fixed `at "…"` URL,** because the URL is a contract with the
outside system.

```k
// examples/shop/shop/src/webhooks/StripeWebhook.k
public page StripeWebhook at "/webhooks/stripe" {
    @Inject let verifier: StripeEvents
    @Inject let recorder: PaymentRecorder
    // ...
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        let signature = request.headers["stripe-signature"]

        do {
            let event = try await verifier.verify(payload: request.rawBody, signatureHeader: signature)

            await apply(event: event)
        } catch .disabled {
            return HttpResponse(status: .notFound)
                .body(.text("This shop doesn't take payments through Stripe."))
        } catch let error {
            Log.debug("Refused a Stripe webhook delivery: {error}")

            return HttpResponse(status: .badRequest)
                .body(.text("Webhook signature verification failed."))
        }

        return HttpResponse(status: .noContent)
    }
    // ...
}
```

## Rules

- **A page whose body has handler functions and no `view` is an endpoint page.** A handler is annotated
  with its HTTP method: `@Get`, `@Post`, `@Put`, `@Patch` or `@Delete`. A page has either a `view` or
  handlers, never both, and at most one handler per method.
- **An endpoint page must have an `at` clause.** Without one, it is a compile error with a quick fix that
  inserts `at "/<kebab-name>"`. A derived URL would move when the page is renamed.
- **Path parameters** are allowed in the template as typed `@Param` fields: `at "/webhooks/{tenant}"` with
  `@Param let tenant: TenantId`. A value that fails to parse gives `404`. Every `@Param` must appear in the
  template; query-string values are read from `request.query`.
- **Handlers have one fixed signature:** `func name(request: IncomingRequest) async -> HttpResponse`.
  **They don't throw.** Every failure must become a response the handler chose, because the status code
  is the endpoint's contract with its caller. That is why the webhook above uses `try … else` to turn a
  failed verification into a `400`. An unexpected panic is still a `500`.
- A request with a method the page doesn't handle gets `405 Method Not Allowed`, with an `Allow` header.
  `HEAD` follows `GET`.
- **Request bodies larger than 1 MB are refused with `413`** before the handler runs.

### Server-only, and not a `Page`

- **Endpoint pages are server-only.** They are compiled into the server bundle only, never hydrated, and
  have no `metadata`, `loading`, `failed` or `style`. `@Layout` or `@NoLayout` on one is a compile error.
- **They are cases of the generated, server-only `Endpoint` enum, not of `Page`.** `link(to:)`,
  `Router.navigate(to:)`, `@SignInPage` and `Page`-typed parameters therefore can't name one, by type, and
  a crafted `?returnTo=stripeWebhook` can't decode to one. `Endpoint.stripeWebhook.url` is the path, and
  `urls.absolute(endpoint: .stripeWebhook)` the absolute URL, from
  [`PublicUrls`](routing.md#absolute-urls-publicurls).
- Endpoint URLs are recorded in `routes.lock` (see [Project files](../reference/project-files.md#lockfiles))
  and listed by `k routes`, like every page. An endpoint whose recorded URL is no longer served fails
  `k check`.

### Injection and guards

- **An endpoint page may inject server-side services,** including server-only ones (a service that
  injects a config or a repository). It can't inject repositories or protocols directly: business logic
  stays in services.
- **`requires RateLimit(…)` works,** answering `429` with `Retry-After`.
- **`requires Authenticated` and `requires Role(…)` are compile errors on an endpoint page.** Callers of
  an endpoint are programs, and they authenticate by signature or token, checked in the handler with
  `Mac.matches` or `Secret.matches` ([Bytes and cryptography](crypto.md)).

## Sessions

- **An endpoint page receives no session by default.** The cookie is ignored and no forgery check is
  needed, which is what webhooks and health checks want.
- **An endpoint page that needs the session says so with `@Inject let session: Session`.** It then
  receives the cookie session, guest or signed in, so an OAuth callback can sign the user in through a
  server-side service that uses `SessionManager`. Such a page is subject to the
  [request-forgery checks](auth.md#request-forgery), with no exemption, and its responses carry
  `Cache-Control: private, no-store`.
- If a page's dependency graph reaches `Session` or `SessionManager` without the page itself injecting
  `Session`, it is a compile error with a quick fix that adds the field, so the use of cookies is visible
  on the page.

## The incoming request

Handlers receive an **`IncomingRequest`**, a read-only type distinct from the outgoing `HttpRequest` of
the [HTTP client](http.md):

| Member | Meaning |
|---|---|
| `method: HttpMethod` | the request's method |
| `url: Url` | the absolute URL, built from `K_PUBLIC_ORIGIN` |
| `query: Map<String, String>` | the query string |
| `headers` | **names lowercased,** so lookups never depend on the sender's capitalization: `request.headers["stripe-signature"]`. `headers.all(name)` returns each value of a repeated header. |
| `rawBody: Bytes` | **the body exactly as received.** Signature schemes (Stripe, GitHub, Slack) sign these bytes; re-encoding would change them. |
| `text() -> String?` | the body as UTF-8, or `nil` if it isn't valid UTF-8 |
| `json() throws DecodingError -> T` | the body decoded as JSON into a `Codable` type, given by the binding |
| `clientAddress: String` | the caller's IP address |

**`clientAddress` is the right-most `X-Forwarded-For` hop that isn't a trusted proxy,** where trusted
proxies are the CIDR ranges in `K_TRUSTED_PROXIES`. With the list empty, the default, it is the socket's
address and forwarding headers are ignored, so a client can't choose its own address by sending a header
(see [Deployment](deployment.md#behind-a-proxy)).

## Responses

`HttpResponse` and `HttpStatus` are the same types the HTTP client uses:

```k
~import k.http.HttpResponse
~
~struct Summary implements Codable {
~    let count: Int
~}
~
~page Home at "/" {
~    view {
~        text { "Home" }
~    }
~}
~
~func responses(summary: Summary, providerAuthorizeUrl: Url) -> List<HttpResponse> {
~    return [
HttpResponse(status: .noContent)
~,

HttpResponse(status: .ok)
    .body(.json(summary))                       // sets Content-Type: application/json
~,

HttpResponse(status: .serviceUnavailable)
    .body(.text("database unavailable"))
~,

HttpResponse.redirect(to: .home)                // 303 See Other to the page's URL
~,
HttpResponse.redirect(url: providerAuthorizeUrl)
~    ]
~}
```

- Statuses are a typed `HttpStatus` with implicit members for the standard codes (`.ok`, `.badRequest`,
  `.notFound`, `.serviceUnavailable`, …), never a bare number.
- **Cookies are set only through `Session`,** never as raw headers. Setting `set-cookie` on a response is
  a compile error for a literal header name, in any case (`K5013`), and a panic otherwise.

## Health checks

A health endpoint is an ordinary endpoint page, so the `Dockerfile` and orchestrator configuration that
depend on its URL can't drift:

```k
// examples/shop/shop/src/ops/HealthCheck.k
public page HealthCheck at "/health" {
    @Inject let readiness: Readiness
    // ...
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        guard await readiness.isReady() else {
            return HttpResponse(status: .serviceUnavailable)
                .body(.text("database unavailable"))
        }

        return HttpResponse(status: .ok)
            .body(.text("ok"))
    }
}
// ...
service Readiness {
    @Inject let database: Database

    func isReady() async -> Bool {
        return await database.ping(timeout: 2.seconds)
    }
}
```

`dist/app probe http://127.0.0.1:8080/health` checks it from inside a container that has no shell (see
[Deployment](deployment.md#commands)).

## Tests

Endpoint pages are tested with [`TestClient`](testing.md#http-level-tests-testclient), which sends an
`HttpRequest` through the whole server stack in-process, converted to the `IncomingRequest` the handler
receives, as the wire would:

```k
// examples/shop/shop/src/webhooks/StripeWebhook.test.k
test "an unsigned request is refused and changes nothing" {
    let order = try await placeOrder()
    let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)

    let response = await deliver(payload: payload, signature: nil)

    expect response.status == .badRequest
    expect await orders.byId(order.id)?.status == .awaitingPayment
    expect mailer.sent.isEmpty
}
```

`deliver` is a helper of the test group: it builds an `HttpRequest` to
`urls.absolute(endpoint: .stripeWebhook)` and sends it with the injected `TestClient`.
