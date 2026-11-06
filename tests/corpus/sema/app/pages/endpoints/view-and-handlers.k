// A page has either a view or handlers, never both.
import k.http.HttpResponse
import k.http.IncomingRequest

page Hybrid at "/hybrid" {
    @Get
    func run(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }

    view {
        text { "hybrid" }
    }
}
