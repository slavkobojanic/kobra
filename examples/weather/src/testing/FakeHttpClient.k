import k.encoding.Json
import k.http.HttpClient
import k.http.HttpError
import k.http.HttpRequest
import k.http.HttpResponse

/// An `HttpClient` that answers from canned responses instead of the network,
/// and records every request it receives.
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
/// This module is test-only (see `docs/design/proposals/testing-support.md`).
public service FakeHttpClient implements HttpClient {
    private var responses: Map<String, HttpResponse> = Map()
    private var failures: Map<String, HttpError> = Map()

    /// Every request sent so far, oldest first.
    public var requests: List<HttpRequest> = []

    /// Answers requests for `path` with `body` encoded as JSON and status 200.
    ///
    /// - Parameters:
    ///   - path: the URL path to answer, for example `"/v1/search"`.
    ///   - body: the value to encode, usually one of the wire structs in
    ///     `weather/OpenMeteo.k`.
    public func serveJson<T>(path: String, body: T) where T implements Codable {
        responses[path] = HttpResponse(status: 200, body: Json.encode(body))
    }

    /// Answers requests for `path` with a raw text body, for responses that
    /// are not valid JSON or that carry an error status.
    ///
    /// - Parameters:
    ///   - path: the URL path to answer.
    ///   - body: the response body, sent as is.
    ///   - status: the HTTP status code.
    public func serveText(path: String, body: String, status: Int = 200) {
        responses[path] = HttpResponse(status: status, body: body)
    }

    /// Makes requests for `path` fail before any response arrives, as a
    /// dropped connection or a timeout would.
    ///
    /// - Parameters:
    ///   - path: the URL path that fails.
    ///   - error: the transport error to throw, such as `.timeout` or `.offline`.
    public func fail(path: String, error: HttpError) {
        failures[path] = error
    }

    /// Records the request, then throws the configured failure or returns the
    /// configured response for its path.
    public func send(request: HttpRequest) async throws HttpError -> HttpResponse {
        requests.append(request)

        if let failure = failures[request.url.path] {
            throw failure
        }

        guard let response = responses[request.url.path] else {
            return HttpResponse(status: 404, body: "No canned response for {request.url.path}")
        }

        return response
    }
}
