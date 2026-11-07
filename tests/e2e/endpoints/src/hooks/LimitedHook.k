import k.auth.RateLimit
import k.http.HttpResponse
import k.http.IncomingRequest

/// A rate-limited endpoint: the second call in a minute is 429.
public page LimitedHook requires RateLimit(1.perMinute) at "/limited-hook" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}
