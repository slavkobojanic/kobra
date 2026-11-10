# Endpoint Pages

## Decision

**An endpoint page is a page with HTTP handlers instead of a `view`.** It is K's one deliberate escape hatch to raw HTTP, and it exists **only for external callers** that can't speak K's RPC: webhooks, OAuth callbacks, health checks. The app's own client always uses RPC ([11-protocols-and-rpc.md](11-protocols-and-rpc.md)), so developers otherwise never see HTTP.

**Endpoint pages are allowed only with a fixed `at "…"` URL,** because the URL is a contract with the outside system.

```k
public page StripeWebhook at "/webhooks/stripe" {
    @Inject let verifier: StripeWebhookVerifier
    @Inject let recorder: PaymentRecorder

    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        let event = try await verifier.verify(payload: request.rawBody, signatureHeader: request.headers["stripe-signature"]) else {
            return HttpResponse(status: .badRequest)
                .body(.text("Webhook signature verification failed."))
        }

        await recorder.apply(event: event)

        return HttpResponse(status: .noContent)
    }
}
```

## Rules

- **A page whose body has handler functions and no `view` is an endpoint page.** A handler is annotated with its HTTP method: `@Get`, `@Post`, `@Put`, `@Patch` or `@Delete`. A page has either a `view` or handlers, never both, and at most one handler per method.
- **An endpoint page must have an `at` clause.** Without one, it is a compile error, with a quick fix that inserts `at "/<kebab-name>"` (`StripeWebhook` → `at "/stripe-webhook"`). A derived URL would move when the page is renamed, which is exactly the drift a named URL prevents ([12-routing.md](12-routing.md#named-urls)).
- **Path parameters are allowed** in the template as typed `@Param` fields: `at "/webhooks/{tenant}"` with `@Param let tenant: TenantId`. A value that fails to parse gives `404`. Every `@Param` of an endpoint page must appear in the template; query-string values are read from `request.query`.
- **Handlers have one fixed signature:** `func name(request: IncomingRequest) async -> HttpResponse`. They **don't throw**: every failure must become a response the handler chose, because the status code is the endpoint's contract with its caller. An unexpected panic is still a `500` ([09-errors.md](09-errors.md#panics)).
- A request with a method the page doesn't handle gets `405 Method Not Allowed` with an `Allow` header. `HEAD` follows `GET`.
- **Request bodies larger than 1 MB are refused with `413`** before the handler runs.

### Server-only, and not a `Page`

- **Endpoint pages are server-only.** They are compiled into the server bundle only, never hydrated, and have no `metadata`, `loading`, `failed` or `style`. App and layout defaults don't apply: `@Layout` or `@NoLayout` on an endpoint page is a compile error ([23-app-structure.md](23-app-structure.md)).
- **They are cases of the generated, server-only `Endpoint` enum, not of `Page`.** `link(to:)`, `Router.navigate(to:)`, `@SignInPage` and `Page`-typed parameters therefore can't name one, by type, and a crafted `?returnTo=stripeWebhook` can't decode to one. `Endpoint.stripeWebhook.url` is the path, and `urls.absolute(endpoint: .stripeWebhook)` the absolute URL ([12-routing.md](12-routing.md#absolute-urls-publicurls)).
- Endpoint URLs are recorded in `routes.lock` and listed by `k routes`, like every page.

### Injection

- **An endpoint page may inject server-side services,** including server-only ones (a service that injects a config or a repository). It can't inject repositories or protocols directly: business logic stays in services, as for every other page ([10-services.md](10-services.md#layering)).

### Guards

- **`requires RateLimit(…)` works,** answering `429` with `Retry-After`.
- **`requires Authenticated` and `requires Role(…)` on an endpoint page are compile errors.** Callers of an endpoint are programs, and they authenticate by signature or token, checked in the handler (with `Mac.matches` or `Secret.matches`, [33-crypto.md](33-crypto.md)).

### Sessions

- **An endpoint page receives no session by default.** The cookie is ignored, and no CSRF check is needed, which is what webhooks and health checks want.
- **An endpoint page that needs the session says so with `@Inject let session: Session`.** It then receives the cookie session, guest or signed in, so an OAuth callback can sign the user in through a server-side service that uses `SessionManager`. Such a page is subject to the request-forgery checks in [26-auth-and-sessions.md](26-auth-and-sessions.md#request-forgery); there is no exemption.
- If a page's dependency graph reaches `Session` or `SessionManager` without the page itself injecting `Session`, it is a compile error with a quick fix that adds the field, so the use of cookies is visible on the page.
- Responses of an endpoint page that uses the session carry `Cache-Control: private, no-store`.

## The incoming request

Handlers receive an **`IncomingRequest`**, a read-only type distinct from the outgoing `HttpRequest` ([32-http-client.md](32-http-client.md)):

| Member | Meaning |
|---|---|
| `method: HttpMethod` | the request's method |
| `url: Url` | the absolute URL, built from `K_PUBLIC_ORIGIN` |
| `query: Map<String, String>` | the query string |
| `headers` | **names lowercased,** so lookups never depend on the sender's capitalization: `request.headers["stripe-signature"]`. A repeated header reads as its values joined with `, `; `headers.all(name)` returns each value. |
| `rawBody: Bytes` | **the body exactly as received.** Signature schemes (Stripe, GitHub, Slack) sign these bytes; parsing and re-encoding would change them. |
| `text() -> String?` | the body as UTF-8, or `nil` if it isn't valid UTF-8 |
| `json() throws DecodingError -> T` | the body decoded as JSON into a `Codable` type, written out or given by the binding: `let event = try request.json<StripeEvent>()` |
| `clientAddress: String` | the caller's IP address (below) |

### Client address and proxies

- **`clientAddress` is the right-most `X-Forwarded-For` hop that isn't a trusted proxy,** where trusted proxies are the CIDR ranges in `K_TRUSTED_PROXIES` ([25-config-and-secrets.md](25-config-and-secrets.md#stdlib-configs)). With the list empty (the default), it is the socket's address and forwarding headers are ignored, so a client can't choose its own address by sending a header.
- `dist/app serve` logs a warning the first time a request arrives with `X-Forwarded-For` while the list is empty, because every user behind the proxy would otherwise share one address and one rate limit ([38-deployment.md](38-deployment.md#behind-a-proxy)).

## Responses

`HttpResponse` and `HttpStatus` are the types from [32-http-client.md](32-http-client.md#responses):

```k
HttpResponse(status: .noContent)

HttpResponse(status: .ok)
    .body(.json(summary))                       // sets Content-Type: application/json

HttpResponse(status: .serviceUnavailable)
    .body(.text("database unavailable"))

HttpResponse.redirect(to: .home)                // 303 See Other to the page's URL
HttpResponse.redirect(url: providerAuthorizeUrl)
```

- **Cookies are set only through `Session`,** never as raw headers. Setting `set-cookie` on a response is a compile error for a literal header name and a panic otherwise.

## Health checks

A health endpoint is an ordinary endpoint page, so the `Dockerfile` and orchestrator configuration that depend on its URL can't drift:

```k
public page HealthCheck at "/health" {
    @Inject let readiness: Readiness

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

service Readiness {
    @Inject let database: Database

    func isReady() async -> Bool {
        return await database.ping(timeout: 2.seconds)
    }
}
```

`dist/app probe http://127.0.0.1:8080/health` checks it from inside a container that has no shell ([38-deployment.md](38-deployment.md#commands)).

## Tests

Endpoint pages are tested with `TestClient`, which sends an `HttpRequest` through the whole server stack in-process, converted to the `IncomingRequest` the handler receives, as the wire would ([21-testing.md](21-testing.md#http-level-tests-testclient)).

## Why

- Making endpoints pages, not a new kind, reuses what pages already have: named URLs, `routes.lock`, guards and the injection rules. A webhook is "a URL something outside depends on", which is what named URLs are for.
- A separate `Endpoint` enum keeps server-only routes out of the client's typed navigation, by type rather than by a check.
- No session by default means a webhook can't be the target of a cookie-based forgery; opting in is visible and brings the checks with it.
- A non-throwing handler signature puts the choice of status code, which is the endpoint's contract, in plain sight.
- `rawBody` and lowercased header names remove the two most common webhook-verification bugs: verifying a re-serialized body, and a case-sensitive header lookup.

## Open questions

- Streaming request and response bodies (large uploads, server-sent events from an endpoint).
- Raising the 1 MB body limit per handler.
- Whether a handler should be able to take a typed JSON body (`func receive(event: StripeEvent)`) with an automatic `400` on decoding failure. Webhooks with signatures need the raw bytes first, so the explicit form comes first.
