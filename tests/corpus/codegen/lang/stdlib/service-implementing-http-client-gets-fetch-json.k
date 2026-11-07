// A service implementing the stdlib's `HttpClient` (a test fake) gets the
// interface's default `fetchJson`, over its own `send`, so code that
// injects `HttpClient` can call either method on it.
import k.http.HttpClient
import k.http.HttpError
import k.http.HttpRequest
import k.http.HttpResponse

service Canned implements HttpClient {
    func send(request: HttpRequest) async throws HttpError -> HttpResponse {
        return HttpResponse(status: .ok).body(.text("[]"))
    }
}

component Probe {
    @Inject let http: Canned

    view {
        text { "probe" }
    }
}
