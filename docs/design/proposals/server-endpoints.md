# Proposal: Endpoint Pages (Raw HTTP Handlers)

> **Status: proposal.** Not yet accepted into the design. First used by `examples/shop`.

## Problem

[12-routing.md](../12-routing.md) names webhooks and OAuth callbacks as reasons for named URLs (`page StripeWebhook at "/webhooks/stripe"`), but every page described so far renders a `view`. A webhook receives a `POST` with a body that must be read byte for byte (to verify a signature) and answers with a status code, not HTML. Health checks for containers and load balancers need the same. Nothing says how a page does that, which injection rules apply, or how such a page is tested.

## Proposal

### A page with handlers instead of a view

```k
public page StripeWebhook at "/webhooks/stripe" {
    @Inject let verifier: StripeWebhookVerifier
    @Inject let recorder: PaymentRecorder

    @Post
    func receive(request: HttpRequest) async -> HttpResponse {
        let event = try verifier.verify(payload: request.rawBody, signatureHeader: request.headers["stripe-signature"]) else {
            return HttpResponse(status: 400, body: "Webhook signature verification failed.")
        }

        …

        return HttpResponse(status: 204)
    }
}
```

- **A page whose body has handler functions and no `view` is an endpoint page.** A handler is annotated with its HTTP method: `@Get`, `@Post`, `@Put`, `@Patch` or `@Delete`. A page has either a `view` or handlers, never both, and at most one handler per method. A `view` page answers `GET` with HTML, as today.
- **Handlers have one fixed signature:** `func name(request: HttpRequest) async -> HttpResponse`. They **don't throw**: every failure must become a response the handler chose, because the status code is the endpoint's contract with its caller. An unexpected panic is still a `500` ([09-errors.md](../09-errors.md)).
- A request with a method the page doesn't handle gets `405 Method Not Allowed` with an `Allow` header listing the handled methods. `HEAD` follows `GET`.
- **Endpoint pages are server-only.** They are compiled into the server bundle only, never hydrated, and have no `metadata`, `loading`, `failed` or `style`. They are cases of the generated `Page` enum like every page, so `Page.stripeWebhook.absoluteUrl` works ([deployment.md](deployment.md)).
- **An endpoint page may inject server-side services**, including server-only ones (a service that injects a config or a repository). It still can't inject repositories or protocols directly: business logic stays in services, as for every other page ([10-services.md](../10-services.md)).
- **Guards work as on any page.** `requires RateLimit(60.perMinute)` answers `429` with `Retry-After`; `requires Authenticated` answers `401` instead of redirecting, because callers of an endpoint are programs, not people.
- Endpoint pages aren't subject to the session cookie's CSRF protection and receive no `Session` unless they declare `requires Authenticated`: a webhook authenticates its caller itself, by signature.

### The received request

Endpoint handlers receive the `HttpRequest` struct from [http-client.md](http-client.md), filled in from the wire, plus two members that only make sense for a received request:

| Member | Meaning |
|---|---|
| `method`, `url`, `query` | as for an outgoing request; `url` is absolute, built from `K_PUBLIC_ORIGIN` |
| `headers: Map<String, String>` | **names lowercased**, as in HTTP/2, so lookups never depend on the sender's capitalization: `request.headers["stripe-signature"]`. Repeated headers are joined with `, `. |
| `rawBody: Bytes` | **the body exactly as received.** Signature schemes (Stripe, GitHub, Slack) sign these bytes; parsing and re-encoding would change them. |
| `body: HttpBody?` | the body decoded by `Content-Type` (`.json`, `.text`, `.form`), for handlers that don't need the raw bytes |
| `clientAddress: String` | the caller's IP address, from the connection or from trusted proxy headers |

Bodies larger than 1 MB are refused with `413` before the handler runs. Raising the limit per handler is left open.

### Health checks

A health endpoint is an ordinary endpoint page with a named URL, so the `Dockerfile` and orchestrator configuration that depend on it can't drift:

```k
public page HealthCheck at "/health" {
    @Inject let readiness: Readiness

    @Get
    func check(request: HttpRequest) async -> HttpResponse {
        guard await readiness.isReady() else {
            return HttpResponse(status: 503, body: "database unavailable")
        }

        return HttpResponse(status: 200, body: "ok")
    }
}
```

`Database` (from [repository-writes.md](repository-writes.md)) gains `ping(timeout: Duration) async -> Bool`, which runs `SELECT 1` and never throws.

### Testing: `TestClient`

`k.test` provides `TestClient`, which sends requests through the whole server stack in-process: routing, guards, server rendering and endpoint handlers, with no socket.

```k
test StripeWebhook {
    @Inject let client: TestClient

    test "an unsigned request is refused" {
        let request = HttpRequest(method: .post, url: Page.stripeWebhook.absoluteUrl)
            .body(.text(payload))

        let response = await client.send(request: request)

        expect response.status == 400
    }
}
```

- `send(request:)` never throws; transport failures can't happen in-process. The response's header names are lowercased.
- **Requests use the test group's container**, so its `use` rules (fakes, configs, `InlineJobQueue`) apply to the server code that handles them, and the database is the test's rolled-back one.
- **The client keeps cookies across requests in a test**, like one browser. It shares the test's `Session` ([auth-and-sessions.md](auth-and-sessions.md)), so `await session.signIn(userId:roles:)` in the test signs in the client's requests too.
- `TestClient` renders view pages too, which is how a test checks what crawlers receive (`<title>`, the canonical link, structured data), since `render()` doesn't produce document metadata.

## Why

- Making endpoints pages, not a new kind, reuses everything pages already have: named URLs, `routes.lock`, the `Page` enum, guards and the injection rules. A webhook is "a URL something outside depends on", which is exactly what named page URLs are for.
- A non-throwing handler signature puts the choice of status code, which is the endpoint's contract, in plain sight.
- `rawBody` and lowercased header names remove the two most common webhook-verification bugs: verifying a re-serialized body, and a case-sensitive header lookup.
- In-process HTTP tests give full-stack coverage with the speed and isolation of unit tests.

## Open questions

- Streaming request and response bodies (large uploads, server-sent events from an endpoint) are left to [server-streaming.md](server-streaming.md) and a later revision.
- Whether endpoint pages should be able to declare a typed JSON body (`func receive(event: StripeEvent)`) with automatic `400` on decoding failure. Webhooks with signatures need the raw bytes first, so the explicit form comes first.
