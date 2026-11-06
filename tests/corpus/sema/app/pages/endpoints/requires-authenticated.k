// `requires Authenticated` doesn't apply to endpoint pages; `RateLimit` does.
import k.auth.Authenticated
import k.auth.RateLimit
import k.http.HttpResponse
import k.http.IncomingRequest

page Export requires Authenticated at "/export" {
    @Get
    func run(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

page Ping requires RateLimit(10.perMinute) at "/ping" {
    @Get
    func run(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
