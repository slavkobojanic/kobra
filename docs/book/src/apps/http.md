# Calling HTTP APIs

External HTTP APIs are called through the **`HttpClient`** interface in `k.http`, with typed requests,
responses, statuses and errors. Like every platform interface, it is injectable only into services, so
components can't call external APIs directly, and retries, caching and error wrapping stay in one place.

```k
// crates/k_sema/stdlib/k/http/HttpClient.k
public interface HttpClient {
    // ...
    func send(request: HttpRequest) async throws HttpError -> HttpResponse
    // ...
    func fetchJson<T>(request: HttpRequest) async throws HttpError -> T where T implements Codable {
        let response = try await send(request: request)

        guard response.isSuccess else {
            throw HttpError.status(response.status, body: response.body)
        }

        return try response.json()
    }
}
```

- `HttpClient` is usable on both sides. The standard library ships one implementation, `FetchHttpClient`,
  built on the web-standard `fetch` that both the browser and the embedded server runtime provide, so
  `@Inject let http: HttpClient` resolves to it with no `use` rule.
- **Map `HttpError` to cases of your own error enum** rather than exposing it, as every layer boundary
  does ([below](#wrapping-at-the-service-boundary)).

## Requests are a `@Builder` struct

```k
// examples/shop/shop/src/payments/StripeCheckout.k
let call = HttpRequest(method: .post, url: config.apiUrl.appending(path: "/v1/checkout/sessions"))
    .bearerToken(config.apiKey)
    .headers(["idempotency-key": "checkout-{request.orderId}"])
    .body(.form([
        "mode": "payment",
        "client_reference_id": "{request.orderId}",
        // ...
```

| Field | Kind | Default |
|---|---|---|
| `method: HttpMethod` (`.get`, `.post`, `.put`, `.patch`, `.delete`, `.head`) | required | |
| `url: Url` | required | |
| `query: Map<String, String>` | chained | empty |
| `headers: Map<String, String>` | chained | empty |
| `secretHeaders: Map<String, Secret<String>>` | chained | empty |
| `bearerToken: Secret<String>?` | chained; sends `Authorization: Bearer …` | `nil` |
| `body: HttpBody?` | chained | `nil` |
| `timeout: Duration` | chained | `30.seconds` |

- **Secrets go into requests without `reveal()`.** `bearerToken` and `secretHeaders` take `Secret`
  values, which logging and error output redact.
- **`HttpBody`** is `.json(value)`, `.text(string)`, `.form(map)` or `.bytes(data, contentType: …)`. Each
  sets `Content-Type`.
- Write header names in lower case; responses report them lowercased too.

### `Url`

- `Url("https://…")` takes a string literal, validated at compile time. Runtime text is parsed with
  `Url.parse(text) -> Url?`.
- **`url.appending(path:)`** joins a path onto a base URL held in configuration, so a configurable base URL
  never needs string concatenation and re-parsing.
- `url.path` and `url.host` are available, so fakes can route on them.

## Responses

`HttpResponse` is a `@Builder` struct, used both for responses a client receives and for responses an
[endpoint page](endpoints.md) sends:

| Member | Meaning |
|---|---|
| `HttpResponse(status: HttpStatus)` | required status; chained `.headers(…)` and `.body(HttpBody)` |
| `HttpResponse.redirect(to: Page)`, `HttpResponse.redirect(url: Url)` | `303 See Other` |
| `status: HttpStatus` | the status |
| `isSuccess: Bool` | a 2xx status |
| `headers` | names lowercased; `headers.all(name)` returns every value of a repeated header |
| `bodyBytes: Bytes` | the body exactly as received |
| `body: String` | the body as UTF-8 text |
| `json() throws HttpError -> T` | the body decoded as JSON into a `Codable` type |

### `HttpStatus`

- **Statuses are a typed `HttpStatus`,** not a bare `Int`. **`HttpStatus` is an enum** with a case for
  each named status, `.ok`, `.created`, `.noContent`, `.seeOther`, `.badRequest`, `.unauthorized`,
  `.forbidden`, `.notFound`, `.tooManyRequests`, `.internalServerError`, `.serviceUnavailable` and the
  rest, plus **`.other(Int)`** for any other code.
- **Because it is an enum, statuses are patterns:** `case .upstream(.serviceUnavailable)` in a `catch` or
  `match` is valid, and so is `.status(.notFound, _)`.
- `HttpStatus(code: 418)` takes an integer literal, validated at compile time to `100...599`. A code
  with a named case gives that case (`HttpStatus(code: 503)` is `.serviceUnavailable`), and any other
  gives `.other(418)`. The runtime does the same for received statuses, so `.other` never holds a code
  that has a name. Writing `.other(503)` is a compile error (`K2401`), with a quick fix to
  `.serviceUnavailable`.
- `status.code` is the number, and `status.isSuccess`, `status.isClientError` and `status.isServerError`
  classify it.

## Errors

```k
// crates/k_sema/stdlib/k/http/HttpError.k
public enum HttpError implements Error, Codable {
    /// The server couldn't be reached: a DNS failure, a refused
    /// connection or no network.
    case offline
    /// The request's [`HttpRequest.timeout`] elapsed.
    case timeout
    /// A non-2xx response, from [`HttpClient.fetchJson`], with the third
    /// party's body.
    case status(HttpStatus, body: String)
    /// The body wasn't the expected JSON.
    case decoding(message: String)
}
```

- `send` itself never throws `.status`. A non-2xx response is still a response; only `fetchJson` turns it
  into an error.
- **Cancellation isn't an error.** When the effect or `@Load` that started a request is cancelled, the
  request is aborted and the awaiting code never resumes.
- **`.status` carries a third party's response body.** An error that crosses to the browser, such as a
  `@Load` error or an rpc error, shouldn't include it (a warning for one is **Planned**
  error-payload-lint). Wrap the status without the body, as below.

### Wrapping at the service boundary

A service maps each `HttpError` case to a case of its own error enum, keeping only what its callers need:

```k
// examples/weather/src/weather/WeatherError.k
public enum WeatherError implements Error, Codable {
    // ...
    case placeNotFound(id: Int)
    // ...
    case offline
    // ...
    case timeout
    // ...
    case upstream(HttpStatus)
    // ...
}
```

```k
// examples/weather/src/weather/WeatherService.k
private func fetch<T>(url: Url, query: Map<String, String>) async throws WeatherError -> T where T implements Codable {
    let request = HttpRequest(method: .get, url: url)
        .query(query)
        .timeout(requestTimeout)

    do {
        return try await http.fetchJson(request: request)
    } catch .offline {
        throw WeatherError.offline
    } catch .timeout {
        throw WeatherError.timeout
    } catch .status(let status, _) {
        throw WeatherError.upstream(status)
    } catch .decoding(let message) {
        throw WeatherError.invalidResponse(reason: message)
    }
}
```

Callers then match on the status they care about, `catch .upstream(.serviceUnavailable)`, and on
`.offline` and `.timeout`, which are the same situations an rpc reports as `rpcFailure(.offline)` and
`rpcFailure(.timeout)` ([Protocols and RPC](protocols.md#network-failures)). A page can also match
`.upstream(let status) where status.isServerError` in its `failed` block.

## JSON field names

Codable structs map JSON keys to field names one to one. When an external API uses other names, a field
says so with `@JsonKey` (from `k.encoding`):

```k
~import k.encoding.JsonKey
struct CurrentResponse implements Codable {
    @JsonKey("temperature_2m") let temperature: Double
}
```

There is no global naming strategy such as "convert snake_case", so every renamed field is visible where
it is declared.

## Tests

**`k test` installs `TestHttpClient`** (from `k.test`) for `HttpClient` by default, so a test that forgets
a fake never reaches the internet:

| Member | Meaning |
|---|---|
| `serveJson(path:_:)` | answers requests to `path` with a value encoded as JSON, status `.ok` |
| `serveText(path:_:status:)` | answers requests to `path` with text, status `.ok` unless given |
| `respond(path:with:)` | answers requests to `path` with an `HttpResponse` |
| `fail(path:error:)` | makes requests to `path` throw an `HttpError` |
| `requests: List<HttpRequest>` | every request sent so far |

The value and the text are unlabelled: `http.serveText(path: "/v1/forecast", "Service unavailable",
status: .serviceUnavailable)`.

The weather example uses its own fake instead, `FakeHttpClient` (chosen with `use FakeHttpClient for
HttpClient`), whose `serveText` labels the text `body:`:

```k
// examples/weather/src/weather/WeatherService.test.k
test "a server error status is an upstream error" {
    http.serveText(path: "/v1/forecast", body: "Service unavailable", status: .serviceUnavailable)

    expect throws WeatherError.upstream(.serviceUnavailable) {
        try await weather.forecast(place: .berlin, units: .metric)
    }
}
```

Any request with no canned response fails the test, naming the method and URL. An app can still write its
own fake, implementing only `send` (`fetchJson` is a default method, so the real decoding path runs in
tests), and choose it with `use FakeStripeApi for HttpClient`.

## Raw WebSockets

A third party's realtime endpoint (an exchange's price feed, a chat bridge, a device on the local
network) speaks its own protocol over a WebSocket. **`WebSocketClient`** in `k.http` opens one, typed,
next to `HttpClient`. For realtime between your own browser and server, use a
[channel](channels.md) instead: it reconnects, resumes and checks types for you.

```k
// src/prices/PriceFeed.k
import k.http.WebSocketClient
import k.http.WebSocketError
import k.http.WebSocketRequest
import k.time.Backoff
import k.time.Clock

struct Tick implements Codable {
    let symbol: String
    let cents: Int
}

public service PriceFeed {
    @Inject let sockets: WebSocketClient
    @Inject let clock: Clock

    @State var cents: Int? = nil

    func follow() async {
        let backoff = Backoff()

        while true {
            do {
                let socket = try await sockets.connect(request: WebSocketRequest(url: Url("wss://feed.example/v1")).subprotocol("feed.v2"))
                backoff.reset()
                try await socket.send(.text("subscribe BTC"))

                for await message in socket.messages {
                    let tick: Tick = try message.json()
                    cents = tick.cents
                }
            } catch _ {
            }

            await clock.sleep(backoff.next())
        }
    }

    func announce(text: String) async throws WebSocketError {
        let socket = try await sockets.connect(request: WebSocketRequest(url: Url("wss://feed.example/v1")))
        try await socket.send(.text(text))
        socket.close(code: 1000, reason: "done")
    }
}
```

- **`connect(request:)`** waits for the handshake and returns a `WebSocket`: `state` (reactive: `open`,
  `closing`, `closed(code:reason:)`), `messages` (`.text(String)` and `.binary(Bytes)`, ending when the
  socket closes), `send(_:)` and `close(code:reason:)`. `message.json()` decodes a `Codable` value.
- **It throws `WebSocketError`:** `offline`, `timeout`, `refused(status:)` (the server sees the answer
  that didn't upgrade; a browser doesn't say, so there it is `offline`), `tooLarge` (an incoming message
  over the request's `maxMessage`, 1 MB by default, closed the socket) and `closed`.
- **Nothing reconnects by itself**, since the runtime can't know how to resubscribe to someone else's
  protocol. `Backoff` in `k.time` gives a loop the delays channels use (a random wait up to 0.5 s, then
  1 s, 2 s and so on, up to 30 s), and sleeping on the injected `Clock` lets `TestClock` drive them.
- **A socket belongs to the run that opened it:** leaving the page, or an effect re-running, closes it.
- **Handshake headers are server-only.** A browser can't send headers on a WebSocket handshake, so
  `headers`, `secretHeaders` and `bearerToken` of `WebSocketRequest` are a compile error (K4031) in code
  that may run in the browser. A feed that needs a token is opened by a server-only service, which can
  publish what it receives to a `Broadcast` that the browser's channels or streams follow; the token
  never reaches the browser.

**In `k test`, `TestWebSocketClient` stands in** and connects only to endpoints the test scripted with a
`TestWebSocketServer`, which plays the third party. One double serves the whole test, the browser and the
server, so a test also scripts what a server service connects to:

```k
// src/prices/PriceFeed.test.k
import k.http.WebSocketError
import k.test.TestWebSocketClient
import k.test.TestWebSocketServer

test PriceFeed {
    @Inject let sockets: TestWebSocketClient
    @Inject let feed: PriceFeed

    test "what the service sends reaches the endpoint" {
        let endpoint = TestWebSocketServer()
        sockets.serve(url: Url("wss://feed.example/v1"), with: endpoint)

        try await feed.announce(text: "hello")

        expect endpoint.received == [.text("hello")]
        expect endpoint.connections == 0
    }

    test "a refused handshake is thrown" {
        sockets.fail(url: Url("wss://feed.example/v1"), error: .refused(status: .forbidden))

        expect throws WebSocketError.refused {
            try await feed.announce(text: "hello")
        }
    }
}
```

`endpoint.send(_:)` plays the third party sending to every open connection,
`endpoint.close(code:reason:)` closes them, and `sockets.fail(url:error:)` makes connecting throw. A connection to a URL nobody scripted
fails the test, naming the URL.

