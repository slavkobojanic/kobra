import k.http.HttpClient
import k.http.HttpError
import k.http.HttpRequest
import k.http.HttpResponse
import k.http.HttpStatus

/// The default [`HttpClient`] in `k test`: it answers only requests it has a
/// canned response for, and fails the test on any other.
@TestDefault(replaces: HttpClient)
@Native
public service TestHttpClient implements HttpClient {
    /// Every request sent so far.
    var requests: List<HttpRequest>

    /// Answers the canned response for the request's path, or throws the
    /// error set by [`fail(path:error:)`]; a request with nothing canned
    /// fails the test.
    func send(request: HttpRequest) async throws HttpError -> HttpResponse
    /// Answers requests to `path` with `response`.
    func respond(path: String, with response: HttpResponse)
    /// Answers requests to `path` with `value` as JSON and status
    /// [`HttpStatus.ok`].
    func serveJson<T>(path: String, _ value: T) where T implements Codable
    /// Answers requests to `path` with `text` and `status`.
    func serveText(path: String, _ text: String, status: HttpStatus = .ok)
    /// Makes requests to `path` throw `error`.
    func fail(path: String, error: HttpError)
}
