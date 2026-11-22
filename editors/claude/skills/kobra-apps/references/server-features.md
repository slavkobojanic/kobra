# Jobs, email, HTTP, endpoint pages, storage and timers

Sources: `book/apps/jobs.md`, `email.md`, `http.md`, `endpoints.md`, `local-storage.md` and
`time.md`, and the stdlib reference pages `book/reference/stdlib/k-server.md`, `k-http.md`,
`k-storage.md` and `k-time.md`.

## Background jobs

A job is a service that implements `Job<Payload>`, with no new keyword:

```k
import k.server.Email
import k.server.EmailProvider
import k.server.Job
import k.server.JobError
import k.server.JobName
import k.server.Retry
import k.text.EmailAddress

/// Emails the receipt of a paid order.
@Retry(attempts: 8, backoff: 30.seconds)
@JobName("order-confirmation")
public service OrderConfirmationJob implements Job<OrderConfirmation> {
    @Inject let orders: OrderRepository
    @Inject let mailer: EmailProvider

    func run(payload: OrderConfirmation) async throws JobError {
        guard let order = await orders.byId(payload.orderId) else {
            throw JobError.permanent(reason: "Order {payload.orderId} no longer exists")
        }

        guard let to = EmailAddress.parse(order.account.email) else {
            throw JobError.permanent(reason: "The account's address doesn't parse")
        }

        let email = Email(to: to, subject: "Your order is confirmed", text: "Total: {order.total.formatted(as: .currency)}")
            .replyTo(EmailAddress("support@shop.example"))

        do {
            try await mailer.send(email: email)
        } catch .unavailable {
            throw JobError.retryable(reason: "The SMTP server is unavailable")
        } catch .rejected(let reason) {
            throw JobError.permanent(reason: "The SMTP server rejected the message: {reason}")
        } catch .invalidMessage(let reason) {
            throw JobError.permanent(reason: "The message can't be sent as written: {reason}")
        }
    }
}
```

- The payload is `Codable` and stored. Prefer ids to copies of data (`struct OrderConfirmation
  implements Codable, Equatable { let orderId: Uuid }`). Its shape is recorded in `storage.lock`.
- Enqueue with `await jobs.enqueue(OrderConfirmationJob.kind, payload: …)` on an injected
  `JobQueue` (`k.server`). `delay:` is optional. Inside a transaction, the job becomes visible on
  commit. Delivery is at least once.
- `JobError.retryable(reason:)` retries after the `@Retry` backoff, and `.permanent(reason:)` marks the
  job dead. A panic counts as a failed attempt.
- Schedules use `@Schedule(.every(1.hour))`, `.daily(at: TimeOfDay("03:00"))` or `.weekly(on:at:)`:

```k
import k.server.Job
import k.server.JobError
import k.server.Schedule
import k.time.TimeOfDay

/// Runs every night at three.
@Schedule(.daily(at: TimeOfDay("03:00")))
public service PurgeCancelledOrders implements Job<Void> {
    func run(payload: Void) async throws JobError {}
}
```

- The worker runs inside `dist/app serve` and `k dev`. `K_JOB_CONCURRENCY` defaults to 4.
- In tests, `RecordingJobQueue` (the default) stores jobs without running them, and
  `use InlineJobQueue for JobQueue` (`import k.test.InlineJobQueue`) runs them on commit. A test can
  also call `job.run(payload:)` directly.

## Email

`Email(to:subject:text:)` is a `@Builder` (`.html(…)`, `.replyTo(…)`). `EmailProvider.send(email:)`
throws `EmailError` (`.unavailable`, `.rejected(reason:)`, `.invalidMessage(reason:)`). Production
uses SMTP (`SMTP_HOST`, `SMTP_PORT`, `SMTP_SECURITY`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `MAIL_FROM`).
`k dev` catches mail into `.k/mail/` when `SMTP_HOST` is unset, and tests use
`RecordingEmailProvider` (`mailer.sent`, `mailer.simulateFailure(error:)`). `EmailAddress("…")`
checks a literal, and `EmailAddress.parse(text)` parses at runtime.

## Calling HTTP APIs

```k
import k.http.HttpClient
import k.http.HttpRequest

/// Fetches forecasts from a third-party API.
public service ForecastService {
    @Inject let http: HttpClient

    private let api = Url("https://api.open-meteo.com")

    func current(latitude: Double, longitude: Double) async throws ForecastError -> Forecast {
        let request = HttpRequest(method: .get, url: api.appending(path: "/v1/forecast"))
            .query(["latitude": "{latitude}", "longitude": "{longitude}"])
            .timeout(10.seconds)

        do {
            return try await http.fetchJson(request: request)
        } catch .offline {
            throw ForecastError.offline
        } catch .timeout {
            throw ForecastError.timeout
        } catch .status(let status, _) {
            throw ForecastError.upstream(status)
        } catch .decoding(let message) {
            throw ForecastError.invalidResponse(reason: message)
        }
    }
}
```

