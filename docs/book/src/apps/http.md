# Calling HTTP APIs

External HTTP APIs are called through the **`HttpClient`** interface in `k.http`, with typed requests,
responses, statuses and errors. Like every platform interface, it is injectable only into services, so
components can't call external APIs directly, and retries, caching and error wrapping stay in one place.

```k
public interface HttpClient {
    func send(request: HttpRequest) async throws HttpError -> HttpResponse

    /// Sends the request, requires a 2xx status, and decodes the body as JSON.
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
- **Wrap `HttpError` in your own error enum** rather than exposing it, as every layer boundary does.

## Requests are a `@Builder` struct

```k
let request = HttpRequest(method: .post, url: config.apiUrl.appending(path: "/v1/checkout/sessions"))
    .bearerToken(config.apiKey)
    .headers(["idempotency-key": "checkout-{order.id}"])
    .body(.form(["mode": "payment", "client_reference_id": "{order.id}"]))
    .timeout(10.seconds)
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

- **Statuses are a typed `HttpStatus`,** not a bare `Int`, with implicit members for the standard codes:
  `.ok`, `.created`, `.noContent`, `.seeOther`, `.badRequest`, `.unauthorized`, `.forbidden`,
  `.notFound`, `.tooManyRequests`, `.internalServerError`, `.serviceUnavailable`, and the rest.
- `HttpStatus(code: 418)` takes an integer literal, validated at compile time to `100...599`.
  `status.code` is the number, and `status.isSuccess`, `status.isClientError` and `status.isServerError`
  classify it.

## Errors

```k
public enum HttpError implements Error {
    case offline                                // DNS failure, refused connection, no network
    case timeout                                // the request's timeout elapsed
    case status(HttpStatus, body: String)       // a non-2xx response, from fetchJson
    case decoding(message: String)              // the body wasn't the expected JSON
}
```

- `send` itself never throws `.status`. A non-2xx response is still a response; only `fetchJson` turns it
  into an error.
- **Cancellation isn't an error.** When the effect or `@Load` that started a request is cancelled, the
  request is aborted and the awaiting code never resumes.
- **`.status` carries a third party's response body.** An error that crosses to the browser, such as a
  `@Load` error or an rpc error, shouldn't include it, and the compiler warns when it would. Wrap the
  status without the body:

```k
public enum WeatherError implements Error {
    case placeNotFound
    case offline
    case timeout
    case upstream(HttpStatus)
    case invalidResponse(reason: String)
}

private func fetch<T>(url: Url, query: Map<String, String>) async throws WeatherError -> T where T implements Codable {
    do {
        return try await http.fetchJson(request: HttpRequest(method: .get, url: url).query(query))
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

A page can then match `.upstream(let status) where status.isServerError` in its `failed` block.

## JSON field names

Codable structs map JSON keys to field names one to one. When an external API uses other names, a field
says so with `@JsonKey` (from `k.encoding`):

```k
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
| `respond(path:with:)` | answers requests to `path` with an `HttpResponse` |
| `requests: List<HttpRequest>` | every request sent so far |

```k
test WeatherService {
    @Inject let http: TestHttpClient
    @Inject let weather: WeatherService

    test "an upstream outage is reported without its body" {
        http.respond(path: "/v1/forecast", with: HttpResponse(status: .serviceUnavailable).body(.text("down")))

        expect throws WeatherError.upstream {
            try await weather.forecast(place: .berlin)
        }
    }
}
```

Any request with no canned response fails the test, naming the method and URL. An app can still write its
own fake, implementing only `send` (`fetchJson` is a default method, so the real decoding path runs in
tests), and choose it with `use FakeStripeApi for HttpClient`.
