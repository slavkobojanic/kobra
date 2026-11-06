# Proposal: `k.http` HttpClient

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`.

## Problem

[20-standard-library.md](../20-standard-library.md) lists "`HttpClient` for external APIs" in `k.http` but doesn't define its API, its errors, how it fits the layering rules, or how tests replace it.

## Proposal

### HttpClient is an interface

```k
public interface HttpClient {
    func send(request: HttpRequest) async throws HttpError -> HttpResponse

    /// Sends the request, requires a 2xx status, and decodes the body as JSON.
    func fetchJson<T>(request: HttpRequest) async throws HttpError -> T where T implements Codable {
        let response = try await send(request: request)

        guard response.isSuccess else {
            throw HttpError.status(code: response.status, body: response.body)
        }

        return try response.json()
    }
}
```

- The stdlib ships one implementation, `FetchHttpClient`, built on the web-standard `fetch` that both the browser and the embedded server runtime provide ([13-tooling.md](../13-tooling.md)). Since it is the only implementation, `@Inject let http: HttpClient` resolves to it with no `use` rule ([10-services.md](../10-services.md)).
- Tests replace it with `use FakeHttpClient for HttpClient`. A fake implements only `send`; `fetchJson` is a default method, so the real decoding path runs in tests.
- **Layering:** `HttpClient` is injectable only into services, like protocols. Components can't call external APIs directly, which keeps retries, caching and error wrapping in one place. This follows the note in [17-javascript-interop.md](../17-javascript-interop.md) that raw platform calls stay behind services.

### Requests are a `@Builder` struct

```k
let request = HttpRequest(method: .get, url: forecastUrl)
    .query(["latitude": "52.52", "longitude": "13.41"])
    .headers(["Accept": "application/json"])
    .timeout(10.seconds)
```

| Field | Kind | Default |
|---|---|---|
| `method: HttpMethod` (`.get`, `.post`, `.put`, `.patch`, `.delete`, `.head`) | required | |
| `url: Url` | required | |
| `query: Map<String, String>` | chained | empty |
| `headers: Map<String, String>` | chained | empty |
| `body: HttpBody?` (`.json(value)`, `.text(string)`, `.form(map)`) | chained | `nil` |
| `timeout: Duration` | chained | `30.seconds` |

- `Url("https://…")` built from a string literal is validated at compile time. Built from a runtime string, it is `Url.parse(text) -> Url?`.
- `url.path` and `url.host` are available, so fakes can route on them.

### Responses

`HttpResponse` has `status: Int`, `headers: Map<String, String>`, `body: String`, `isSuccess: Bool` (2xx), and `json<T>() throws HttpError -> T`.

### Errors

```k
public enum HttpError implements Error {
    case offline                            // DNS failure, refused connection, no network
    case timeout                            // the request's timeout elapsed
    case status(code: Int, body: String)    // a non-2xx response, from fetchJson
    case decoding(message: String)          // the body wasn't the expected JSON
}
```

- `send` itself never throws `.status`. A non-2xx response is still a response. Only the convenience `fetchJson` turns it into an error.
- **Cancellation isn't an error.** When the effect or `@Load` that started a request is cancelled ([03-reactivity.md](../03-reactivity.md)), the request is aborted and the awaiting code never resumes. There is no `.cancelled` case to handle.
- Following [09-errors.md](../09-errors.md), services wrap `HttpError` in their own error enum (`case network(HttpError)`) rather than exposing it.

### JSON field names

Codable structs map JSON keys to field names one-to-one. When an external API uses other names, a field says so with `@JsonKey` (from `k.encoding`):

```k
struct CurrentResponse implements Codable {
    @JsonKey("temperature_2m") let temperature: Double
}
```

There is no global naming strategy such as "convert snake_case", so every renamed field is visible where it is declared.

### Extensions

*First used by `examples/shop`.*

- **`url.appending(path:) -> Url`** joins a path onto a base URL held in configuration, for example `config.apiUrl.appending(path: "/v1/checkout/sessions")`, so a configurable base URL never needs string concatenation and re-parsing.
- **Requests a server receives** use the same `HttpRequest` type, with lowercased header names and `rawBody: Bytes` for signature checks. See [server-endpoints.md](server-endpoints.md).

## Why

- An interface gives tests a seam without a mocking library, using the existing `use X for Y`.
- A builder struct follows the "required named, optional chained" rule, so a request reads like an element.
- Typed errors with a small, closed set of cases keep `catch` exhaustive and useful.
