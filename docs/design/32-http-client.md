# HTTP Client

## Decision

External HTTP APIs are called through the **`HttpClient`** interface in `k.http`, with typed requests, responses, statuses and errors. Outgoing requests (`HttpRequest`) and requests an endpoint page receives (`IncomingRequest`, [27-endpoint-pages.md](27-endpoint-pages.md#the-incoming-request)) are separate types, because they have different members and different meanings for "body".

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

## Rules

- **`HttpClient` is a stdlib platform interface usable on both sides** ([10-services.md](10-services.md#layering)). It is injectable only into services, like protocols, so components can't call external APIs directly and retries, caching and error wrapping stay in one place.
- The stdlib ships one implementation, `FetchHttpClient`, built on the web-standard `fetch` that both the browser and the embedded server runtime provide ([13-tooling.md](13-tooling.md)). It is the only implementation, so `@Inject let http: HttpClient` resolves to it with no `use` rule.
- Following [09-errors.md](09-errors.md), services wrap `HttpError` in their own error enum (`case network(HttpError)`) rather than exposing it.

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

- **Secrets go into requests without `reveal()`.** `bearerToken` and `secretHeaders` take `Secret` values, which logging and error output redact ([25-config-and-secrets.md](25-config-and-secrets.md#secrett)).
- **`HttpBody`** is `.json(value)`, `.text(string)`, `.form(map)` or `.bytes(data, contentType: …)`. Each sets `Content-Type`.

### `Url`

- `Url("https://…")` takes a string literal, validated at compile time. Runtime text is parsed with `Url.parse(text) -> Url?` ([20-standard-library.md](20-standard-library.md#literals-and-parsing)).
- **`url.appending(path:) -> Url`** joins a path onto a base URL held in configuration, so a configurable base URL never needs string concatenation and re-parsing.
- `url.path` and `url.host` are available, so fakes can route on them.

## Responses

`HttpResponse` is a `@Builder` struct used both for responses a client receives and for responses an endpoint page sends:

| Member | Meaning |
|---|---|
| `HttpResponse(status: HttpStatus)` | required status; chained `.headers(Map<String, String>)` and `.body(HttpBody)` |
| `HttpResponse.redirect(to: Page)` | `303 See Other` to the page's URL |
| `HttpResponse.redirect(url: Url)` | `303 See Other` to an external URL |
| `status: HttpStatus` | the status |
| `isSuccess: Bool` | a 2xx status |
| `headers` | names lowercased. A repeated header reads as its values joined with `, `, except `set-cookie`; `headers.all(name)` returns every value. |
| `bodyBytes: Bytes` | the body exactly as received |
| `body: String` | the body as UTF-8 text |
| `json() throws HttpError -> T` | the body decoded as JSON into a `Codable` type |

### `HttpStatus`

- **Statuses are a typed `HttpStatus`,** not a bare `Int`, with implicit members for the standard codes: `.ok`, `.created`, `.noContent`, `.seeOther`, `.badRequest`, `.unauthorized`, `.forbidden`, `.notFound`, `.tooManyRequests`, `.internalServerError`, `.serviceUnavailable`, and the rest.
- `HttpStatus(code: 418)` takes an integer literal, validated at compile time to `100...599`. `status.code` is the number, and `status.isSuccess`, `status.isClientError` and `status.isServerError` classify it.

## Errors

```k
public enum HttpError implements Error {
    case offline                                // DNS failure, refused connection, no network
    case timeout                                // the request's timeout elapsed
    case status(HttpStatus, body: String)       // a non-2xx response, from fetchJson
    case decoding(message: String)              // the body wasn't the expected JSON
}
```

- `send` itself never throws `.status`. A non-2xx response is still a response. Only the convenience `fetchJson` turns it into an error.
- **Cancellation isn't an error.** When the effect or `@Load` that started a request is cancelled ([03-reactivity.md](03-reactivity.md)), the request is aborted and the awaiting code never resumes. There is no `.cancelled` case to handle.
- **`.status` carries a third party's response body.** An error that crosses to the browser (a `@Load` error, an rpc error) shouldn't include it; the compiler warns when it would ([14-async-data.md](14-async-data.md#server-rendering-and-hydration)). Wrap the status without the body (`case upstream(HttpStatus)`).

## JSON field names

Codable structs map JSON keys to field names one-to-one. When an external API uses other names, a field says so with `@JsonKey` (from `k.encoding`):

```k
struct CurrentResponse implements Codable {
    @JsonKey("temperature_2m") let temperature: Double
}
```

There is no global naming strategy such as "convert snake_case", so every renamed field is visible where it is declared.

## Tests

**`k test` installs `TestHttpClient`** (from `k.test`) for `HttpClient` by default ([21-testing.md](21-testing.md#test-doubles-and-defaults)), so a test that forgets a fake never reaches the internet:

| Member | Meaning |
|---|---|
| `respond(path:with:)` | answers requests to `path` with an `HttpResponse` |
| `requests: List<HttpRequest>` | every request sent so far |

Any request with no canned response fails the test, naming the method and URL. An app can still write its own fake, implementing only `send` (`fetchJson` is a default method, so the real decoding path runs in tests), and choose it with `use FakeStripeApi for HttpClient`.

## Why

- An interface gives tests a seam without a mocking library, using the existing `use X for Y`.
- Separate outgoing and incoming types mean no member exists where it is meaningless.
- A builder struct follows the "required named, optional chained" rule, so a request reads like an element.
- Typed statuses and a small, closed set of error cases keep `catch` exhaustive and useful.
