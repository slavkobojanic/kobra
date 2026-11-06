import k.http.HttpClient
import k.http.HttpError
import k.http.HttpRequest
import k.http.HttpResponse
import k.http.HttpStatus

/// An `HttpClient` that answers from canned responses instead of the network,
/// records every request it receives, and can fail a request before any
/// response arrives.
///
/// `k test` already installs `TestHttpClient` for `HttpClient` by default.
/// This fake is chosen instead because `WeatherService` must also be tested
/// against dropped connections and timeouts, which only a fake can throw.
///
/// Responses are keyed by URL path (`"/v1/forecast"`), which is enough to tell
/// the Open-Meteo endpoints apart. A path with no canned response answers
/// `404 Not Found`, so a test that forgets to set one up fails loudly.
///
/// ```k
/// test WeatherService {
///     use FakeHttpClient for HttpClient
///
///     @Inject let http: FakeHttpClient
/// }
/// ```
///
/// This module is test-only (see `docs/design/21-testing.md`).
public service FakeHttpClient implements HttpClient {
    private var responses: Map<String, HttpResponse> = Map()
    private var failures: Map<String, HttpError> = Map()

    /// Every request sent so far, oldest first.
    var requests: List<HttpRequest> = []

    /// Answers requests for `path` with `body` encoded as JSON and status 200.
    ///
    /// - Parameters:
    ///   - path: the URL path to answer, for example `"/v1/search"`.
    ///   - body: the value to encode, usually one of the wire structs in
    ///     `weather/OpenMeteo.k`.
    func serveJson<T>(path: String, body: T) where T implements Codable {
        responses[path] = HttpResponse(status: .ok).body(.json(body))
    }

    /// Answers requests for `path` with a raw text body, for responses that
    /// are not valid JSON or that carry an error status.
    ///
    /// - Parameters:
    ///   - path: the URL path to answer.
    ///   - body: the response body, sent as is.
    ///   - status: the HTTP status.
    func serveText(path: String, body: String, status: HttpStatus = .ok) {
        responses[path] = HttpResponse(status: status).body(.text(body))
    }

    /// Makes requests for `path` fail before any response arrives, as a
    /// dropped connection or a timeout would.
    ///
    /// - Parameters:
    ///   - path: the URL path that fails.
    ///   - error: the transport error to throw, `.timeout` or `.offline`.
    func fail(path: String, error: HttpError) {
        failures[path] = error
    }

    /// Records the request, then throws the configured failure or returns the
    /// configured response for its path.
    func send(request: HttpRequest) async throws HttpError -> HttpResponse {
        requests.append(request)

        if let failure = failures[request.url.path] {
            throw failure
        }

        guard let response = responses[request.url.path] else {
            return HttpResponse(status: .notFound).body(.text("No canned response for {request.url.path}"))
        }

        return response
    }
}