- Only services inject `HttpClient`. **Map `HttpError` to your own error enum** and drop the third
  party's body (`.status(let status, _)`), especially for errors that reach the browser.
- `HttpRequest` is a `@Builder`: `.headers([…])`, `.query([…])`, `.timeout(…)`,
  `.bearerToken(secret)`, `.secretHeaders(…)` and `.body(.json(v) / .text(s) / .form(map) /
  .bytes(data, contentType:))`. `http.send(request:)` returns an `HttpResponse`.
- `HttpStatus` is an enum (`.ok`, `.notFound`, `.serviceUnavailable`, …), so it works in patterns.
- `@JsonKey("temperature_2m") let temperature: Double` (`k.encoding.JsonKey`) renames a JSON key.
- Raw WebSockets to other servers use `WebSocketClient` (`book/apps/http.md`).

## Endpoint pages (raw HTTP: webhooks, health checks, OAuth callbacks)

```k
import k.crypto.Uuid
import k.http.HttpResponse
import k.http.IncomingRequest

/// Where the payment provider reports a paid order.
public page PaymentWebhook at "/webhooks/payments/{orderId}" {
    @Param let orderId: Uuid

    @Inject let verifier: WebhookVerifier
    @Inject let recorder: PaymentRecorder

    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        guard let token = request.headers["x-webhook-token"], verifier.isAuthentic(token: token) else {
            return HttpResponse(status: .badRequest)
                .body(.text("Webhook verification failed."))
        }

        await recorder.recordPayment(orderId: orderId)

        return HttpResponse(status: .noContent)
    }
}
```

- A page with `@Get`, `@Post`, `@Put`, `@Patch` or `@Delete` handlers and no `view` is an endpoint
  page. **It must have `at "…"`** (K5004). Handlers always have the signature
  `func name(request: IncomingRequest) async -> HttpResponse`.
- Endpoint pages are server-only. They're cases of the generated `Endpoint` enum, not `Page`, so
  `link(to:)` can't name one. Use `urls.absolute(endpoint: .paymentWebhook(orderId: id))`.
- There's no session unless the page injects `Session`. `requires RateLimit(…)` works.
  `Authenticated` and `Role` aren't allowed.
- Responses: `HttpResponse(status: .ok).body(.json(value))`, `HttpResponse.redirect(to: .home)` and
  `HttpResponse.redirect(url: …)`. Cookies are set only through `Session`.
- Request bodies over 1 MB are refused with 413. `request.rawBody` gives the bytes for signature
  checks.

```k
@Get
func check(request: IncomingRequest) async -> HttpResponse {
    guard await readiness.isReady() else {
        return HttpResponse(status: .serviceUnavailable)
            .body(.text("database unavailable"))
    }

    return HttpResponse(status: .ok)
        .body(.text("ok"))
}
```

## Local storage (browser)

`LocalStorage` (`k.storage`) is client-only, and it's typed by `StorageKey<T>`. `read` throws
`StorageReadError` (`.unavailable`, `.unreadable`), and `write` throws `StorageWriteError`
(`.unavailable`, `.quotaExceeded`):

```k
func restore() {
    do {
        notes = try storage.read(key: savedNotes) ?? []
        problem = nil
    } catch .unreadable {
        notes = []
        problem = .unreadable
    } catch .unavailable {
        notes = []
        problem = .unavailable
    }
}
```

A service that injects it is client-only. In a server-rendered app, a page reading it gets the K4013
warning, so use `@Rendering(.client)` on the app or the component. Key names and value shapes are
locked in `storage.lock`. For larger data there are indexed collections (`Collections`,
`Collection<Key, Value>`, `CollectionIndex`); see `book/apps/local-storage.md`.

## Timers and time

```k
import k.time.Clock
import k.time.TimerHandle

/// Rings once, a while after it is started.
public service Reminder {
    @Inject let clock: Clock

    @State var hasRung: Bool = false

    private var timer: TimerHandle? = nil

    func start(after delay: Duration) {
        timer?.cancel()
        timer = clock.after(delay) { hasRung = true }
    }
}
```

`clock.now`, `clock.after(d) { … }`, `clock.every(d) { … }` (both return a `TimerHandle` to
`cancel()`) and `await clock.sleep(d)`. Timers started in an effect go in `onCleanup`. Tests get
`TestClock` (`clock.advance(by: 10.minutes)`).
